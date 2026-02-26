//! Flakes registry API handlers.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sqlx::PgPool;
use tracing::error;

use crate::api::models::{
    ApiError, CommitDiffResponse, CreateFlakeRequest, FlakeRegistryItem, FlakeTimeline,
    UpdateFlakeRequest,
};
use crate::auth::extractors::{RequireAdmin, RequireOperator};
use crate::flake::commits::{get_commit_diff, infer_default_branch, sync_commits_for_repo};
use crate::handlers::api::rbac::require_viewer_or_above;
use crate::queries::flakes::{
    count_systems_for_flake, delete_flake_by_id, fetch_flake_timelines, get_flake_by_id,
    get_flake_by_name, insert_flake, list_flake_registry, update_flake,
};

pub async fn list_flakes(State(pool): State<PgPool>, headers: HeaderMap) -> impl IntoResponse {
    if require_viewer_or_above(&pool, &headers).await.is_none() {
        return forbidden_viewer();
    }

    match list_flake_registry(&pool).await {
        Ok(flakes) => (StatusCode::OK, Json(flakes)).into_response(),
        Err(e) => {
            error!("Failed to list flakes: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to list flakes".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// Get flake timelines with recent commits for dashboard.
///
/// **Authorization**: Requires Viewer role or above.
pub async fn get_flake_timelines(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_viewer_or_above(&pool, &headers).await.is_none() {
        return forbidden_viewer();
    }

    // Fetch up to 10 most recent commits per flake
    match fetch_flake_timelines(&pool, 10).await {
        Ok(timelines) => (StatusCode::OK, Json(timelines)).into_response(),
        Err(e) => {
            error!("Failed to fetch flake timelines: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to fetch flake timelines".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// Get the git diff for a specific commit in a flake.
///
/// **Authorization**: Requires Viewer role or above.
pub async fn get_commit_diff_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((flake_id, commit_hash)): Path<(i32, String)>,
) -> impl IntoResponse {
    if require_viewer_or_above(&pool, &headers).await.is_none() {
        return forbidden_viewer();
    }

    // Get flake details to get repo_url and branch
    let flake = match get_flake_by_id(&pool, flake_id).await {
        Ok(flake) => flake,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: "Flake not found".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let branch = resolve_sync_branch(&flake.repo_url).await;

    // Fetch the diff from git
    match get_commit_diff(&flake.repo_url, &branch, &commit_hash).await {
        Ok(diff) => (
            StatusCode::OK,
            Json(CommitDiffResponse {
                commit_hash: commit_hash.clone(),
                diff,
            }),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to fetch commit diff for {}: {e:#}", commit_hash);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch diff for commit {}", commit_hash),
                    details: Some(serde_json::json!({"error": e.to_string()})),
                }),
            )
                .into_response()
        }
    }
}

/// Create a new flake in the registry.
///
/// **Authorization**: Requires Operator or Admin role (write operation).
pub async fn create_flake(
    RequireOperator(_user): RequireOperator,
    State(pool): State<PgPool>,
    Json(payload): Json<CreateFlakeRequest>,
) -> impl IntoResponse {
    if let Err(message) = validate_create_payload(&payload) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message,
                details: None,
            }),
        )
            .into_response();
    }

    let name = payload.name.trim();
    let repo_url = payload.repo_url.trim();

    if let Err(message) = validate_repo_url_reachable(repo_url).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message,
                details: None,
            }),
        )
            .into_response();
    }

    match get_flake_by_name(&pool, name).await {
        Ok(existing) if !existing.repo_url.eq_ignore_ascii_case(repo_url) => {
            return (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "conflict".to_string(),
                    message: "Flake name already exists in the registry".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
        Ok(_) => {}
        Err(e) => {
            if !matches!(
                e.downcast_ref::<sqlx::Error>(),
                Some(sqlx::Error::RowNotFound)
            ) {
                error!("Failed to query existing flake by name: {e:#}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: "internal_error".to_string(),
                        message: "Failed to create flake".to_string(),
                        details: None,
                    }),
                )
                    .into_response();
            }
        }
    }

    match insert_flake(&pool, name, repo_url).await {
        Ok(flake) => (
            StatusCode::CREATED,
            Json(FlakeRegistryItem {
                id: flake.id,
                name: flake.name,
                repo_url: flake.repo_url,
                system_count: 0,
            }),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to create flake: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to create flake".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// Update an existing flake in the registry.
///
/// **Authorization**: Requires Operator or Admin role (write operation).
pub async fn update_flake_handler(
    RequireOperator(_user): RequireOperator,
    State(pool): State<PgPool>,
    Path(flake_id): Path<i32>,
    Json(payload): Json<UpdateFlakeRequest>,
) -> impl IntoResponse {
    if let Err(message) = validate_update_payload(&payload) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message,
                details: None,
            }),
        )
            .into_response();
    }

    let name = payload.name.trim();
    let repo_url = payload.repo_url.trim();

    if let Err(message) = validate_repo_url_reachable(repo_url).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message,
                details: None,
            }),
        )
            .into_response();
    }

    match update_flake(&pool, flake_id, name, repo_url).await {
        Ok(flake) => (
            StatusCode::OK,
            Json(FlakeRegistryItem {
                id: flake.id,
                name: flake.name,
                repo_url: flake.repo_url,
                system_count: count_systems_for_flake(&pool, flake_id).await.unwrap_or(0),
            }),
        )
            .into_response(),
        Err(e) => {
            if matches!(
                e.downcast_ref::<sqlx::Error>(),
                Some(sqlx::Error::RowNotFound)
            ) {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: "not_found".to_string(),
                        message: "Flake not found".to_string(),
                        details: None,
                    }),
                )
                    .into_response();
            }

            if matches!(
                e.downcast_ref::<sqlx::Error>(),
                Some(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505")
            ) {
                return (
                    StatusCode::CONFLICT,
                    Json(ApiError {
                        error: "conflict".to_string(),
                        message: "Repository URL already exists in the registry".to_string(),
                        details: None,
                    }),
                )
                    .into_response();
            }

            error!("Failed to update flake: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to update flake".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// Delete a flake from the registry.
///
/// **Authorization**: Requires Admin role (destructive operation).
pub async fn delete_flake(
    RequireAdmin(_user): RequireAdmin,
    State(pool): State<PgPool>,
    Path(flake_id): Path<i32>,
) -> impl IntoResponse {
    match count_systems_for_flake(&pool, flake_id).await {
        Ok(system_count) if system_count > 0 => {
            return (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "flake_in_use".to_string(),
                    message: format!(
                        "Flake is linked to {system_count} systems and cannot be removed"
                    ),
                    details: None,
                }),
            )
                .into_response();
        }
        Ok(_) => {}
        Err(e) => {
            error!("Failed to check flake usage: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to validate flake removal".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    }

    match delete_flake_by_id(&pool, flake_id).await {
        Ok(0) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "not_found".to_string(),
                message: "Flake not found".to_string(),
                details: None,
            }),
        )
            .into_response(),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            error!("Failed to delete flake: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to delete flake".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// Trigger a commit sync for all tracked flakes.
///
/// **Authorization**: Requires Operator or Admin role.
pub async fn sync_all_flakes_handler(
    RequireOperator(_user): RequireOperator,
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    let flakes = match list_flake_registry(&pool).await {
        Ok(flakes) => flakes,
        Err(e) => {
            error!("Failed to list flakes for sync: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to start flake sync".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let attempted = flakes.len();
    let mut synced = 0usize;
    let mut inserted = 0usize;
    let mut failed = Vec::new();
    for flake in flakes {
        let branch = resolve_sync_branch(&flake.repo_url).await;
        match sync_commits_for_repo(&pool, &flake.repo_url, &branch).await {
            Ok(new_commits) => {
                synced += 1;
                inserted += new_commits;
            }
            Err(e) => {
                error!(
                    "Failed syncing flake {} ({}): {e:#}",
                    flake.name, flake.repo_url
                );
                failed.push(serde_json::json!({
                    "id": flake.id,
                    "name": flake.name,
                    "repo_url": flake.repo_url,
                    "error": e.to_string(),
                }));
            }
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": if failed.is_empty() { "ok" } else { "partial" },
            "message": format!(
                "Synced {synced}/{attempted} flakes from source ({inserted} new commits)."
            ),
            "attempted": attempted,
            "succeeded": synced,
            "failed_count": failed.len(),
            "failed": failed,
        })),
    )
        .into_response()
}

/// Trigger a commit sync for a specific flake.
///
/// **Authorization**: Requires Operator or Admin role.
pub async fn sync_flake_handler(
    RequireOperator(_user): RequireOperator,
    State(pool): State<PgPool>,
    Path(flake_id): Path<i32>,
) -> impl IntoResponse {
    let flake = match get_flake_by_id(&pool, flake_id).await {
        Ok(flake) => flake,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: "Flake not found".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let branch = resolve_sync_branch(&flake.repo_url).await;

    match sync_commits_for_repo(&pool, &flake.repo_url, &branch).await {
        Ok(new_commits) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "message": format!(
                    "Synced {} from source on {} ({} new commits).",
                    flake.name, branch, new_commits
                ),
                "branch": branch,
            })),
        )
            .into_response(),
        Err(e) => {
            error!(
                "Failed syncing flake {} ({}): {e:#}",
                flake.name, flake.repo_url
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: format!("Failed to sync {} from source", flake.name),
                    details: Some(serde_json::json!({"error": e.to_string()})),
                }),
            )
                .into_response()
        }
    }
}
fn forbidden_viewer() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Viewer, operator, or admin privileges are required".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn validate_create_payload(payload: &CreateFlakeRequest) -> Result<(), String> {
    let name = payload.name.trim();
    let repo_url = payload.repo_url.trim();

    if name.is_empty() {
        return Err("Flake name is required".to_string());
    }
    if repo_url.is_empty() {
        return Err("Repository URL is required".to_string());
    }
    if !looks_like_repo_url(repo_url) {
        return Err("Repository URL must look like a git remote".to_string());
    }

    Ok(())
}

fn validate_update_payload(payload: &UpdateFlakeRequest) -> Result<(), String> {
    let create_payload = CreateFlakeRequest {
        name: payload.name.clone(),
        repo_url: payload.repo_url.clone(),
    };
    validate_create_payload(&create_payload)
}

async fn validate_repo_url_reachable(repo_url: &str) -> Result<(), String> {
    infer_default_branch(repo_url)
        .await
        .map(|_| ())
        .map_err(|e| format!("Repository URL is not reachable as a git remote: {e}"))
}

async fn resolve_sync_branch(repo_url: &str) -> String {
    match infer_default_branch(repo_url).await {
        Ok(branch) => branch,
        Err(e) => {
            error!(
                "Failed to resolve default branch for {} (falling back to main): {e:#}",
                repo_url
            );
            "main".to_string()
        }
    }
}

fn looks_like_repo_url(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("git@")
        || lower.starts_with("ssh://")
        || lower.starts_with("github:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_payload_requires_name() {
        let payload = CreateFlakeRequest {
            name: "   ".to_string(),
            repo_url: "https://github.com/org/repo".to_string(),
        };
        let err = validate_create_payload(&payload).unwrap_err();
        assert!(err.contains("name"));
    }

    #[test]
    fn create_payload_requires_repo_url() {
        let payload = CreateFlakeRequest {
            name: "prod-core".to_string(),
            repo_url: "   ".to_string(),
        };
        let err = validate_create_payload(&payload).unwrap_err();
        assert!(err.contains("URL"));
    }

    #[test]
    fn create_payload_rejects_invalid_repo_url() {
        let payload = CreateFlakeRequest {
            name: "prod-core".to_string(),
            repo_url: "repo-no-scheme".to_string(),
        };
        let err = validate_create_payload(&payload).unwrap_err();
        assert!(err.contains("git remote"));
    }

    #[test]
    fn create_payload_accepts_valid_repo_url() {
        let payload = CreateFlakeRequest {
            name: "prod-core".to_string(),
            repo_url: "git@github.com:org/repo.git".to_string(),
        };
        assert!(validate_create_payload(&payload).is_ok());
    }
}
