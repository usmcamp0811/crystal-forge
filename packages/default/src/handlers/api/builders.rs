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
use crate::handlers::builder_request::{
    authenticate_builder_request, authenticate_builder_request_allow_inactive,
    VerifiedBuilderRequest,
};
use crate::models::builders::{
    AppendLogsRequest, Builder, BuilderCreatedResponse, BuilderMetrics, BuilderSummary, 
    BuilderWithEnvironments, CreateBuilderRequest, KeypairRegeneratedResponse, ReportMetricsRequest, UpdateBuilderEnvironmentsRequest,
    UpdateBuilderPublicKeyRequest, UpdateBuilderRequest,
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
        return Err((StatusCode::BAD_REQUEST, "Builder name cannot be empty".to_string()));
    }
    if request.name.len() > 255 {
        return Err((StatusCode::BAD_REQUEST, "Builder name too long (max 255 characters)".to_string()));
    }
    
    // Validate public key if provided (prevent DoS via oversized input)
    if let Some(ref pk) = request.public_key {
        if pk.is_empty() {
            return Err((StatusCode::BAD_REQUEST, "Public key cannot be empty".to_string()));
        }
        if pk.len() > 1000 {
            return Err((StatusCode::BAD_REQUEST, "Public key too long (max 1000 characters)".to_string()));
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
            // Return validation errors as 400, others as 500
            if e.to_string().contains("Invalid public key") || 
               e.to_string().contains("must be exactly 32 bytes") ||
               e.to_string().contains("Failed to decode base64") {
                (StatusCode::BAD_REQUEST, format!("Invalid public key: {}", e))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create builder".to_string())
            }
        })?;
    
    // Get environment IDs for response
    let assigned_environment_ids = request.environment_ids.clone();
    
    // If keypair was generated server-side, private_key will be Some(...)
    // This is returned ONCE and never stored
    let private_key = private_key_option.unwrap_or_else(|| {
        // Client provided public key - no private key to return
        "".to_string()
    });

    Ok(Json(BuilderCreatedResponse {
        builder,
        private_key,
        assigned_environment_ids,
    }))
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
    let builder =
        builders::update_builder_public_key(&state.pool, &builder_id, &request.public_key, &existing.name)
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
    let (public_key_base64, private_key_base64) = builders::generate_ed25519_keypair()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update builder's public key
    builders::update_builder_public_key(&state.pool, &builder_id, &public_key_base64, &existing.name)
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
    let verified = authenticate_builder_request_allow_inactive(&headers, body.clone(), "POST", &path, &state.pool).await?;

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
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/jobs/next", builder_id);
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

    // Check current active jobs vs limit
    let active_count = builders::count_active_jobs_for_builder(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if active_count >= builder.max_concurrent_jobs as i64 {
        // Builder at capacity
        return Ok(Json(NextJobResponse {
            job_id: None,
            derivation_id: None,
            message: "Builder at max concurrent job limit".to_string(),
        }));
    }

    // Get builder's environment assignments (empty = wildcard)
    let environment_ids = builders::get_builder_environment_ids(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get next queued job
    let job = builders::get_next_queued_job(&state.pool, &environment_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(job) = job {
        // Assign job to builder
        let assigned_job = builders::assign_job_to_builder(&state.pool, &job.id, &builder_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(Json(NextJobResponse {
            job_id: Some(assigned_job.id),
            derivation_id: Some(assigned_job.derivation_id),
            message: "Job assigned".to_string(),
        }))
    } else {
        // No jobs available
        Ok(Json(NextJobResponse {
            job_id: None,
            derivation_id: None,
            message: "No jobs available".to_string(),
        }))
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
    let verified = authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;

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
        Ok(StatusCode::ACCEPTED) // Job permanently failed
    }
}

/// POST /api/v1/builders/:id/jobs/:job_id/logs - Append build logs
pub async fn append_job_logs(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/jobs/{}/logs", builder_id, job_id);
    let verified = authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Parse log content
    let request: AppendLogsRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Verify the job is assigned to this builder
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if job.builder_id != Some(builder_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Append logs
    builders::append_job_logs(&state.pool, &job_id, &request.logs)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::ACCEPTED)
}
