//! Flakes registry API handlers.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use tracing::error;

use crate::api::models::{
    ApiError, CommitDiffResponse, CreateFlakeRequest, FlakeRegistryItem, FlakeTimeline,
    UpdateFlakeRequest,
};
use crate::auth::extractors::{RequireAdmin, RequireOperator};
use crate::flake::commits::{
    branch_exists, get_commit_changed_files, get_commit_diff, get_commit_metadata,
    get_commit_nixos_configurations, infer_default_branch, sync_commits_for_repo,
    GitCommitMetadata,
};
use crate::handlers::api::rbac::{require_operator_or_admin, require_viewer_or_above};
use crate::queries::flakes::{
    count_systems_for_flake, delete_flake_by_id, fetch_flake_timelines, get_flake_by_id,
    get_flake_by_name, insert_flake, list_flake_registry, update_flake,
};
use crate::queries::users::get_by_email;

const MAX_HYDRATION_COMMITS_PER_REQUEST: usize = 20;

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
            let mut remaining_hydration_budget = MAX_HYDRATION_COMMITS_PER_REQUEST;

            for timeline in &mut timelines {
                let hashes: Vec<String> = timeline
                    .commits
                    .iter()
                    .filter(|commit| {
                        commit.message.trim().is_empty() || commit.author.trim().is_empty()
                    })
                    .take(remaining_hydration_budget)
                    .map(|commit| commit.hash.clone())
                    .collect();
                let metadata = if hashes.is_empty() {
                    HashMap::new()
                } else {
                    remaining_hydration_budget =
                        remaining_hydration_budget.saturating_sub(hashes.len());
                    match get_commit_metadata(&timeline.repo_url, &hashes).await {
                        Ok(data) => data,
                        Err(err) => {
                            error!(
                                "Failed to hydrate commit metadata for flake {}: {:#}",
                                timeline.flake_name, err
                            );
                            HashMap::new()
                        }
                    }
                };

                let mut user_lookup_cache: HashMap<String, Option<String>> = HashMap::new();
                let commit_hashes: Vec<String> =
                    timeline.commits.iter().map(|commit| commit.hash.clone()).collect();

                let missing_config_hashes: Vec<String> = timeline
                    .commits
                    .iter()
                    .filter(|commit| commit.systems.is_empty())
                    .map(|commit| commit.hash.clone())
                    .collect();
                let hydrated_configs = if missing_config_hashes.is_empty() {
                    HashMap::new()
                } else {
                    get_commit_nixos_configurations(&timeline.repo_url, &missing_config_hashes).await
                };
                let hydrated_changed_files = if missing_config_hashes.is_empty() {
                    HashMap::new()
                } else {
                    get_commit_changed_files(&timeline.repo_url, &missing_config_hashes)
                        .await
                        .unwrap_or_else(|err| {
                            error!(
                                "Failed to hydrate changed files for {}: {:#}",
                                timeline.flake_name, err
                            );
                            HashMap::new()
                        })
                };

                let cf_config_matches = if commit_hashes.is_empty() {
                    HashMap::new()
                } else {
                    fetch_cf_system_matches(&pool, &commit_hashes)
                        .await
                        .unwrap_or_else(|err| {
                            error!(
                                "Failed to fetch CF-system commit matches for {}: {:#}",
                                timeline.flake_name, err
                            );
                            HashMap::new()
                        })
                };

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

                    if commit.systems.is_empty() {
                        if let Some(configs) = hydrated_configs.get(&commit.hash) {
                            let changed_files = hydrated_changed_files
                                .get(&commit.hash)
                                .cloned()
                                .unwrap_or_default();

                            if let Err(err) = upsert_commit_artifacts_cache(
                                &pool,
                                timeline.flake_id,
                                &commit.hash,
                                configs,
                                &changed_files,
                            )
                            .await
                            {
                                error!(
                                    "Failed to persist commit artifacts for {}@{}: {:#}",
                                    timeline.flake_name, commit.hash, err
                                );
                            }

                            let marked = mark_cf_system_matches(configs, cf_config_matches.get(&commit.hash));
                            commit.system_count = marked.len() as i64;
                            commit.systems = marked;
                        }
                    } else {
                        let marked = mark_cf_system_matches(
                            &commit.systems,
                            cf_config_matches.get(&commit.hash),
                        );
                        commit.system_count = marked.len() as i64;
                        commit.systems = marked;
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

async fn upsert_commit_artifacts_cache(
    pool: &PgPool,
    flake_id: i32,
    commit_hash: &str,
    nixos_configurations: &[String],
    changed_files: &[String],
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO commit_artifacts_cache (commit_id, nixos_configurations, changed_files, populated_at)
        SELECT c.id, $3, $4, NOW()
        FROM commits c
        WHERE c.flake_id = $1
          AND c.git_commit_hash = $2
        ON CONFLICT (commit_id) DO UPDATE
        SET nixos_configurations = EXCLUDED.nixos_configurations,
            changed_files = EXCLUDED.changed_files,
            populated_at = NOW()
        "#,
    )
    .bind(flake_id)
    .bind(commit_hash)
    .bind(nixos_configurations)
    .bind(changed_files)
    .execute(pool)
    .await?;

    Ok(())
}

async fn fetch_cf_system_matches(
    pool: &PgPool,
    commit_hashes: &[String],
) -> anyhow::Result<HashMap<String, HashSet<String>>> {
    if commit_hashes.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query_as::<_, (String, Option<Vec<String>>)>(
        r#"
        SELECT
            s.current_commit_hash,
            ARRAY_AGG(DISTINCT s.hostname ORDER BY s.hostname) AS hostnames
        FROM view_system_deployment_status s
        WHERE s.current_commit_hash = ANY($1)
        GROUP BY s.current_commit_hash
        "#,
    )
    .bind(commit_hashes)
    .fetch_all(pool)
    .await?;

    let mut out = HashMap::new();
    for (hash, hostnames) in rows {
        out.insert(
            hash,
            hostnames
                .unwrap_or_default()
                .into_iter()
                .collect::<HashSet<_>>(),
        );
    }

    Ok(out)
}

fn mark_cf_system_matches(configs: &[String], cf_matches: Option<&HashSet<String>>) -> Vec<String> {
    let Some(cf_matches) = cf_matches else {
        return configs.to_vec();
    };

    configs
        .iter()
        .map(|name| {
            let bare_name = name.strip_suffix(" [CF system]").unwrap_or(name);
            if cf_matches.contains(bare_name) {
                format!("{} [CF system]", bare_name)
            } else {
                bare_name.to_string()
            }
        })
        .collect()
}

async fn resolve_timeline_author(
    pool: &PgPool,
    detail: &GitCommitMetadata,
    user_lookup_cache: &mut HashMap<String, Option<String>>,
) -> String {
    if let Some(email) = detail
        .author_email
        .as_deref()
        .and_then(normalize_author_email)
    {
        let key = email.to_ascii_lowercase();
        let username = if let Some(value) = user_lookup_cache.get(&key) {
            value.clone()
        } else {
            let resolved = match get_by_email(pool, &email).await {
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
        .and_then(normalize_author_email)
    {
        return email.to_string();
    }

    "Unknown author".to_string()
}

fn normalize_author_email(email: &str) -> Option<String> {
    let trimmed = email.trim();
    let without_brackets = trimmed
        .strip_prefix('<')
        .unwrap_or(trimmed)
        .strip_suffix('>')
        .unwrap_or(trimmed)
        .trim();

    if without_brackets.is_empty() {
        None
    } else {
        Some(without_brackets.to_string())
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

    // Fetch the diff from git
    match get_commit_diff(&flake.repo_url, &flake.branch, &commit_hash).await {
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

    let branch = match resolve_requested_branch(repo_url, payload.branch.as_deref()).await {
        Ok(branch) => branch,
        Err(message) => {
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
    };

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

    match insert_flake(&pool, name, repo_url, &branch).await {
        Ok(flake) => (
            StatusCode::CREATED,
            Json(FlakeRegistryItem {
                id: flake.id,
                name: flake.name,
                repo_url: flake.repo_url,
                branch: flake.branch,
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

    let branch = match resolve_requested_branch(repo_url, payload.branch.as_deref()).await {
        Ok(branch) => branch,
        Err(message) => {
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
    };

    match update_flake(&pool, flake_id, name, repo_url, &branch).await {
        Ok(flake) => (
            StatusCode::OK,
            Json(FlakeRegistryItem {
                id: flake.id,
                name: flake.name,
                repo_url: flake.repo_url,
                branch: flake.branch,
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
        match sync_commits_for_repo(&pool, &flake.repo_url, &flake.branch).await {
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
                    "branch": flake.branch,
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

    match sync_commits_for_repo(&pool, &flake.repo_url, &flake.branch).await {
        Ok(new_commits) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "message": format!(
                    "Synced {} from source on {} ({} new commits).",
                    flake.name, flake.branch, new_commits
                ),
                "branch": flake.branch,
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
        branch: payload.branch.clone(),
    };
    validate_create_payload(&create_payload)
}

async fn resolve_requested_branch(
    repo_url: &str,
    requested_branch: Option<&str>,
) -> Result<String, String> {
    if let Some(branch) = requested_branch {
        let branch = branch.trim();
        if !branch.is_empty() {
            validate_branch(branch)?;
            let exists = branch_exists(repo_url, branch)
                .await
                .map_err(|e| format!("Repository URL is not reachable as a git remote: {e}"))?;
            if !exists {
                return Err(format!("Branch '{branch}' was not found on the repository"));
            }
            return Ok(branch.to_string());
        }
    }

    infer_default_branch(repo_url)
        .await
        .map_err(|e| format!("Failed to infer default branch for repository: {e}"))
}

fn validate_branch(branch: &str) -> Result<(), String> {
    if branch.is_empty() {
        return Err("Branch is required when provided".to_string());
    }
    if branch.contains(char::is_whitespace) {
        return Err("Branch must not contain whitespace".to_string());
    }
    if branch.starts_with('-') {
        return Err("Branch must not start with '-'".to_string());
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

    #[test]
    fn create_payload_requires_name() {
        let payload = CreateFlakeRequest {
            name: "   ".to_string(),
            repo_url: "https://github.com/org/repo".to_string(),
            branch: None,
        };
        let err = validate_create_payload(&payload).unwrap_err();
        assert!(err.contains("name"));
    }

    #[test]
    fn create_payload_requires_repo_url() {
        let payload = CreateFlakeRequest {
            name: "prod-core".to_string(),
            repo_url: "   ".to_string(),
            branch: None,
        };
        let err = validate_create_payload(&payload).unwrap_err();
        assert!(err.contains("URL"));
    }

    #[test]
    fn create_payload_rejects_invalid_repo_url() {
        let payload = CreateFlakeRequest {
            name: "prod-core".to_string(),
            repo_url: "repo-no-scheme".to_string(),
            branch: None,
        };
        let err = validate_create_payload(&payload).unwrap_err();
        assert!(err.contains("git remote"));
    }

    #[test]
    fn create_payload_accepts_valid_repo_url() {
        let payload = CreateFlakeRequest {
            name: "prod-core".to_string(),
            repo_url: "git@github.com:org/repo.git".to_string(),
            branch: None,
        };
        assert!(validate_create_payload(&payload).is_ok());
    }

    #[test]
    fn normalize_author_email_trims_and_strips_brackets() {
        assert_eq!(
            normalize_author_email(" <dev@example.com> "),
            Some("dev@example.com".to_string())
        );
        assert_eq!(normalize_author_email(""), None);
        assert_eq!(normalize_author_email("   "), None);
    }

    // NOTE: Authorization tests for create_flake, delete_flake moved to extractor-level tests
    // in auth/extractors.rs. These handlers now use RequireOperator and RequireAdmin extractors
    // which enforce authorization before the handler is called, so unit tests at this level
    // cannot test authorization behavior. Integration tests should test the full request path.
}
