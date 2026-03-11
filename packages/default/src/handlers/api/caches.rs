use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::api::models::ApiError;
use crate::handlers::api::rbac::{authenticated_user_roles, require_admin as require_admin_user};
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
                    error: "internal_error".to_string(),
                    message: "Failed to list cache destinations".to_string(),
                    details: None,
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
                error: "not_found".to_string(),
                message: format!("Cache destination with id {} not found", id),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to get cache destination {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to get cache destination".to_string(),
                    details: None,
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
            })
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
            })
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
                error: "not_found".to_string(),
                message: format!("Cache destination with id {} not found", id),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to update cache destination {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                message: e.to_string(),
                details: None,
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
                error: "not_found".to_string(),
                message: format!("Cache destination with id {} not found", id),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to delete cache destination {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to delete cache destination".to_string(),
                    details: None,
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
                    error: "internal_error".to_string(),
                    message: "Failed to list cache push jobs".to_string(),
                    details: None,
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
                error: "not_found".to_string(),
                message: format!("Cache push job with id {} not found", id),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to get cache push job {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to get cache push job".to_string(),
                    details: None,
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
                error: "error".to_string(),
                message: format!("Cache push job {} not found or not in a retryable state", id),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to retry cache push job {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to retry cache push job".to_string(),
                    details: None,
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
                error: "error".to_string(),
                message: format!("Cache push job {} not found or not in a cancellable state", id),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to cancel cache push job {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to cancel cache push job".to_string(),
                    details: None,
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
                error: "validation_error".to_string(),
                message: "No job IDs provided".to_string(),
                details: None,
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
                    error: "internal_error".to_string(),
                    message: "Failed to bulk retry cache push jobs".to_string(),
                    details: None,
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
                error: "validation_error".to_string(),
                message: "No job IDs provided".to_string(),
                details: None,
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
                    error: "internal_error".to_string(),
                    message: "Failed to bulk cancel cache push jobs".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}


// ─────────────────────────────────────────────────────────────────────────────
// Environment Assignment Handlers
// ─────────────────────────────────────────────────────────────────────────────

/// Request to assign environments to a cache destination
#[derive(Debug, Deserialize)]
pub struct AssignEnvironmentsRequest {
    pub environment_ids: Vec<i32>,
}

/// GET /api/caches/:id/environments - Get environments assigned to a cache
pub async fn get_cache_environments_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(cache_id): Path<i32>,
) -> impl IntoResponse {
    // Require authentication
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                error: "unauthorized".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    match crate::queries::cache_destinations::get_cache_environments(&pool, cache_id).await {
        Ok(environment_ids) => (StatusCode::OK, Json(environment_ids)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "internal_server_error".to_string(),
                message: format!("Failed to get cache environments: {e}"),
                details: None,
            }),
        )
            .into_response(),
    }
}

/// PUT /api/caches/:id/environments - Assign environments to a cache destination
pub async fn assign_cache_environments_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(cache_id): Path<i32>,
    Json(req): Json<AssignEnvironmentsRequest>,
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

    match crate::queries::cache_destinations::assign_environments_to_cache(
        &pool,
        cache_id,
        &req.environment_ids,
    )
    .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Environments assigned successfully",
                "cache_id": cache_id,
                "environment_count": req.environment_ids.len()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "internal_server_error".to_string(),
                message: format!("Failed to assign environments: {e}"),
                details: None,
            }),
        )
            .into_response(),
    }
}

/// GET /api/environments/:id/caches - Get caches assigned to an environment
pub async fn get_environment_caches_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(environment_id): Path<i32>,
) -> impl IntoResponse {
    // Require authentication
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                error: "unauthorized".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    match crate::queries::cache_destinations::get_caches_for_environment(&pool, environment_id).await {
        Ok(caches) => (StatusCode::OK, Json(caches)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "internal_server_error".to_string(),
                message: format!("Failed to get environment caches: {e}"),
                details: None,
            }),
        )
            .into_response(),
    }
}

