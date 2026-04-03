//! API handlers for builder management and work queue operations.
//!
//! This module provides two sets of endpoints:
//! 1. Builder Management (Admin-only): CRUD operations for builders
//! 2. Builder Work Queue (Builder-authenticated): Job polling and status updates

use axum::{
    Json,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use bytes::Bytes;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::{require_admin, require_viewer_or_above};
use crate::handlers::builder_request::{
    authenticate_builder_request, authenticate_builder_request_allow_inactive,
};
use crate::models::builders::{
    AppendLogsRequest, BuildJob, Builder, BuilderCreatedResponse, BuilderMetrics, BuilderSummary,
    BuilderWithEnvironments, CreateBuilderRequest, KeypairRegeneratedResponse,
    ReportMetricsRequest, UpdateBuilderEnvironmentsRequest, UpdateBuilderPublicKeyRequest,
    UpdateBuilderRequest,
};
use crate::queries::builders;

// =============================================================================
// BUILDER MANAGEMENT ENDPOINTS (Admin-only)
// =============================================================================

/// POST /api/v1/builders - Create a new builder (admin-only)
///
/// If `public_key` is not provided in request, server generates a proper Ed25519 keypair.
/// Response includes the private key ONLY if generated server-side.
///
/// WARNING: Save the private key immediately - it's shown only once!
pub async fn create_builder(
    State(state): State<CFState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<CreateBuilderRequest>,
) -> Result<Json<BuilderCreatedResponse>, (StatusCode, String)> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err((StatusCode::FORBIDDEN, "Admin access required".to_string()));
    };

    // Validate request fields (input sanitization)
    if request.name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Builder name cannot be empty".to_string(),
        ));
    }
    if request.name.len() > 255 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Builder name too long (max 255 characters)".to_string(),
        ));
    }

    // Validate public key if provided (prevent DoS via oversized input)
    if let Some(ref pk) = request.public_key {
        if pk.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Public key cannot be empty".to_string(),
            ));
        }
        if pk.len() > 1000 {
            return Err((
                StatusCode::BAD_REQUEST,
                "Public key too long (max 1000 characters)".to_string(),
            ));
        }
    }

    // Create builder (may generate keypair server-side)
    // PublicKey::from_base64() will validate:
    // - Base64 decoding
    // - Exactly 32 bytes (Ed25519 requirement)
    // - Valid Ed25519 curve point
    let (builder, private_key_option) = builders::create_builder(&state.pool, &request)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create builder: {}", e);
            map_create_builder_error(&e)
        })?;

    // Get environment IDs for response
    let assigned_environment_ids = request.environment_ids.clone();

    Ok(Json(BuilderCreatedResponse {
        builder,
        private_key: private_key_option,
        assigned_environment_ids,
    }))
}

fn map_create_builder_error(error: &anyhow::Error) -> (StatusCode, String) {
    let message = error.to_string();

    if message.contains("Invalid public key")
        || message.contains("must be exactly 32 bytes")
        || message.contains("Failed to decode base64")
    {
        return (
            StatusCode::BAD_REQUEST,
            format!("Invalid public key: {}", error),
        );
    }

    if message.contains("builders_name_key")
        || (message.contains("duplicate key value violates unique constraint")
            && message.contains("builders"))
    {
        return (
            StatusCode::CONFLICT,
            "Builder name already exists".to_string(),
        );
    }

    if message.contains("builder_environment_assignments_environment_id_fkey")
        || (message.contains("violates foreign key constraint")
            && message.contains("environment_id"))
    {
        return (
            StatusCode::BAD_REQUEST,
            "One or more selected environments do not exist".to_string(),
        );
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Failed to create builder".to_string(),
    )
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

/// DELETE /api/v1/builders/:id/permanent - Permanently delete builder (admin-only)
pub async fn delete_builder_permanently(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    builders::delete_builder(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
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
    let builder = builders::update_builder_public_key(
        &state.pool,
        &builder_id,
        &request.public_key,
        &existing.name,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(builder))
}

/// POST /api/v1/builders/:id/regenerate-keypair - Generate new Ed25519 keypair for builder (admin-only)
///
/// Generates a cryptographically correct Ed25519 keypair and updates the builder's public key.
/// Returns the new private key ONCE - save it immediately, it won't be shown again!
pub async fn regenerate_builder_keypair(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<KeypairRegeneratedResponse>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Check builder exists
    let existing = builders::get_builder_by_id(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Generate new Ed25519 keypair
    let (public_key_base64, private_key_base64) =
        builders::generate_ed25519_keypair().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update builder's public key
    builders::update_builder_public_key(
        &state.pool,
        &builder_id,
        &public_key_base64,
        &existing.name,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Return keypair (private key shown ONLY ONCE)
    Ok(Json(KeypairRegeneratedResponse {
        public_key: public_key_base64,
        private_key: private_key_base64,
    }))
}

/// POST /api/v1/build-jobs/:id/prioritize - Move queued build job to front (admin-only)
pub async fn prioritize_build_job(
    State(state): State<CFState>,
    Path(job_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    builders::prioritize_build_job(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(StatusCode::OK)
}

/// POST /api/v1/build-jobs/:id/cancel - Cancel/stop a build job (admin-only)
pub async fn cancel_build_job(
    State(state): State<CFState>,
    Path(job_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<BuildJob>, (StatusCode, String)> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err((StatusCode::FORBIDDEN, "Admin access required".to_string()));
    };

    builders::cancel_build_job(&state.pool, &job_id)
        .await
        .map(Json)
        .map_err(|e| {
            let message = e.to_string();
            if message.to_lowercase().contains("not found") {
                (StatusCode::NOT_FOUND, message)
            } else {
                (StatusCode::BAD_REQUEST, message)
            }
        })
}

/// GET /api/v1/build-jobs - Paginated build queue with filtering (viewer+)
///
/// Query parameters (all optional):
/// - `page` (default 1), `limit` (default 50, max 200)
/// - `status`: comma-separated statuses to include (queued, building, success, failed)
/// - `commit_hash`: prefix match on git commit hash
/// - `flake_name`: partial match on flake name
/// - `config_name`: partial match on system hostname / config name
/// - `queued_after`, `queued_before`: ISO-8601 timestamps bounding queued_at
pub async fn list_build_queue(
    State(state): State<CFState>,
    headers: HeaderMap,
    Query(params): Query<crate::api::models::BuildQueueParams>,
) -> Result<Json<crate::api::models::BuildQueuePageResponse>, StatusCode> {
    let Some(_viewer) = require_viewer_or_above(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    let result = crate::queries::dashboard::list_build_queue_paginated(&state.pool, &params)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list build queue: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}

/// GET /api/v1/build-jobs/recent - Recent completed/failed builds (viewer+)
pub async fn list_recent_build_jobs(
    State(state): State<CFState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::api::models::BuildQueueItem>>, StatusCode> {
    let Some(_viewer) = require_viewer_or_above(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    let items = crate::queries::dashboard::fetch_recent_build_history(&state.pool, 100)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(items))
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
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/heartbeat", builder_id);
    let verified = authenticate_builder_request_allow_inactive(
        &headers,
        body.clone(),
        "POST",
        &path,
        &state.pool,
    )
    .await?;

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
) -> Result<Json<BuildJob>, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/next-job", builder_id);
    let verified = authenticate_builder_request(&headers, body, "GET", &path, &state.pool).await?;

    // Verify the builder_id matches
    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Get builder to check max_concurrent_jobs
    let builder = builders::get_builder_by_id(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Get builder's environment assignments (empty = wildcard)
    let environment_ids = builders::get_builder_environment_ids(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // TASK-147: Atomically claim next job with race-free concurrency enforcement
    // This single transaction ensures count check + job assignment are atomic,
    // preventing multiple builders from exceeding their max_concurrent_jobs limit
    let job = builders::claim_next_job_atomic(
        &state.pool,
        &builder_id,
        builder.max_concurrent_jobs,
        &environment_ids,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to claim job atomically: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(job) = job {
        // Job successfully claimed (already marked as 'building')
        Ok(Json(job))
    } else {
        // Either no jobs available OR builder at capacity
        // Return 404 NOT_FOUND so builder knows to wait
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Debug, Deserialize)]
pub struct JobStatusRequest {
    pub status: String,
    pub error_message: Option<String>,
}

/// POST /api/v1/builders/:id/jobs/:job_id/start - Mark job as started
///
/// Note: This is a no-op since get_next_job already marks the job as building.
/// Kept for API consistency and future extensibility.
pub async fn start_job(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/jobs/{}/start", builder_id, job_id);
    let verified = authenticate_builder_request(&headers, body, "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Verify the job exists and is assigned to this builder
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if job.builder_id != Some(builder_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Job already marked as building by get_next_job
    Ok(StatusCode::ACCEPTED)
}

/// POST /api/v1/builders/:id/jobs/:job_id/complete - Mark job as complete
pub async fn complete_job(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/jobs/{}/complete", builder_id, job_id);
    let verified = authenticate_builder_request(&headers, body, "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Verify the job is assigned to this builder
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if job.builder_id != Some(builder_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Mark job as complete
    builders::mark_job_complete(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    cleanup_build_log_channel(&state, job_id).await;

    Ok(StatusCode::OK)
}

/// POST /api/v1/builders/:id/jobs/:job_id/fail - Mark job as failed
///
/// Implements retry logic:
/// - If retry_count < max_retries: increment retry_count, re-queue job, reduce priority
/// - If retry_count >= max_retries: mark as permanently failed
pub async fn fail_job(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/jobs/{}/fail", builder_id, job_id);
    let verified =
        authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Parse failure details
    let request: JobStatusRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Verify the job is assigned to this builder
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if job.builder_id != Some(builder_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Mark job as failed with retry logic
    let updated_job = builders::mark_job_failed_with_retry(
        &state.pool,
        &job_id,
        request.error_message.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Return 200 for re-queued jobs, 202 for permanently failed jobs
    if updated_job.status == "queued" {
        Ok(StatusCode::OK) // Job re-queued for retry
    } else {
        cleanup_build_log_channel(&state, job_id).await;
        Ok(StatusCode::ACCEPTED) // Job permanently failed
    }
}

/// POST /api/v1/builders/:id/jobs/:job_id/logs - Append build logs
pub async fn append_job_logs(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let max_chunk_bytes = state.server_config.max_build_log_chunk_mb * 1024 * 1024;
    let max_total_bytes = state.server_config.max_build_log_size_mb * 1024 * 1024;

    // Enforce per-request payload size limit before parsing JSON.
    if body.len() > max_chunk_bytes {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Log payload too large: {} bytes exceeds {} byte limit",
                body.len(),
                max_chunk_bytes
            ),
        ));
    }

    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/jobs/{}/logs", builder_id, job_id);
    let verified = authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool)
        .await
        .map_err(|status| {
            (
                status,
                "Builder authentication failed for log append request".to_string(),
            )
        })?;

    if verified.builder_id != builder_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Builder ID mismatch in log append request".to_string(),
        ));
    }

    // Parse log content
    let request: AppendLogsRequest = serde_json::from_slice(&body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid log append payload: expected JSON with 'logs' string field".to_string(),
        )
    })?;

    if request.logs.len() > max_chunk_bytes {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Log chunk too large: {} bytes exceeds {} byte limit",
                request.logs.len(),
                max_chunk_bytes
            ),
        ));
    }

    // Verify the job is assigned to this builder
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load build job for log append".to_string(),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Build job not found for log append".to_string(),
        ))?;

    if job.builder_id != Some(builder_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Builder cannot append logs for a job assigned to another builder".to_string(),
        ));
    }

    // Only queued/building jobs may receive log appends.
    if job.status != "queued" && job.status != "building" {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Cannot append logs for job in '{}' status; only 'queued' and 'building' are allowed",
                job.status
            ),
        ));
    }

    // Append logs with per-job size cap enforcement.
    builders::append_job_logs_with_limits(&state.pool, &job_id, &request.logs, max_total_bytes)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("log_size_limit_exceeded") {
                (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    format!("Total job logs would exceed {} byte limit", max_total_bytes),
                )
            } else if msg.contains("invalid_job_status") {
                (
                    StatusCode::CONFLICT,
                    "Cannot append logs for job in current status".to_string(),
                )
            } else if msg.contains("job_not_found") {
                (
                    StatusCode::NOT_FOUND,
                    "Build job not found for log append".to_string(),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to append logs due to internal server error".to_string(),
                )
            }
        })?;

    if let Some(tx) = get_or_create_build_log_channel(&state, job_id).await {
        let log_msg = BuildStreamMessage::Log {
            message: request.logs.clone(),
        };
        record_build_stream_message(&state, job_id, &log_msg).await;
        let _ = broadcast_build_stream_message(&tx, &log_msg);
    }

    Ok((
        StatusCode::ACCEPTED,
        format!(
            "Log chunk accepted ({} bytes). Max per-chunk: {} bytes, max total per job: {} bytes",
            request.logs.len(),
            max_chunk_bytes,
            max_total_bytes
        ),
    ))
}

// =============================================================================
// WEBSOCKET LOG STREAMING
// =============================================================================

const BUILD_LOG_WS_CHANNEL_BUFFER: usize = 1024;
const MAX_BUILD_LOG_WS_CHANNELS: usize = 2048;
const BUILD_LOG_HISTORY_BUFFER: usize = 4000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BuildStreamMessage {
    Log {
        message: String,
    },
    Metrics {
        cpu_percent: f32,
        ram_used_mb: u64,
        ram_total_mb: u64,
        timestamp: String,
    },
    Error {
        message: String,
    },
}

enum BuildLogStreamPrincipal {
    Viewer,
    Builder(Uuid),
}

/// WebSocket endpoint for real-time build log streaming
/// GET /api/v1/build-jobs/:job_id/logs/stream
///
/// This endpoint allows clients (UI or builders) to stream logs in real-time.
/// Builders send log lines, UI clients receive them.
///
/// Message Format:
/// - Text messages from builder -> stored as logs in database
/// - Text messages to clients -> broadcast log lines
/// - JSON messages -> system metrics (CPU/RAM usage)
pub async fn stream_build_logs(
    ws: WebSocketUpgrade,
    Path(job_id): Path<Uuid>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let principal = match authorize_build_log_stream(&state, &headers, job_id).await {
        Ok(principal) => principal,
        Err(status) => return status.into_response(),
    };

    ws.on_upgrade(move |socket| handle_log_stream(socket, job_id, state, principal))
}

async fn authorize_build_log_stream(
    state: &CFState,
    headers: &HeaderMap,
    job_id: Uuid,
) -> Result<BuildLogStreamPrincipal, StatusCode> {
    if require_viewer_or_above(&state.pool, headers)
        .await
        .is_some()
    {
        return Ok(BuildLogStreamPrincipal::Viewer);
    }

    let path = format!("/api/v1/build-jobs/{}/logs/stream", job_id);
    let verified = authenticate_builder_request(headers, Bytes::new(), "GET", &path, &state.pool)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;

    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if job.builder_id != Some(verified.builder_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(BuildLogStreamPrincipal::Builder(verified.builder_id))
}

async fn handle_log_stream(
    mut socket: WebSocket,
    job_id: Uuid,
    state: CFState,
    principal: BuildLogStreamPrincipal,
) {
    tracing::info!("WebSocket connection established for job {}", job_id);

    let Some(tx) = get_or_create_build_log_channel(&state, job_id).await else {
        let _ = socket
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1013,
                reason: "Server overloaded".into(),
            })))
            .await;
        return;
    };

    let history_snapshot = {
        let history = state.build_log_history.lock().await;
        history.get(&job_id).cloned().unwrap_or_default()
    };

    for frame in history_snapshot {
        if let Err(e) = socket.send(Message::Text(frame.into())).await {
            tracing::debug!(
                "Failed to replay build log history to websocket for job {}: {}",
                job_id,
                e
            );
            return;
        }
    }

    match principal {
        BuildLogStreamPrincipal::Viewer => {
            let mut rx = tx.subscribe();
            while let Ok(frame) = rx.recv().await {
                if let Err(e) = socket.send(Message::Text(frame)).await {
                    tracing::debug!(
                        "Viewer build-log websocket closed for job {}: {}",
                        job_id,
                        e
                    );
                    break;
                }
            }
        }
        BuildLogStreamPrincipal::Builder(builder_id) => {
            let max_chunk_bytes = state.server_config.max_build_log_chunk_mb * 1024 * 1024;
            let max_total_bytes = state.server_config.max_build_log_size_mb * 1024 * 1024;

            while let Some(msg) = socket.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if text.len() > max_chunk_bytes {
                            let error = BuildStreamMessage::Error {
                                message: format!(
                                    "stream frame too large: {} bytes exceeds {}",
                                    text.len(),
                                    max_chunk_bytes
                                ),
                            };
                            let _ = send_build_stream_message(&mut socket, &error).await;
                            break;
                        }

                        let parsed = match serde_json::from_str::<BuildStreamMessage>(&text) {
                            Ok(message) => message,
                            Err(_) => {
                                let error = BuildStreamMessage::Error {
                                    message:
                                        "invalid websocket payload; expected typed JSON message"
                                            .to_string(),
                                };
                                let _ = send_build_stream_message(&mut socket, &error).await;
                                break;
                            }
                        };

                        match parsed {
                            BuildStreamMessage::Log { message } => {
                                if let Err(e) = builders::append_job_logs_with_limits(
                                    &state.pool,
                                    &job_id,
                                    &message,
                                    max_total_bytes,
                                )
                                .await
                                {
                                    tracing::error!(
                                        "Failed to append log over WS for job {} from builder {}: {}",
                                        job_id,
                                        builder_id,
                                        e
                                    );
                                    let error = BuildStreamMessage::Error {
                                        message: "failed to persist log frame".to_string(),
                                    };
                                    let _ = send_build_stream_message(&mut socket, &error).await;
                                    break;
                                }

                                let log_msg = BuildStreamMessage::Log { message };
                                record_build_stream_message(&state, job_id, &log_msg).await;
                                let _ = broadcast_build_stream_message(&tx, &log_msg);
                            }
                            BuildStreamMessage::Metrics {
                                cpu_percent,
                                ram_used_mb,
                                ram_total_mb,
                                timestamp,
                            } => {
                                let metrics_msg = BuildStreamMessage::Metrics {
                                    cpu_percent,
                                    ram_used_mb,
                                    ram_total_mb,
                                    timestamp,
                                };
                                record_build_stream_message(&state, job_id, &metrics_msg).await;
                                let _ = broadcast_build_stream_message(&tx, &metrics_msg);
                            }
                            BuildStreamMessage::Error { .. } => {
                                let error = BuildStreamMessage::Error {
                                    message: "clients cannot send error frames".to_string(),
                                };
                                let _ = send_build_stream_message(&mut socket, &error).await;
                                break;
                            }
                        }
                    }
                    Ok(Message::Ping(data)) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(e) => {
                        tracing::debug!(
                            "Builder build-log websocket error for job {} (builder {}): {}",
                            job_id,
                            builder_id,
                            e
                        );
                        break;
                    }
                }
            }
        }
    }

    tracing::info!("WebSocket connection closed for job {}", job_id);
}

fn broadcast_build_stream_message(
    tx: &tokio::sync::broadcast::Sender<String>,
    msg: &BuildStreamMessage,
) -> Result<(), serde_json::Error> {
    let json = serde_json::to_string(msg)?;
    let _ = tx.send(json);
    Ok(())
}

async fn send_build_stream_message(
    socket: &mut WebSocket,
    msg: &BuildStreamMessage,
) -> Result<(), ()> {
    let json = match serde_json::to_string(msg) {
        Ok(json) => json,
        Err(_) => return Err(()),
    };
    socket.send(Message::Text(json)).await.map_err(|_| ())
}

async fn get_or_create_build_log_channel(
    state: &CFState,
    job_id: Uuid,
) -> Option<tokio::sync::broadcast::Sender<String>> {
    let mut channels = state.build_log_channels.lock().await;
    if let Some(tx) = channels.get(&job_id) {
        return Some(tx.clone());
    }

    if channels.len() >= MAX_BUILD_LOG_WS_CHANNELS {
        return None;
    }

    let (tx, _rx) = tokio::sync::broadcast::channel(BUILD_LOG_WS_CHANNEL_BUFFER);
    channels.insert(job_id, tx.clone());
    Some(tx)
}

async fn cleanup_build_log_channel(state: &CFState, job_id: Uuid) {
    let mut channels = state.build_log_channels.lock().await;
    channels.remove(&job_id);
    drop(channels);

    let mut history = state.build_log_history.lock().await;
    history.remove(&job_id);
}

async fn record_build_stream_message(state: &CFState, job_id: Uuid, msg: &BuildStreamMessage) {
    if let Ok(json) = serde_json::to_string(msg) {
        let mut history = state.build_log_history.lock().await;
        let entry = history.entry(job_id).or_default();
        entry.push(json);
        if entry.len() > BUILD_LOG_HISTORY_BUFFER {
            let overflow = entry.len() - BUILD_LOG_HISTORY_BUFFER;
            entry.drain(0..overflow);
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use axum::http::StatusCode;

    use super::BuildStreamMessage;
    use super::map_create_builder_error;

    #[test]
    fn build_stream_requires_explicit_type_discriminator() {
        let ambiguous_metrics_json =
            r#"{"cpu_percent":10.0,"ram_used_mb":100,"ram_total_mb":200,"timestamp":"t"}"#;

        let parsed = serde_json::from_str::<BuildStreamMessage>(ambiguous_metrics_json);
        assert!(
            parsed.is_err(),
            "untagged JSON should not be accepted as a valid stream frame"
        );
    }

    #[test]
    fn create_builder_duplicate_name_maps_to_conflict() {
        let error = anyhow!("duplicate key value violates unique constraint \"builders_name_key\"");
        let (status, body) = map_create_builder_error(&error);

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body, "Builder name already exists");
    }

    #[test]
    fn create_builder_invalid_environment_maps_to_bad_request() {
        let error = anyhow!(
            "insert or update on table \"builder_environment_assignments\" violates foreign key constraint \"builder_environment_assignments_environment_id_fkey\"",
        );
        let (status, body) = map_create_builder_error(&error);

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, "One or more selected environments do not exist");
    }

    #[test]
    fn create_builder_invalid_public_key_maps_to_bad_request() {
        let error = anyhow!("Invalid public key format");
        let (status, body) = map_create_builder_error(&error);

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.starts_with("Invalid public key:"));
    }

    #[test]
    fn create_builder_unexpected_error_maps_to_internal_server_error() {
        let error = anyhow!("database connection timeout");
        let (status, body) = map_create_builder_error(&error);

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, "Failed to create builder");
    }
}
