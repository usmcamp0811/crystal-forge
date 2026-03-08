use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::api::models::ApiError;
use crate::handlers::api::rbac::require_admin as require_admin_user;
use crate::models::cache_destination::{CacheDestination, CreateCacheDestination, UpdateCacheDestination};
use crate::queries::{cache_destinations, cache_push};

// ============================================================================
// Cache Destinations API
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListCacheDestinationsQuery {
    #[serde(default)]
    pub enabled_only: bool,
}

/// GET /api/caches - List all cache destinations
pub async fn list_cache_destinations(
    State(pool): State<PgPool>,
    Query(query): Query<ListCacheDestinationsQuery>,
) -> impl IntoResponse {
    match cache_destinations::list_cache_destinations(&pool, query.enabled_only).await {
        Ok(destinations) => (StatusCode::OK, Json(destinations)).into_response(),
        Err(e) => {
            tracing::error!("Failed to list cache destinations: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to list cache destinations".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// GET /api/caches/:id - Get a single cache destination
pub async fn get_cache_destination(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match cache_destinations::get_cache_destination(&pool, id).await {
        Ok(Some(destination)) => (StatusCode::OK, Json(destination)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("Cache destination with id {} not found", id),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to get cache destination {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to get cache destination".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/caches - Create a new cache destination (admin only)
pub async fn create_cache_destination(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(create): Json<CreateCacheDestination>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    // Validate the request
    if let Err(e) = create.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "error".to_string(),
                message: e,
                details: None,
            }),
        )
            .into_response();
    }

    match cache_destinations::create_cache_destination(&pool, &create).await {
        Ok(destination) => (StatusCode::CREATED, Json(destination)).into_response(),
        Err(e) => {
            tracing::error!("Failed to create cache destination: {:#}", e);
            let error_msg = if e.to_string().contains("duplicate key") || e.to_string().contains("unique constraint") {
                format!("Cache destination with name '{}' already exists", create.name)
            } else {
                "Failed to create cache destination".to_string()
            };
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                error: "error".to_string(),
                message: error_msg,
                details: None,
            }),
            )
                .into_response()
        }
    }
}

/// PUT /api/caches/:id - Update a cache destination (admin only)
pub async fn update_cache_destination(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(update): Json<UpdateCacheDestination>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match cache_destinations::update_cache_destination(&pool, id, &update).await {
        Ok(Some(destination)) => (StatusCode::OK, Json(destination)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("Cache destination with id {} not found", id),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to update cache destination {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// DELETE /api/caches/:id - Delete a cache destination (admin only)
pub async fn delete_cache_destination(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match cache_destinations::delete_cache_destination(&pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("Cache destination with id {} not found", id),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to delete cache destination {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to delete cache destination".to_string(),
                }),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Cache Push Jobs API
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListCachePushJobsQuery {
    pub status: Option<String>,
    pub cache_destination: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

fn default_limit() -> i32 {
    50
}

/// GET /api/cache-push-jobs - List cache push jobs with filtering
pub async fn list_cache_push_jobs(
    State(pool): State<PgPool>,
    Query(query): Query<ListCachePushJobsQuery>,
) -> impl IntoResponse {
    match cache_push::list_cache_push_jobs(
        &pool,
        query.status.as_deref(),
        query.cache_destination.as_deref(),
        Some(query.limit),
        Some(query.offset),
    )
    .await
    {
        Ok(jobs) => (StatusCode::OK, Json(jobs)).into_response(),
        Err(e) => {
            tracing::error!("Failed to list cache push jobs: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to list cache push jobs".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// GET /api/cache-push-jobs/:id - Get cache push job details
pub async fn get_cache_push_job(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match cache_push::get_cache_push_job_detail(&pool, id).await {
        Ok(Some(job)) => (StatusCode::OK, Json(job)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("Cache push job with id {} not found", id),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to get cache push job {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to get cache push job".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/cache-push-jobs/:id/retry - Retry a failed cache push job (admin only)
pub async fn retry_cache_push_job(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match cache_push::retry_cache_push_job(&pool, id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Cache push job queued for retry"
            })),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("Cache push job {} not found or not in a retryable state", id),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to retry cache push job {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to retry cache push job".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/cache-push-jobs/:id/cancel - Cancel a pending cache push job (admin only)
pub async fn cancel_cache_push_job(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match cache_push::cancel_cache_push_job(&pool, id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Cache push job cancelled"
            })),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("Cache push job {} not found or not in a cancellable state", id),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to cancel cache push job {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to cancel cache push job".to_string(),
                }),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkJobAction {
    pub job_ids: Vec<i32>,
}

/// POST /api/cache-push-jobs/bulk-retry - Bulk retry cache push jobs (admin only)
pub async fn bulk_retry_cache_push_jobs(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(action): Json<BulkJobAction>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    if action.job_ids.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "No job IDs provided".to_string(),
            }),
        )
            .into_response();
    }

    match cache_push::bulk_retry_cache_push_jobs(&pool, &action.job_ids).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": format!("Queued {} jobs for retry", count),
                "count": count
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to bulk retry cache push jobs: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to bulk retry cache push jobs".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/cache-push-jobs/bulk-cancel - Bulk cancel cache push jobs (admin only)
pub async fn bulk_cancel_cache_push_jobs(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(action): Json<BulkJobAction>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    if action.job_ids.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "No job IDs provided".to_string(),
            }),
        )
            .into_response();
    }

    match cache_push::bulk_cancel_cache_push_jobs(&pool, &action.job_ids).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": format!("Cancelled {} jobs", count),
                "count": count
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to bulk cancel cache push jobs: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Failed to bulk cancel cache push jobs".to_string(),
                }),
            )
                .into_response()
        }
    }
}
