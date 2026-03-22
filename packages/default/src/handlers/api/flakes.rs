//! Flakes registry API handlers.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use tracing::{error, warn};

use crate::api::models::{
    ApiError, CommitDiffResponse, CreateFlakeRequest, FlakeRegistryItem, FlakeTimeline,
    UpdateFlakeRequest,
};
use crate::auth::extractors::{AuthenticatedUser, RequireAdmin, RequireOperator};
use crate::config::CrystalForgeConfig;
use crate::flake::commits::{
    GitCommitMetadata, branch_exists, get_commit_changed_files, get_commit_diff,
    get_commit_metadata, get_commit_nixos_configurations, infer_default_branch,
    sync_commits_for_repo,
};
use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::{require_operator_or_admin, require_viewer_or_above};
use crate::queries::admin::insert_admin_audit_event;
use crate::queries::commits::insert_commit_with_metadata;
use crate::queries::flakes::{
    cascade_delete_flake, check_flake_dependencies, count_systems_for_flake, delete_flake_by_id,
    fetch_dashboard_flake_timelines, fetch_flake_timelines, get_flake_by_id, get_flake_by_name,
    insert_flake, list_flake_registry, soft_delete_flake, update_flake,
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
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    if require_viewer_or_above(&pool, &headers).await.is_none() {
        return forbidden_viewer();
    }

    // Dashboard view shows CF system deployment counts
    // Flakes view shows nixosConfigurations from cache
    let use_dashboard_view = params
        .get("view")
        .map(|v| v == "dashboard")
        .unwrap_or(false);

    // Fetch up to 10 most recent commits per flake
    let fetch_result = if use_dashboard_view {
        fetch_dashboard_flake_timelines(&pool, 10).await
    } else {
        fetch_flake_timelines(&pool, 10).await
    };

    match fetch_result {
        Ok(mut timelines) => {
            // Dashboard view doesn't need git metadata (message/author), skip hydration
            if use_dashboard_view {
                return (StatusCode::OK, Json(timelines)).into_response();
            }

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
                    // This fallback should rarely run now that metadata is cached in commits table
                    warn!(
                        "Falling back to git metadata hydration for {} commits in flake {} (likely old commits from before metadata caching)",
                        hashes.len(),
                        timeline.flake_name
                    );
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
                let commit_hashes: Vec<String> = timeline
                    .commits
                    .iter()
                    .map(|commit| commit.hash.clone())
                    .collect();

                // Skip inline hydration for now - too slow for API requests
                // TODO: Background job to populate commit_artifacts_cache
                let hydrated_configs: HashMap<String, Vec<String>> = HashMap::new();
                let hydrated_changed_files: HashMap<String, Vec<String>> = HashMap::new();

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

                            let marked = mark_cf_system_matches(
                                configs,
                                cf_config_matches.get(&commit.hash),
                            );
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
/// Delete a flake from the registry.
///
/// **Authorization**:
/// - Admin: Can delete any flake
/// - Operator: Can delete flakes only in environments they have access to
/// - Viewer: Denied (403)
///
/// **Query Parameters**:
/// - `hard` (bool): If true, permanently delete from database. Default: soft delete
/// - `cascade` (bool): If true, also delete all related evaluations, builds, deployments. Default: false
///
/// **Responses**:
/// - 200 OK: Flake deleted successfully
/// - 403 Forbidden: User doesn't have permission to delete this flake
/// - 404 Not Found: Flake doesn't exist
/// - 409 Conflict: Flake has active dependencies (use cascade=true to force delete)
pub async fn delete_flake(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
    Path(flake_id): Path<i32>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let hard_delete = params
        .get("hard")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);
    let cascade = params
        .get("cascade")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    // Get flake to check existence and environment
    let flake = match get_flake_by_id(&pool, flake_id).await {
        Ok(f) => f,
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
            error!("Failed to get flake: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to get flake".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    // RBAC: Admin can delete any flake, Operator can delete flakes ONLY if they have
    // access to ALL environments where this flake is used
    if !user.is_admin() {
        // For Operator: verify they have access to ALL environments using this flake.
        // This prevents scope leak where an operator with access to one environment
        // could delete a flake also used by other environments they don't control.
        let rbac_check = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT NOT EXISTS(
                -- Find systems using this flake in environments the operator does NOT have access to
                SELECT 1 FROM systems s
                WHERE s.flake_id = $1
                  AND s.environment_id IS NOT NULL
                  AND NOT EXISTS (
                      SELECT 1 FROM user_environment_memberships uem
                      WHERE uem.environment_id = s.environment_id
                        AND uem.user_id = $2
                  )
                
                UNION
                
                -- Also check historical derivations targeting systems in inaccessible environments
                SELECT 1 FROM commits c
                JOIN derivations d ON d.commit_id = c.id
                JOIN systems s ON s.id::text = d.target_id
                WHERE c.flake_id = $1
                  AND s.environment_id IS NOT NULL
                  AND NOT EXISTS (
                      SELECT 1 FROM user_environment_memberships uem
                      WHERE uem.environment_id = s.environment_id
                        AND uem.user_id = $2
                  )
            )
            "#,
        )
        .bind(flake_id)
        .bind(user.user_id)
        .fetch_one(&pool)
        .await;

        match rbac_check {
            Ok(true) => {} // Operator has access to ALL environments using this flake
            Ok(false) => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(ApiError {
                        error: "forbidden".to_string(),
                        message: "You do not have permission to delete this flake. This flake is used in environments you do not have access to.".to_string(),
                        details: None,
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                error!("Failed to check flake access: {e:#}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: "internal_error".to_string(),
                        message: "Failed to check permissions".to_string(),
                        details: None,
                    }),
                )
                    .into_response();
            }
        }
    }

    // Check for active dependencies unless cascade delete requested
    if !cascade {
        match check_flake_dependencies(&pool, flake_id).await {
            Ok(count) if count > 0 => {
                return (
                    StatusCode::CONFLICT,
                    Json(ApiError {
                        error: "conflict".to_string(),
                        message: format!(
                            "Flake has {} active dependencies (evaluations, builds, or deployments). Use cascade=true to force delete.",
                            count
                        ),
                        details: Some(serde_json::json!({
                            "dependencies_count": count,
                            "hint": "Add ?cascade=true to the request to delete all related data"
                        })),
                    }),
                )
                    .into_response();
            }
            Ok(_) => {} // No dependencies, safe to delete
            Err(e) => {
                error!("Failed to check dependencies: {e:#}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: "internal_error".to_string(),
                        message: "Failed to check dependencies".to_string(),
                        details: None,
                    }),
                )
                    .into_response();
            }
        }
    }

    // Perform deletion (soft, hard, or cascade)
    let delete_result = if cascade {
        // Cascade delete (hard delete + all dependencies)
        // Use transaction for safety
        let mut tx = match pool.begin().await {
            Ok(tx) => tx,
            Err(e) => {
                error!("Failed to begin transaction: {e:#}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: "internal_error".to_string(),
                        message: "Failed to delete flake".to_string(),
                        details: None,
                    }),
                )
                    .into_response();
            }
        };

        // Execute cascade delete within the transaction
        let result = cascade_delete_flake(&mut tx, flake_id).await;

        if result.is_ok() {
            if let Err(e) = tx.commit().await {
                error!("Failed to commit cascade delete: {e:#}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: "internal_error".to_string(),
                        message: "Failed to delete flake".to_string(),
                        details: None,
                    }),
                )
                    .into_response();
            }
        } else {
            let _ = tx.rollback().await;
        }

        result
    } else if hard_delete {
        delete_flake_by_id(&pool, flake_id).await
    } else {
        soft_delete_flake(&pool, flake_id).await
    };

    match delete_result {
        Ok(rows) if rows > 0 => {
            // Audit log
            let deletion_type = if cascade {
                "cascade"
            } else if hard_delete {
                "hard"
            } else {
                "soft"
            };

            let metadata = serde_json::json!({
                "flake_id": flake_id,
                "flake_name": flake.name,
                "deletion_type": deletion_type,
            });

            if let Err(e) = insert_admin_audit_event(
                &pool,
                user.user_id,
                &user.user_id.to_string(),
                "delete_flake",
                &format!("flake:{}", flake_id),
                headers
                    .get("x-forwarded-for")
                    .and_then(|h| h.to_str().ok())
                    .map(String::from),
                metadata,
            )
            .await
            {
                warn!("Failed to log audit event for flake deletion: {e:#}");
            }

            (
                StatusCode::OK,
                Json(serde_json::json!({"message": "Flake deleted successfully"})),
            )
                .into_response()
        }
        Ok(_) => {
            // No rows affected - flake might already be soft-deleted
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: "Flake not found or already deleted".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
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
    State(state): State<CFState>,
) -> impl IntoResponse {
    let pool = state.pool.clone();

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

    if inserted > 0 {
        state.queue_notifier.notify_eval_queue();
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
    State(state): State<CFState>,
    Path(flake_id): Path<i32>,
) -> impl IntoResponse {
    let pool = state.pool.clone();

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

    if should_inject_mock_sync_commit() {
        return match inject_mock_sync_commit(
            &pool,
            flake.id,
            &flake.name,
            &flake.repo_url,
            &flake.branch,
        )
        .await
        {
            Ok(mock_commit_hash) => {
                state.queue_notifier.notify_eval_queue();
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "status": "ok",
                        "message": format!(
                            "Mock-synced {} on {} (1 synthetic commit).",
                            flake.name, flake.branch
                        ),
                        "branch": flake.branch,
                        "mock_commit": mock_commit_hash,
                    })),
                )
                    .into_response()
            }
            Err(e) => {
                error!(
                    "Failed injecting mock sync commit for {} ({}): {e:#}",
                    flake.name, flake.repo_url
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: "internal_error".to_string(),
                        message: format!("Failed to mock-sync {}", flake.name),
                        details: Some(serde_json::json!({"error": e.to_string()})),
                    }),
                )
                    .into_response()
            }
        };
    }

    match sync_commits_for_repo(&pool, &flake.repo_url, &flake.branch).await {
        Ok(new_commits) => {
            if new_commits > 0 {
                state.queue_notifier.notify_eval_queue();
            }
            (
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
                .into_response()
        }
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

async fn inject_mock_sync_commit(
    pool: &PgPool,
    flake_id: i32,
    flake_name: &str,
    repo_url: &str,
    branch: &str,
) -> anyhow::Result<String> {
    let now = chrono::Utc::now();
    let synthetic_hash = synthetic_mock_sync_hash(flake_id, repo_url, now);
    let synthetic_message = format!(
        "MOCK SYNC: synthetic commit for {} on {} at {}",
        flake_name,
        branch,
        now.to_rfc3339()
    );

    insert_commit_with_metadata(
        pool,
        &synthetic_hash,
        repo_url,
        now,
        Some(&synthetic_message),
        Some("mock-sync"),
    )
    .await?;

    Ok(synthetic_hash)
}

fn should_inject_mock_sync_commit() -> bool {
    CrystalForgeConfig::load()
        .map(|cfg| cfg.server.execution_mode.is_mock())
        .unwrap_or(false)
}

fn synthetic_mock_sync_hash(
    flake_id: i32,
    repo_url: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let seed = format!(
        "{flake_id}:{repo_url}:{}",
        now.timestamp_nanos_opt().unwrap_or_default()
    );

    let mut h1 = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut h1);
    let a = h1.finish();

    let mut h2 = std::collections::hash_map::DefaultHasher::new();
    (seed.clone() + ":mock").hash(&mut h2);
    let b = h2.finish();

    format!("{:016x}{:016x}{:08x}", a, b, flake_id as u32)
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

    #[test]
    fn synthetic_mock_sync_hash_is_git_like_and_stable() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-03-06T00:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&chrono::Utc);
        let one = synthetic_mock_sync_hash(7, "https://github.com/org/repo", now);
        let two = synthetic_mock_sync_hash(7, "https://github.com/org/repo", now);

        assert_eq!(one, two);
        assert_eq!(one.len(), 40);
        assert!(one.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // NOTE: Authorization tests for create_flake, delete_flake moved to extractor-level tests
    // in auth/extractors.rs. These handlers now use RequireOperator and RequireAdmin extractors
    // which enforce authorization before the handler is called, so unit tests at this level
    // cannot test authorization behavior. Integration tests should test the full request path.

    #[cfg(test)]
    mod delete_tests {
        use super::*;
        use crate::models::flakes::Flake;
        use crate::queries::flakes::{
            cascade_delete_flake, check_flake_dependencies, delete_flake_by_id, get_flake_by_id,
            insert_flake, soft_delete_flake,
        };
        use sqlx::PgPool;

        async fn setup_test_flake(pool: &PgPool) -> Flake {
            insert_flake(pool, "test-flake", "https://github.com/test/repo", "main")
                .await
                .expect("Failed to create test flake")
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn test_soft_delete_sets_timestamp(pool: PgPool) {
            let flake = setup_test_flake(&pool).await;

            // Soft delete
            let affected = soft_delete_flake(&pool, flake.id)
                .await
                .expect("Soft delete should succeed");
            assert_eq!(affected, 1);

            // Verify deleted_at is set
            let result = sqlx::query_as::<_, Flake>("SELECT * FROM flakes WHERE id = $1")
                .bind(flake.id)
                .fetch_one(&pool)
                .await
                .expect("Should find flake in raw query");

            assert!(result.deleted_at.is_some(), "deleted_at should be set");
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn test_soft_deleted_flake_excluded_from_get_by_id(pool: PgPool) {
            let flake = setup_test_flake(&pool).await;

            // Soft delete
            soft_delete_flake(&pool, flake.id).await.unwrap();

            // get_flake_by_id should now fail
            let result = get_flake_by_id(&pool, flake.id).await;
            assert!(
                result.is_err(),
                "get_flake_by_id should not find soft-deleted flake"
            );
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn test_soft_delete_idempotent(pool: PgPool) {
            let flake = setup_test_flake(&pool).await;

            // First soft delete
            let affected = soft_delete_flake(&pool, flake.id).await.unwrap();
            assert_eq!(affected, 1);

            // Second soft delete should return 0 (already deleted)
            let affected = soft_delete_flake(&pool, flake.id).await.unwrap();
            assert_eq!(affected, 0);
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn test_hard_delete_removes_permanently(pool: PgPool) {
            let flake = setup_test_flake(&pool).await;

            // Hard delete
            let affected = delete_flake_by_id(&pool, flake.id)
                .await
                .expect("Hard delete should succeed");
            assert_eq!(affected, 1);

            // Verify flake is gone (even in raw query)
            let result = sqlx::query_as::<_, Flake>("SELECT * FROM flakes WHERE id = $1")
                .bind(flake.id)
                .fetch_optional(&pool)
                .await
                .expect("Query should succeed");

            assert!(result.is_none(), "Flake should be permanently deleted");
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn test_resurrection_clears_deleted_at(pool: PgPool) {
            let flake = setup_test_flake(&pool).await;
            let repo_url = flake.repo_url.clone();

            // Soft delete
            soft_delete_flake(&pool, flake.id).await.unwrap();

            // Re-insert same repo_url
            let resurrected = insert_flake(&pool, "test-flake-restored", &repo_url, "main")
                .await
                .expect("Re-insert should succeed");

            // Verify deleted_at is cleared
            assert!(
                resurrected.deleted_at.is_none(),
                "deleted_at should be cleared on resurrection"
            );

            // Verify it's now visible via get_by_id
            let fetched = get_flake_by_id(&pool, resurrected.id).await;
            assert!(fetched.is_ok(), "Resurrected flake should be visible");
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn test_check_dependencies_counts_systems(pool: PgPool) {
            let flake = setup_test_flake(&pool).await;

            // Create a test environment
            let env_id = sqlx::query_scalar::<_, uuid::Uuid>(
                "INSERT INTO environments (name, description, color_hex) VALUES ($1, $2, $3) RETURNING id"
            )
            .bind("test-env")
            .bind("Test environment")
            .bind("#FF0000")
            .fetch_one(&pool)
            .await
            .expect("Failed to create test environment");

            // Create a system using this flake
            sqlx::query(
                "INSERT INTO systems (id, name, hostname, environment_id, flake_id, enabled) 
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(uuid::Uuid::new_v4())
            .bind("test-system")
            .bind("test-host")
            .bind(env_id)
            .bind(flake.id)
            .bind(true)
            .execute(&pool)
            .await
            .expect("Failed to create test system");

            // Check dependencies
            let count = check_flake_dependencies(&pool, flake.id)
                .await
                .expect("check_dependencies should succeed");

            assert_eq!(count, 1, "Should count the system as a dependency");
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn test_cascade_delete_within_transaction(pool: PgPool) {
            let flake = setup_test_flake(&pool).await;

            // Create test data
            let env_id = sqlx::query_scalar::<_, uuid::Uuid>(
                "INSERT INTO environments (name, description, color_hex) VALUES ($1, $2, $3) RETURNING id"
            )
            .bind("test-env")
            .bind("Test environment")
            .bind("#FF0000")
            .fetch_one(&pool)
            .await
            .unwrap();

            let system_id = uuid::Uuid::new_v4();
            sqlx::query(
                "INSERT INTO systems (id, name, hostname, environment_id, flake_id, enabled) 
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(system_id)
            .bind("test-system")
            .bind("test-host")
            .bind(env_id)
            .bind(flake.id)
            .bind(true)
            .execute(&pool)
            .await
            .unwrap();

            // Begin transaction and cascade delete
            let mut tx = pool.begin().await.expect("Failed to begin transaction");
            let affected = cascade_delete_flake(&mut tx, flake.id)
                .await
                .expect("Cascade delete should succeed");
            tx.commit().await.expect("Failed to commit transaction");

            assert_eq!(affected, 1);

            // Verify flake is gone
            let flake_result = sqlx::query_as::<_, Flake>("SELECT * FROM flakes WHERE id = $1")
                .bind(flake.id)
                .fetch_optional(&pool)
                .await
                .unwrap();
            assert!(flake_result.is_none());

            // Verify system is gone
            let system_result =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM systems WHERE id = $1")
                    .bind(system_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(system_result, 0);
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn test_cascade_delete_rollback_on_error(pool: PgPool) {
            let flake = setup_test_flake(&pool).await;

            // Create test system
            let env_id = sqlx::query_scalar::<_, uuid::Uuid>(
                "INSERT INTO environments (name, description, color_hex) VALUES ($1, $2, $3) RETURNING id"
            )
            .bind("test-env")
            .bind("Test environment")
            .bind("#FF0000")
            .fetch_one(&pool)
            .await
            .unwrap();

            sqlx::query(
                "INSERT INTO systems (id, name, hostname, environment_id, flake_id, enabled) 
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(uuid::Uuid::new_v4())
            .bind("test-system")
            .bind("test-host")
            .bind(env_id)
            .bind(flake.id)
            .bind(true)
            .execute(&pool)
            .await
            .unwrap();

            // Begin transaction
            let mut tx = pool.begin().await.expect("Failed to begin transaction");

            // Do cascade delete but rollback
            let _result = cascade_delete_flake(&mut tx, flake.id).await;
            tx.rollback().await.expect("Failed to rollback");

            // Verify flake still exists
            let flake_result = get_flake_by_id(&pool, flake.id).await;
            assert!(
                flake_result.is_ok(),
                "Flake should still exist after rollback"
            );
        }
    }
}
