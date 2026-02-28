//! API handlers for builder management and work queue operations.
//!
//! This module provides two sets of endpoints:
//! 1. Builder Management (Admin-only): CRUD operations for builders
//! 2. Builder Work Queue (Builder-authenticated): Job polling and status updates

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::handlers::api::rbac::require_admin;
use crate::handlers::agent_request::CFState;
use crate::handlers::builder_request::{authenticate_builder_request, VerifiedBuilderRequest};
use crate::models::builders::{
    Builder, BuilderMetrics, BuilderSummary, BuilderWithEnvironments, CreateBuilderRequest,
    ReportMetricsRequest, UpdateBuilderEnvironmentsRequest, UpdateBuilderPublicKeyRequest,
    UpdateBuilderRequest,
};
use crate::queries::builders;

// =============================================================================
// BUILDER MANAGEMENT ENDPOINTS (Admin-only)
// =============================================================================

/// POST /api/v1/builders - Create a new builder (admin-only)
pub async fn create_builder(
    State(state): State<CFState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<CreateBuilderRequest>,
) -> Result<Json<Builder>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Create builder
    let builder = builders::create_builder(&state.pool, &request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(builder))
}

/// GET /api/v1/builders - List all builders (admin-only)
pub async fn list_builders(
    State(state): State<CFState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<BuilderSummary>>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // List builders
    let builders_list = builders::list_builders(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(builders_list))
}

/// GET /api/v1/builders/:id - Get builder details (admin-only)
pub async fn get_builder(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<BuilderWithEnvironments>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Get builder with environments
    let builder = builders::get_builder_with_environments(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(builder))
}

/// PATCH /api/v1/builders/:id - Update builder config (admin-only)
pub async fn update_builder(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(request): Json<UpdateBuilderRequest>,
) -> Result<Json<Builder>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Update builder
    let builder = builders::update_builder(&state.pool, &builder_id, &request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(builder))
}

/// DELETE /api/v1/builders/:id - Deactivate builder (admin-only)
pub async fn deactivate_builder(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Builder>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Deactivate builder
    let builder = builders::deactivate_builder(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(builder))
}

/// PUT /api/v1/builders/:id/public-key - Update builder public key (admin-only)
pub async fn update_builder_public_key(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(request): Json<UpdateBuilderPublicKeyRequest>,
) -> Result<Json<Builder>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Get builder name for validation
    let existing = builders::get_builder_by_id(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Update public key
    let builder =
        builders::update_builder_public_key(&state.pool, &builder_id, &request.public_key, &existing.name)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(builder))
}

/// PATCH /api/v1/builders/:id/environments - Update environment assignments (admin-only)
pub async fn update_builder_environments(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(request): Json<UpdateBuilderEnvironmentsRequest>,
) -> Result<StatusCode, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Update environments
    builders::update_builder_environments(&state.pool, &builder_id, &request.environment_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/builders/:id/metrics - Get builder metrics (admin-only)
pub async fn get_builder_metrics(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<BuilderMetrics>>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Get recent metrics (last 100 data points)
    let metrics = builders::get_builder_metrics(&state.pool, &builder_id, 100)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(metrics))
}

// =============================================================================
// BUILDER WORK QUEUE ENDPOINTS (Builder-authenticated)
// =============================================================================

#[derive(Debug, Serialize)]
pub struct HeartbeatResponse {
    pub status: String,
    pub message: String,
}

/// POST /api/v1/builders/:id/heartbeat - Builder heartbeat with metrics
pub async fn builder_heartbeat(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Json<HeartbeatResponse>, StatusCode> {
    // Authenticate builder request
    let verified = authenticate_builder_request(&headers, body.clone(), &state.pool).await?;

    // Verify the builder_id in the path matches the authenticated builder
    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Parse metrics from body
    let metrics: ReportMetricsRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Update heartbeat timestamp (marks builder as active)
    builders::update_builder_heartbeat(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Record metrics
    builders::record_builder_metrics(&state.pool, &builder_id, &metrics)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(HeartbeatResponse {
        status: "ok".to_string(),
        message: "Heartbeat recorded".to_string(),
    }))
}

#[derive(Debug, Serialize)]
pub struct NextJobResponse {
    pub job_id: Option<Uuid>,
    pub derivation_id: Option<i32>,
    pub message: String,
}

/// GET /api/v1/builders/:id/next-job - Get next job for builder
///
/// This endpoint implements the load-based job assignment logic:
/// 1. Filter jobs by builder's environment assignments (or all if no assignments)
/// 2. Check builder's current concurrency limit
/// 3. Return highest-priority queued job if available
pub async fn get_next_job(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Json<NextJobResponse>, StatusCode> {
    // Authenticate builder request
    let verified = authenticate_builder_request(&headers, body, &state.pool).await?;

    // Verify the builder_id matches
    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // TODO: Implement job assignment logic (Phase 4)
    // For now, return no jobs available
    Ok(Json(NextJobResponse {
        job_id: None,
        derivation_id: None,
        message: "No jobs available (job assignment not yet implemented)".to_string(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct JobStatusRequest {
    pub status: String,
    pub error_message: Option<String>,
}

/// POST /api/v1/builders/:id/jobs/:job_id/start - Mark job as started
pub async fn start_job(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request
    let verified = authenticate_builder_request(&headers, body, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // TODO: Implement job start logic (Phase 4)
    // Update build_jobs status to 'building', set started_at timestamp

    Ok(StatusCode::ACCEPTED)
}

/// POST /api/v1/builders/:id/jobs/:job_id/complete - Mark job as complete
pub async fn complete_job(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request
    let verified = authenticate_builder_request(&headers, body, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // TODO: Implement job completion logic (Phase 4)
    // Update build_jobs status to 'success', set completed_at timestamp

    Ok(StatusCode::OK)
}

/// POST /api/v1/builders/:id/jobs/:job_id/fail - Mark job as failed
pub async fn fail_job(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request
    let verified = authenticate_builder_request(&headers, body.clone(), &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Parse failure details
    let _request: JobStatusRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // TODO: Implement job failure logic with retry (Phase 5)
    // Increment retry_count, re-queue if under max_retries, adjust priority

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize)]
pub struct AppendLogsRequest {
    pub logs: String,
}

/// POST /api/v1/builders/:id/jobs/:job_id/logs - Append build logs
pub async fn append_job_logs(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request
    let verified = authenticate_builder_request(&headers, body.clone(), &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Parse log content
    let _request: AppendLogsRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // TODO: Implement log appending logic (Phase 4)
    // Append to build_jobs.logs field

    Ok(StatusCode::ACCEPTED)
}
