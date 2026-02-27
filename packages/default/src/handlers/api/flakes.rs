//! Flakes registry API handlers.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::error;

use crate::api::models::{
    ApiError, CommitDiffResponse, CreateFlakeRequest, FlakeRegistryItem, FlakeTimeline,
};
use crate::auth::extractors::{RequireAdmin, RequireOperator};
use crate::flake::commits::{get_commit_diff, get_commit_metadata, GitCommitMetadata};
use crate::handlers::api::rbac::{require_operator_or_admin, require_viewer_or_above};
use crate::queries::flakes::{
    count_systems_for_flake, delete_flake_by_id, fetch_flake_timelines, get_flake_by_id,
    get_flake_by_name, insert_flake, list_flake_registry,
};
use crate::queries::users::get_by_email;

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
        Ok(mut timelines) => {
            for timeline in &mut timelines {
                let hashes: Vec<String> = timeline
                    .commits
                    .iter()
                    .map(|commit| commit.hash.clone())
                    .collect();
                let metadata = match get_commit_metadata(&timeline.repo_url, &hashes).await {
                    Ok(data) => data,
                    Err(err) => {
                        error!(
                            "Failed to hydrate commit metadata for {}: {:#}",
                            timeline.repo_url, err
                        );
                        HashMap::new()
                    }
                };

                let mut user_lookup_cache: HashMap<String, Option<String>> = HashMap::new();
                for commit in &mut timeline.commits {
                    if let Some(detail) = metadata.get(&commit.hash) {
                        if commit.message.trim().is_empty() {
                            commit.message = detail.message.trim().to_string();
                        }

                        commit.author =
                            resolve_timeline_author(&pool, detail, &mut user_lookup_cache).await;
                    }

                    if commit.message.trim().is_empty() {
                        let short = commit.hash.chars().take(7).collect::<String>();
                        commit.message = format!("Commit {short}");
                    }
                    if commit.author.trim().is_empty() {
                        commit.author = "Unknown author".to_string();
                    }
                }
            }

            (StatusCode::OK, Json(timelines)).into_response()
        }
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

async fn resolve_timeline_author(
    pool: &PgPool,
    detail: &GitCommitMetadata,
    user_lookup_cache: &mut HashMap<String, Option<String>>,
) -> String {
    if let Some(email) = detail
        .author_email
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let key = email.to_ascii_lowercase();
        let username = if let Some(value) = user_lookup_cache.get(&key) {
            value.clone()
        } else {
            let resolved = match get_by_email(pool, email).await {
                Ok(Some(user)) => Some(user.username),
                Ok(None) => None,
                Err(err) => {
                    error!(
                        "Failed to resolve user for commit email {}: {:#}",
                        email, err
                    );
                    None
                }
            };
            user_lookup_cache.insert(key.clone(), resolved.clone());
            resolved
        };

        if let Some(username) = username {
            let author_name = detail.author_name.trim();
            if author_name.is_empty() || author_name.eq_ignore_ascii_case(&username) {
                return format!("@{username}");
            }
            return format!("@{username} ({author_name})");
        }
    }

    let author_name = detail.author_name.trim();
    if !author_name.is_empty() {
        return author_name.to_string();
    }

    if let Some(email) = detail
        .author_email
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return email.to_string();
    }

    "Unknown author".to_string()
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

    // Fetch the diff from git
    match get_commit_diff(&flake.repo_url, "main", &commit_hash).await {
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
    headers: HeaderMap,
    Json(payload): Json<CreateFlakeRequest>,
) -> impl IntoResponse {
    if require_operator_or_admin(&pool, &headers).await.is_none() {
        return forbidden();
    }

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

/// Delete a flake from the registry.
///
/// **Authorization**: Requires Admin role (destructive operation).
pub async fn delete_flake(
    RequireAdmin(_user): RequireAdmin,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(flake_id): Path<i32>,
) -> impl IntoResponse {
    if require_operator_or_admin(&pool, &headers).await.is_none() {
        return forbidden();
    }

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

fn forbidden() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Admin or operator privileges are required".to_string(),
            details: None,
        }),
    )
        .into_response()
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
    use crate::models::auth_identity::AuthRole;
    use axum::extract::State;
    use axum::response::IntoResponse;
    use sqlx::postgres::PgPoolOptions;

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

    #[test]
    fn require_operator_or_admin_checks_role_membership() {
        assert!(crate::handlers::api::rbac::has_operator_or_admin_role(&[
            AuthRole::Operator,
        ]));
        assert!(crate::handlers::api::rbac::has_operator_or_admin_role(&[
            AuthRole::Admin
        ]));
        assert!(crate::handlers::api::rbac::has_operator_or_admin_role(&[
            AuthRole::Viewer,
            AuthRole::Operator,
        ]));
        assert!(!crate::handlers::api::rbac::has_operator_or_admin_role(&[
            AuthRole::Viewer,
        ]));
    }

    // NOTE: Authorization tests for create_flake, delete_flake moved to extractor-level tests
    // in auth/extractors.rs. These handlers now use RequireOperator and RequireAdmin extractors
    // which enforce authorization before the handler is called, so unit tests at this level
    // cannot test authorization behavior. Integration tests should test the full request path.
}
