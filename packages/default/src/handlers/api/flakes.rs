//! Flakes registry API handlers.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use tracing::{error, info, warn};

use crate::api::models::{
    ApiError, CommitDiffResponse, CreateFlakeCredentialRequest, CreateFlakeRequest,
    FlakeCommitSystemPath, FlakeCredentialSummary, FlakeRegistryItem, FlakeTimeline,
    UpdateFlakeCredentialRequest, UpdateFlakeRequest,
};
use crate::auth::extractors::{AuthenticatedUser, RequireAdmin, RequireAuth, RequireOperator};
use crate::config::CrystalForgeConfig;
use crate::flake::commits::{
    GitCommitMetadata, branch_exists, branch_exists_with_creds, get_commit_changed_files,
    get_commit_diff, get_commit_metadata, get_commit_nixos_configurations, infer_default_branch,
    infer_default_branch_with_creds, sync_commit_hashes_for_flake,
    is_history_rewrite_error,
};
use crate::flake::credentials::FlakeCredentialEnv;
use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::{require_operator_or_admin, require_viewer_or_above};
use crate::queries::admin::insert_admin_audit_event;
use crate::queries::commits::{insert_commit_with_metadata, promote_pending_commits_by_hashes};
use crate::queries::flakes::{
    cascade_delete_flake, check_flake_dependencies, count_systems_for_flake, delete_flake_by_id,
    fetch_dashboard_flake_timelines, fetch_flake_timelines, get_flake_by_id, get_flake_by_name,
    insert_flake, list_flake_registry, purge_flake_commit_history, soft_delete_flake,
    update_flake,
};
use crate::queries::flake_credentials::{
    delete_flake_credential, get_flake_credential, update_flake_credential, upsert_flake_credential,
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

    let flake_ids = match params.get("ids") {
        Some(raw) if !raw.trim().is_empty() => {
            let mut parsed = Vec::new();
            for part in raw.split(',') {
                let value = part.trim();
                if value.is_empty() {
                    continue;
                }
                match value.parse::<i32>() {
                    Ok(id) => parsed.push(id),
                    Err(_) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ApiError {
                                error: "bad_request".to_string(),
                                message: "Invalid ids query parameter".to_string(),
                                details: Some(serde_json::json!({"ids": raw})),
                            }),
                        )
                            .into_response();
                    }
                }
            }
            Some(parsed)
        }
        _ => None,
    };

    // Fetch up to 10 most recent commits per flake
    let fetch_result = if use_dashboard_view {
        fetch_dashboard_flake_timelines(&pool, 10, flake_ids.as_deref()).await
    } else {
        fetch_flake_timelines(&pool, 10, flake_ids.as_deref()).await
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

                let commit_path_lookup = if commit_hashes.is_empty() {
                    HashMap::new()
                } else {
                    fetch_commit_config_paths(&pool, timeline.flake_id, &commit_hashes)
                        .await
                        .unwrap_or_else(|err| {
                            error!(
                                "Failed to fetch commit config paths for {}: {:#}",
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

                            let marked = mark_cf_system_matches(configs, commit_path_lookup.get(&commit.hash));
                            commit.system_count = marked.len() as i64;
                            commit.systems = marked;
                        }
                    } else {
                        let marked = mark_cf_system_matches(&commit.systems, commit_path_lookup.get(&commit.hash));
                        commit.system_count = marked.len() as i64;
                        commit.systems = marked;
                    }

                    commit.system_paths = build_commit_system_paths(
                        &commit.systems,
                        commit_path_lookup.get(&commit.hash),
                    );
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

#[derive(Debug, Clone)]
struct CommitConfigPathRow {
    config_name: String,
    cf_hostname: Option<String>,
    mapped_host_count: i64,
    expected_store_path: Option<String>,
    current_store_path: Option<String>,
}

async fn fetch_commit_config_paths(
    pool: &PgPool,
    flake_id: i32,
    commit_hashes: &[String],
) -> anyhow::Result<HashMap<String, HashMap<String, CommitConfigPathRow>>> {
    if commit_hashes.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query_as::<_, (String, String, i64, Option<String>, Option<String>, Option<String>)>(
        r#"
        SELECT
            c.git_commit_hash,
            d.derivation_name,
            COALESCE(mapped_hosts.mapped_host_count, 0)::bigint,
            selected_state.hostname,
            d.store_path,
            selected_state.store_path
        FROM commits c
        JOIN derivations d
            ON d.commit_id = c.id
           AND d.derivation_type = 'nixos'
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS mapped_host_count
            FROM systems s
            WHERE s.flake_id = c.flake_id
              AND s.is_active = TRUE
              AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) = d.derivation_name
        ) mapped_hosts ON TRUE
        LEFT JOIN LATERAL (
            SELECT
                s.hostname,
                latest_ss.store_path,
                latest_ss.timestamp,
                latest_ss.id
            FROM systems s
            LEFT JOIN LATERAL (
                SELECT ss.store_path, ss.timestamp, ss.id
                FROM system_states ss
                WHERE ss.hostname = s.hostname
                ORDER BY ss.timestamp DESC, ss.id DESC
                LIMIT 1
            ) latest_ss ON TRUE
            WHERE s.flake_id = c.flake_id
              AND s.is_active = TRUE
              AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) = d.derivation_name
            ORDER BY latest_ss.timestamp DESC NULLS LAST, latest_ss.id DESC NULLS LAST, s.hostname ASC
            LIMIT 1
        ) selected_state ON TRUE
        WHERE c.flake_id = $1
          AND c.git_commit_hash = ANY($2)
        ORDER BY c.git_commit_hash, d.derivation_name, d.id DESC
        "#,
    )
    .bind(flake_id)
    .bind(commit_hashes)
    .fetch_all(pool)
    .await?;

    let mut out: HashMap<String, HashMap<String, CommitConfigPathRow>> = HashMap::new();
    for (hash, config_name, mapped_host_count, cf_hostname, expected_store_path, current_store_path) in rows {
        out.entry(hash)
            .or_default()
            .entry(config_name.clone())
            .or_insert(CommitConfigPathRow {
                config_name,
                cf_hostname,
                mapped_host_count,
                expected_store_path,
                current_store_path,
            });
    }

    Ok(out)
}

fn mark_cf_system_matches(
    configs: &[String],
    path_rows: Option<&HashMap<String, CommitConfigPathRow>>,
) -> Vec<String> {
    let Some(path_rows) = path_rows else {
        return configs
            .iter()
            .map(|name| strip_cf_suffix(name).to_string())
            .collect();
    };

    configs
        .iter()
        .map(|name| {
            let bare_name = strip_cf_suffix(name);
            if path_rows
                .get(bare_name)
                .and_then(|row| row.cf_hostname.as_ref())
                .is_some()
            {
                format!("{} [CF system]", bare_name)
            } else {
                bare_name.to_string()
            }
        })
        .collect()
}

fn build_commit_system_paths(
    configs: &[String],
    path_rows: Option<&HashMap<String, CommitConfigPathRow>>,
) -> Vec<FlakeCommitSystemPath> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for config in configs {
        let bare = strip_cf_suffix(config).to_string();
        if !seen.insert(bare.clone()) {
            continue;
        }

        let detail = path_rows.and_then(|rows| rows.get(&bare));
        out.push(FlakeCommitSystemPath {
            config_name: bare,
            is_cf_system: detail.map(|row| row.mapped_host_count > 0).unwrap_or(false),
            cf_hostname: detail.and_then(|row| row.cf_hostname.clone()),
            mapped_host_count: detail.map(|row| row.mapped_host_count).unwrap_or(0),
            expected_store_path: detail.and_then(|row| row.expected_store_path.clone()),
            current_store_path: detail.and_then(|row| row.current_store_path.clone()),
        });
    }

    if let Some(rows) = path_rows {
        let mut extras: Vec<&CommitConfigPathRow> = rows
            .values()
            .filter(|row| !seen.contains(&row.config_name))
            .collect();
        extras.sort_by(|a, b| a.config_name.cmp(&b.config_name));
        for row in extras {
            out.push(FlakeCommitSystemPath {
                config_name: row.config_name.clone(),
                is_cf_system: row.mapped_host_count > 0,
                cf_hostname: row.cf_hostname.clone(),
                mapped_host_count: row.mapped_host_count,
                expected_store_path: row.expected_store_path.clone(),
                current_store_path: row.current_store_path.clone(),
            });
        }
    }

    out
}

fn strip_cf_suffix(name: &str) -> &str {
    name.strip_suffix(" [CF system]").unwrap_or(name)
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

    let build_scope = match normalize_build_scope(payload.build_scope.as_deref()) {
        Ok(value) => value,
        Err(response) => return response,
    };

    match insert_flake(&pool, name, repo_url, &branch, build_scope).await {
        Ok(flake) => (
            StatusCode::CREATED,
            Json(FlakeRegistryItem {
                id: flake.id,
                name: flake.name,
                repo_url: flake.repo_url,
                branch: flake.branch,
                build_scope: flake.build_scope,
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

    let build_scope = match normalize_build_scope(payload.build_scope.as_deref()) {
        Ok(value) => value,
        Err(response) => return response,
    };

    match update_flake(&pool, flake_id, name, repo_url, &branch, build_scope).await {
        Ok(flake) => (
            StatusCode::OK,
            Json(FlakeRegistryItem {
                id: flake.id,
                name: flake.name,
                repo_url: flake.repo_url,
                branch: flake.branch,
                build_scope: flake.build_scope,
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

pub async fn get_flake_credentials(
    RequireAuth(_user): RequireAuth,
    State(pool): State<PgPool>,
    Path(flake_id): Path<i32>,
) -> impl IntoResponse {
    if get_flake_by_id(&pool, flake_id).await.is_err() {
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

    match get_flake_credential(&pool, flake_id).await {
        Ok(Some(credential)) => (StatusCode::OK, Json(summarize_flake_credential(&credential))).into_response(),
        Ok(None) => (StatusCode::OK, Json(FlakeCredentialSummary {
            flake_id,
            auth_type: "none".to_string(),
            username: None,
            ssh_username: None,
            has_secret: false,
        }))
            .into_response(),
        Err(err) => {
            error!("Failed to load flake credentials for {flake_id}: {err:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to load flake credentials".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

pub async fn put_flake_credentials(
    RequireOperator(_user): RequireOperator,
    State(pool): State<PgPool>,
    Path(flake_id): Path<i32>,
    Json(payload): Json<CreateFlakeCredentialRequest>,
) -> impl IntoResponse {
    if get_flake_by_id(&pool, flake_id).await.is_err() {
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

    let create = crate::models::flake_credentials::CreateFlakeCredential {
        auth_type: payload.auth_type,
        username: payload.username,
        secret: payload.secret,
        ssh_username: payload.ssh_username,
    };

    match upsert_flake_credential(&pool, flake_id, &create).await {
        Ok(credential) => (StatusCode::OK, Json(summarize_flake_credential(&credential))).into_response(),
        Err(err) => {
            let message = err.to_string();
            let status = if message.contains("require") || message.contains("invalid") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            (
                status,
                Json(ApiError {
                    error: if status == StatusCode::BAD_REQUEST {
                        "validation_error".to_string()
                    } else {
                        "internal_error".to_string()
                    },
                    message: if status == StatusCode::BAD_REQUEST {
                        message
                    } else {
                        "Failed to save flake credentials".to_string()
                    },
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

pub async fn patch_flake_credentials(
    RequireOperator(_user): RequireOperator,
    State(pool): State<PgPool>,
    Path(flake_id): Path<i32>,
    Json(payload): Json<UpdateFlakeCredentialRequest>,
) -> impl IntoResponse {
    let update = crate::models::flake_credentials::UpdateFlakeCredential {
        auth_type: payload.auth_type,
        username: payload.username,
        secret: payload.secret,
        ssh_username: payload.ssh_username,
    };

    match update_flake_credential(&pool, flake_id, &update).await {
        Ok(Some(credential)) => (StatusCode::OK, Json(summarize_flake_credential(&credential))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "not_found".to_string(),
                message: "Flake credentials not found".to_string(),
                details: None,
            }),
        )
            .into_response(),
        Err(err) => {
            let message = err.to_string();
            let status = if message.contains("require") || message.contains("invalid") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(ApiError {
                    error: if status == StatusCode::BAD_REQUEST {
                        "validation_error".to_string()
                    } else {
                        "internal_error".to_string()
                    },
                    message: if status == StatusCode::BAD_REQUEST {
                        message
                    } else {
                        "Failed to update flake credentials".to_string()
                    },
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

pub async fn delete_flake_credentials_handler(
    RequireOperator(_user): RequireOperator,
    State(pool): State<PgPool>,
    Path(flake_id): Path<i32>,
) -> impl IntoResponse {
    match delete_flake_credential(&pool, flake_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "not_found".to_string(),
                message: "Flake credentials not found".to_string(),
                details: None,
            }),
        )
            .into_response(),
        Err(err) => {
            error!("Failed to delete flake credentials for {flake_id}: {err:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to delete flake credentials".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

fn summarize_flake_credential(
    credential: &crate::models::flake_credentials::FlakeCredential,
) -> FlakeCredentialSummary {
    FlakeCredentialSummary {
        flake_id: credential.flake_id,
        auth_type: credential.auth_type.clone(),
        username: credential.username.clone(),
        ssh_username: credential.ssh_username.clone(),
        has_secret: credential
            .secret_encrypted
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
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

/// Refresh a flake's cached git repository.
///
/// This forces Nix to re-fetch the flake from the remote repository, clearing any
/// stale cached references. Useful when a flake repository has been force-pushed
/// or its git history has been rewritten.
///
/// **Authorization**: Requires Operator or Admin role.
pub async fn refresh_flake(
    RequireOperator(_user): RequireOperator,
    State(pool): State<PgPool>,
    Path(flake_id): Path<i32>,
) -> impl IntoResponse {
    use crate::flake::eval::refresh_flake_cache_with_creds;

    // Get flake details
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

    let creds = FlakeCredentialEnv::load(&pool, flake_id)
        .await
        .unwrap_or_else(|e| {
            warn!("Failed to load credentials for flake {} during refresh: {e:#}", flake_id);
            None
        });

    // Refresh the flake cache
    match refresh_flake_cache_with_creds(&flake.repo_url, &flake.branch, creds.as_ref()).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Flake cache refreshed successfully",
                "flake_id": flake_id,
                "flake_name": flake.name
            })),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to refresh flake cache for {}: {e:#}", flake.name);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "refresh_failed".to_string(),
                    message: format!("Failed to refresh flake cache: {}", e),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// Accept a detected git history rewrite for a flake by resetting stored commit history.
///
/// This action is explicit and auditable. It clears existing commit lineage for the flake,
/// then re-syncs from current remote branch head.
///
/// **Authorization**: Requires Operator or Admin role.
pub async fn accept_flake_history_rewrite(
    RequireOperator(user): RequireOperator,
    State(state): State<CFState>,
    Path(flake_id): Path<i32>,
    headers: HeaderMap,
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

    info!(
        "history_rewrite_accept_requested flake_id={} flake_name={} repo_url={} branch={} actor={}",
        flake.id,
        flake.name,
        flake.repo_url,
        flake.branch,
        user.user_id
    );

    let previous_head = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT git_commit_hash
        FROM commits
        WHERE flake_id = $1
        ORDER BY commit_timestamp DESC
        LIMIT 1
        "#,
    )
    .bind(flake.id)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();

    let deleted_commits = match purge_flake_commit_history(&pool, flake.id).await {
        Ok(count) => count,
        Err(e) => {
            error!(
                "Failed purging commit history for flake {} ({}): {e:#}",
                flake.name, flake.repo_url
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "history_reset_failed".to_string(),
                    message: "Failed to reset flake commit history".to_string(),
                    details: Some(serde_json::json!({"error": e.to_string()})),
                }),
            )
                .into_response();
        }
    };

    let inserted_hashes =
        match sync_commit_hashes_for_flake(&pool, &flake.repo_url, &flake.branch, flake.id).await {
            Ok(inserted) => inserted,
        Err(e) => {
            error!(
                "Failed re-syncing flake {} ({}) after rewrite acceptance: {e:#}",
                flake.name, flake.repo_url
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "history_resync_failed".to_string(),
                    message: "History reset completed but re-sync failed".to_string(),
                    details: Some(serde_json::json!({
                        "error": e.to_string(),
                        "deleted_commits": deleted_commits
                    })),
                }),
            )
                .into_response();
        }
    };

    let inserted = inserted_hashes.len();

    info!(
        "history_rewrite_accepted flake_id={} flake_name={} deleted_commits={} inserted_commits={} actor={}",
        flake.id,
        flake.name,
        deleted_commits,
        inserted,
        user.user_id
    );

    if inserted > 0 {
        if let Err(e) = promote_pending_commits_by_hashes(&pool, flake.id, &inserted_hashes).await {
            warn!(
                "Failed promoting rewrite-sync commits for flake {} ({}): {e:#}",
                flake.name, flake.repo_url
            );
        }
        state.queue_notifier.notify_eval_queue();
    }

    let metadata = serde_json::json!({
        "flake_id": flake.id,
        "flake_name": flake.name,
        "repo_url": flake.repo_url,
        "branch": flake.branch,
        "previous_head": previous_head,
        "deleted_commits": deleted_commits,
        "inserted_commits": inserted,
        "accepted": true,
    });

    if let Err(e) = insert_admin_audit_event(
        &pool,
        user.user_id,
        &user.user_id.to_string(),
        "accept_flake_history_rewrite",
        &format!("flake:{}", flake_id),
        headers
            .get("x-forwarded-for")
            .and_then(|h| h.to_str().ok())
            .map(String::from),
        metadata,
    )
    .await
    {
        warn!("Failed to log audit event for flake history rewrite acceptance: {e:#}");
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "message": format!(
                "Accepted history rewrite for {} on {}. Reset {} old commits and synced {} current commits.",
                flake.name,
                flake.branch,
                deleted_commits,
                inserted
            ),
            "flake_id": flake.id,
            "deleted_commits": deleted_commits,
            "inserted_commits": inserted,
            "previous_head": previous_head,
        })),
    )
        .into_response()
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
        match sync_commit_hashes_for_flake(&pool, &flake.repo_url, &flake.branch, flake.id).await {
            Ok(new_commit_hashes) => {
                let new_commits = new_commit_hashes.len();
                synced += 1;
                inserted += new_commits;
                if new_commits > 0 {
                    if let Err(e) =
                        promote_pending_commits_by_hashes(&pool, flake.id, &new_commit_hashes).await
                    {
                        warn!(
                            "Failed promoting sync-all commits for flake {} ({}): {e:#}",
                            flake.name, flake.repo_url
                        );
                    }
                }
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

    let creds = FlakeCredentialEnv::load(&pool, flake.id)
        .await
        .unwrap_or_else(|e| {
            warn!("Failed to load credentials for flake {} during sync: {e:#}", flake.id);
            None
        });

    let sync_branch = match branch_exists_with_creds(&flake.repo_url, &flake.branch, creds.as_ref()).await
    {
        Ok(true) => flake.branch.clone(),
        Ok(false) => match infer_default_branch_with_creds(&flake.repo_url, creds.as_ref()).await {
            Ok(inferred) => {
                warn!(
                    "Configured branch '{}' not found for flake {} ({}); syncing against inferred branch '{}'.",
                    flake.branch,
                    flake.name,
                    flake.repo_url,
                    inferred
                );
                inferred
            }
            Err(err) => {
                warn!(
                    "Failed to infer fallback branch for flake {} ({}): {err:#}; using configured branch '{}'.",
                    flake.name,
                    flake.repo_url,
                    flake.branch
                );
                flake.branch.clone()
            }
        },
        Err(err) => {
            warn!(
                "Failed probing configured branch '{}' for flake {} ({}): {err:#}; attempting inferred default branch.",
                flake.branch,
                flake.name,
                flake.repo_url
            );
            match infer_default_branch_with_creds(&flake.repo_url, creds.as_ref()).await {
                Ok(inferred) => inferred,
                Err(_) => flake.branch.clone(),
            }
        }
    };

    match sync_commit_hashes_for_flake(&pool, &flake.repo_url, &sync_branch, flake.id).await {
        Ok(new_commit_hashes) => {
            let new_commits = new_commit_hashes.len();
            if new_commits > 0 {
                if let Err(e) =
                    promote_pending_commits_by_hashes(&pool, flake.id, &new_commit_hashes).await
                {
                    warn!(
                        "Failed promoting sync commits for flake {} ({}): {e:#}",
                        flake.name, flake.repo_url
                    );
                }
                state.queue_notifier.notify_eval_queue();
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "ok",
                    "message": format!(
                        "Synced {} from source on {} ({} new commits).",
                        flake.name, sync_branch, new_commits
                    ),
                    "branch": sync_branch,
                })),
            )
                .into_response()
        }
        Err(e) => {
            error!(
                "Failed syncing flake {} ({}): {e:#}",
                flake.name, flake.repo_url
            );

            if is_history_rewrite_error(&e) {
                warn!(
                    "history_rewrite_detected flake_id={} flake_name={} repo_url={} branch={} details={}",
                    flake.id,
                    flake.name,
                    flake.repo_url,
                    flake.branch,
                    e
                );
                return (
                    StatusCode::CONFLICT,
                    Json(ApiError {
                        error: "history_rewrite_detected".to_string(),
                        message: format!(
                            "Git history rewrite detected for {}. Review and accept rewrite before sync.",
                            flake.name
                        ),
                        details: Some(serde_json::json!({
                            "error": e.to_string(),
                            "accept_rewrite_endpoint": format!("/api/v1/flakes/{}/accept-rewrite", flake.id),
                            "flake_id": flake.id,
                            "branch": flake.branch,
                        })),
                    }),
                )
                    .into_response();
            }

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
    if let Some(build_scope) = payload.build_scope.as_deref() {
        let build_scope = build_scope.trim();
        if !build_scope.is_empty() && !matches!(build_scope, "all_configs" | "cf_systems_only") {
            return Err("Build scope must be all_configs or cf_systems_only".to_string());
        }
    }

    Ok(())
}

fn validate_update_payload(payload: &UpdateFlakeRequest) -> Result<(), String> {
    let create_payload = CreateFlakeRequest {
        name: payload.name.clone(),
        repo_url: payload.repo_url.clone(),
        branch: payload.branch.clone(),
        build_scope: payload.build_scope.clone(),
    };
    validate_create_payload(&create_payload)
}

fn normalize_build_scope(value: Option<&str>) -> Result<&str, axum::response::Response> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some("all_configs") => Ok("all_configs"),
        Some("cf_systems_only") => Ok("cf_systems_only"),
        Some(_) => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message: "Build scope must be all_configs or cf_systems_only".to_string(),
                details: None,
            }),
        )
            .into_response()),
        None => Ok("cf_systems_only"),
    }
}

async fn resolve_requested_branch(
    repo_url: &str,
    requested_branch: Option<&str>,
) -> Result<String, String> {
    if let Some(branch) = requested_branch {
        let branch = branch.trim();
        if !branch.is_empty() {
            validate_branch(branch)?;
            return Ok(branch.to_string());
        }
    }

    match infer_default_branch(repo_url).await {
        Ok(branch) => Ok(branch),
        Err(error) => {
            warn!(
                repo_url,
                error = %error,
                "default branch inference failed; falling back to 'main' so credentials can be saved"
            );
            Ok("main".to_string())
        }
    }
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
            build_scope: None,
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
            build_scope: None,
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
            build_scope: None,
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
            build_scope: None,
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

    #[test]
    fn mark_cf_system_matches_appends_marker_when_config_maps_to_cf_system() {
        let mut rows = HashMap::new();
        rows.insert(
            "alpha".to_string(),
            CommitConfigPathRow {
                config_name: "alpha".to_string(),
                cf_hostname: Some("alpha-host".to_string()),
                mapped_host_count: 1,
                expected_store_path: Some("/nix/store/alpha".to_string()),
                current_store_path: Some("/nix/store/alpha".to_string()),
            },
        );

        let marked = mark_cf_system_matches(&["alpha".to_string(), "beta".to_string()], Some(&rows));
        assert_eq!(marked[0], "alpha [CF system]");
        assert_eq!(marked[1], "beta");
    }

    #[test]
    fn build_commit_system_paths_includes_path_details_and_unavailable_states() {
        let mut rows = HashMap::new();
        rows.insert(
            "alpha".to_string(),
            CommitConfigPathRow {
                config_name: "alpha".to_string(),
                cf_hostname: Some("alpha-host".to_string()),
                mapped_host_count: 2,
                expected_store_path: Some("/nix/store/alpha".to_string()),
                current_store_path: Some("/nix/store/current-alpha".to_string()),
            },
        );

        let details = build_commit_system_paths(
            &["alpha [CF system]".to_string(), "beta".to_string()],
            Some(&rows),
        );

        assert_eq!(details.len(), 2);
        assert_eq!(details[0].config_name, "alpha");
        assert!(details[0].is_cf_system);
        assert_eq!(details[0].mapped_host_count, 2);
        assert_eq!(details[0].cf_hostname.as_deref(), Some("alpha-host"));
        assert_eq!(details[0].expected_store_path.as_deref(), Some("/nix/store/alpha"));
        assert_eq!(
            details[0].current_store_path.as_deref(),
            Some("/nix/store/current-alpha")
        );

        assert_eq!(details[1].config_name, "beta");
        assert!(!details[1].is_cf_system);
        assert_eq!(details[1].mapped_host_count, 0);
        assert!(details[1].expected_store_path.is_none());
        assert!(details[1].current_store_path.is_none());
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
            insert_flake(
                pool,
                "test-flake",
                "https://github.com/test/repo",
                "main",
                "cf_systems_only",
            )
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
            let resurrected = insert_flake(
                &pool,
                "test-flake-restored",
                &repo_url,
                "main",
                "cf_systems_only",
            )
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

#[cfg(test)]
mod task_221_integration_tests {
    //! DB-backed integration tests for TASK-221.
    //!
    //! These tests require a live PostgreSQL connection and are run with:
    //!   cargo test -p crystal-forge --lib task_221 -- --ignored

    use crate::models::systems::System;
    use crate::queries::flake_credentials::{
        delete_flake_credential, get_flake_credential, upsert_flake_credential,
    };
    use crate::models::flake_credentials::CreateFlakeCredential;
    use crate::queries::flakes::insert_flake;
    use crate::queries::systems::insert_system;
    use crate::models::evaluate_with_policies::{
        load_allowed_systems_for_test, should_skip_system_for_test,
    };
    use sqlx::PgPool;

    // ── helpers ──────────────────────────────────────────────────────────────

    async fn make_flake(
        pool: &PgPool,
        name: &str,
        build_scope: &str,
    ) -> crate::models::flakes::Flake {
        insert_flake(
            pool,
            name,
            &format!("https://github.com/test/{name}"),
            "main",
            build_scope,
        )
        .await
        .expect("insert_flake failed")
    }

    async fn make_system(
        pool: &PgPool,
        hostname: &str,
        flake_id: Option<i32>,
        config_name: Option<&str>,
    ) -> System {
        // Use a deterministic ed25519 key for test systems (same approach as security_regression.rs)
        use ed25519_dalek::SigningKey;
        let key = SigningKey::from_bytes(&[42u8; 32]);
        let verifying = key.verifying_key();
        let system = System {
            id: uuid::Uuid::new_v4(),
            hostname: hostname.to_string(),
            environment_id: None,
            is_active: true,
            public_key: crate::models::public_key::PublicKey::from_verifying_key(verifying),
            flake_id,
            derivation: String::new(),
            system_configuration_name: config_name.map(str::to_string),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            desired_target: None,
            deployment_policy: "manual".to_string(),
        };
        insert_system(pool, &system).await.expect("insert_system failed")
    }

    // ── flake credentials ────────────────────────────────────────────────────

    async fn get_test_pool() -> PgPool {
        let db_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for TASK-221 integration tests");
        sqlx::PgPool::connect(&db_url)
            .await
            .expect("Failed to connect to test DB")
    }

    /// Set a deterministic test encryption key so credential tests don't require
    /// a real secret in the environment.  Must be called at the start of any test
    /// that exercises credential encryption/decryption.
    fn set_test_encryption_key() {
        // 64 hex chars = 32-byte AES-256 key — valid for the cache_secrets implementation.
        // SAFETY: single-threaded test context; no concurrent env reads
        unsafe {
            std::env::set_var(
                "CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY",
                "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_pat_credential_round_trips_encrypted() {
        set_test_encryption_key();
        let pool = get_test_pool().await;
        let flake = make_flake(&pool, "test-cred-pat", "cf_systems_only").await;

        let create = CreateFlakeCredential {
            auth_type: "pat".to_string(),
            username: Some("oauth2".to_string()),
            secret: Some("glpat-supersecret1234".to_string()),
            ssh_username: None,
        };

        let stored = upsert_flake_credential(&pool, flake.id, &create)
            .await
            .expect("upsert_flake_credential failed");

        // secret is returned decrypted via the query helper
        assert_eq!(stored.auth_type, "pat");
        assert_eq!(stored.username.as_deref(), Some("oauth2"));
        assert_eq!(stored.secret_encrypted.as_deref(), Some("glpat-supersecret1234"));

        // verify it is actually stored encrypted in the DB (not plaintext)
        let raw = sqlx::query_scalar::<_, Option<String>>(
            "SELECT secret_encrypted FROM flake_credentials WHERE flake_id = $1",
        )
        .bind(flake.id)
        .fetch_one(&pool)
        .await
        .expect("DB fetch failed");

        let raw_secret = raw.expect("secret should be stored");
        assert_ne!(
            raw_secret, "glpat-supersecret1234",
            "Secret must be encrypted at rest, not stored as plaintext"
        );

        // re-read via query helper and confirm decryption
        let fetched = get_flake_credential(&pool, flake.id)
            .await
            .expect("get_flake_credential failed")
            .expect("credential should exist");
        assert_eq!(fetched.secret_encrypted.as_deref(), Some("glpat-supersecret1234"));
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_ssh_credential_round_trips_encrypted() {
        set_test_encryption_key();
        let pool = get_test_pool().await;
        let flake = make_flake(&pool, "test-cred-ssh", "cf_systems_only").await;

        let fake_key = "-----BEGIN OPENSSH PRIVATE KEY-----\nfake-key-data\n-----END OPENSSH PRIVATE KEY-----";
        let create = CreateFlakeCredential {
            auth_type: "ssh_key".to_string(),
            username: None,
            secret: Some(fake_key.to_string()),
            ssh_username: Some("git".to_string()),
        };

        let stored = upsert_flake_credential(&pool, flake.id, &create)
            .await
            .expect("upsert failed");

        assert_eq!(stored.auth_type, "ssh_key");
        assert_eq!(stored.ssh_username.as_deref(), Some("git"));
        assert_eq!(stored.secret_encrypted.as_deref(), Some(fake_key));

        // verify encrypted at rest
        let raw: Option<String> = sqlx::query_scalar(
            "SELECT secret_encrypted FROM flake_credentials WHERE flake_id = $1",
        )
        .bind(flake.id)
        .fetch_one(&pool)
        .await
        .expect("DB fetch failed");
        assert_ne!(raw.unwrap(), fake_key, "SSH key must be encrypted at rest");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_delete_credential_removes_record() {
        set_test_encryption_key();
        let pool = get_test_pool().await;
        let flake = make_flake(&pool, "test-cred-delete", "cf_systems_only").await;

        let create = CreateFlakeCredential {
            auth_type: "pat".to_string(),
            username: None,
            secret: Some("tok".to_string()),
            ssh_username: None,
        };
        upsert_flake_credential(&pool, flake.id, &create).await.unwrap();

        let deleted = delete_flake_credential(&pool, flake.id).await.unwrap();
        assert!(deleted, "delete should return true for existing record");

        let fetched = get_flake_credential(&pool, flake.id).await.unwrap();
        assert!(fetched.is_none(), "credential should be gone after delete");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_pat_validation_rejects_missing_secret() {
        set_test_encryption_key();
        let pool = get_test_pool().await;
        let flake = make_flake(&pool, "test-cred-pat-invalid", "cf_systems_only").await;

        let create = CreateFlakeCredential {
            auth_type: "pat".to_string(),
            username: None,
            secret: None, // missing — should fail validation
            ssh_username: None,
        };
        let result = upsert_flake_credential(&pool, flake.id, &create).await;
        assert!(result.is_err(), "PAT credential without secret must fail validation");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("require"), "Error should mention requirement: {msg}");
    }

    // ── build_scope filtering ────────────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_build_scope_cf_systems_only_filters_to_mapped_configs() {
        let pool = get_test_pool().await;
        let flake = make_flake(&pool, "test-scope-cf", "cf_systems_only").await;
        // Two systems mapped to this flake
        make_system(&pool, "sys-alpha", Some(flake.id), Some("nixos-config-alpha")).await;
        make_system(&pool, "sys-beta", Some(flake.id), None).await; // config_name = hostname

        let allowed = load_allowed_systems_for_test(&pool, &flake, "all")
            .await
            .expect("load_allowed_systems failed");

        let allowed = allowed.expect("should produce a restriction list for cf_systems_only");
        assert!(allowed.contains(&"nixos-config-alpha".to_string()), "custom config name expected");
        assert!(allowed.contains(&"sys-beta".to_string()), "hostname fallback expected");
        assert!(!allowed.contains(&"not-in-cf".to_string()), "unlisted system must be absent");

        // should_skip for an unknown config name
        assert!(
            should_skip_system_for_test(&Some(allowed.clone()), "not-in-cf"),
            "unknown config should be skipped"
        );
        // should NOT skip for a known config name
        assert!(
            !should_skip_system_for_test(&Some(allowed), "nixos-config-alpha"),
            "known config must not be skipped"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_build_scope_all_configs_produces_no_filter() {
        let pool = get_test_pool().await;
        let flake = make_flake(&pool, "test-scope-all", "all_configs").await;
        make_system(&pool, "sys-gamma", Some(flake.id), None).await;

        let allowed = load_allowed_systems_for_test(&pool, &flake, "all")
            .await
            .expect("load_allowed_systems failed");

        assert!(allowed.is_none(), "all_configs scope must not restrict evaluation");
    }

    // ── system_configuration_name and update endpoint ────────────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_system_configuration_name_falls_back_to_hostname() {
        let pool = get_test_pool().await;
        let flake = make_flake(&pool, "test-sysconfig-fallback", "cf_systems_only").await;
        let system = make_system(&pool, "host-fallback", Some(flake.id), None).await;
        assert_eq!(
            system.configuration_name(), "host-fallback",
            "configuration_name() must fall back to hostname when column is NULL"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_system_configuration_name_used_when_set() {
        let pool = get_test_pool().await;
        let flake = make_flake(&pool, "test-sysconfig-explicit", "cf_systems_only").await;
        let system = make_system(&pool, "host-explicit", Some(flake.id), Some("custom-nixos-config")).await;
        assert_eq!(
            system.configuration_name(), "custom-nixos-config",
            "configuration_name() must return the explicit config name"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_update_system_metadata_persists_config_name() {
        let pool = get_test_pool().await;
        use crate::queries::systems::{get_system_detail_by_id, update_system_metadata};
        let flake = make_flake(&pool, "test-sysmeta", "cf_systems_only").await;
        let system = make_system(&pool, "host-meta", Some(flake.id), None).await;

        update_system_metadata(
            &pool,
            system.id,
            "host-meta",
            None,
            Some(flake.id),
            Some("new-config-name"),
            "manual",
        )
        .await
        .expect("update_system_metadata failed");

        let detail = get_system_detail_by_id(&pool, system.id)
            .await
            .expect("query failed")
            .expect("system should exist");

        assert_eq!(
            detail.system_configuration_name.as_deref(),
            Some("new-config-name"),
            "config name should be persisted after update"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_update_system_metadata_with_unknown_flake_is_rejected_at_handler_level() {
        let pool = get_test_pool().await;
        // NOTE: update_system_metadata itself accepts arbitrary flake_id (the FK is enforced by
        // the DB). The 400 validation for "unknown flake name" happens in update_system_handler
        // before metadata is written. This test verifies the query layer correctly persists a
        // known flake_id and that passing an invalid FK gets a DB error (not a silent NULL).
        use crate::queries::systems::update_system_metadata;
        let flake = make_flake(&pool, "test-sysmeta-fk", "cf_systems_only").await;
        let system = make_system(&pool, "host-fk", Some(flake.id), None).await;

        let result = update_system_metadata(
            &pool,
            system.id,
            "host-fk",
            None,
            Some(999999), // non-existent flake_id → FK violation
            None,
            "manual",
        )
        .await;

        assert!(
            result.is_err(),
            "Passing a non-existent flake_id to update_system_metadata must produce a DB error"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_migration_adds_system_configuration_name_column() {
        let pool = get_test_pool().await;
        // Verify the column exists with the correct type
        let col_type: Option<String> = sqlx::query_scalar(
            "SELECT data_type FROM information_schema.columns
             WHERE table_name = 'systems' AND column_name = 'system_configuration_name'"
        )
        .fetch_optional(&pool)
        .await
        .expect("information_schema query failed");

        assert_eq!(
            col_type.as_deref(),
            Some("text"),
            "system_configuration_name column must exist with type 'text'"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_migration_adds_build_scope_column_with_constraint() {
        let pool = get_test_pool().await;
        // Verify build_scope column and default
        let default_val: Option<String> = sqlx::query_scalar(
            "SELECT column_default FROM information_schema.columns
             WHERE table_name = 'flakes' AND column_name = 'build_scope'"
        )
        .fetch_optional(&pool)
        .await
        .expect("information_schema query failed");

        assert!(
            default_val.as_deref().map(|d| d.contains("cf_systems_only")).unwrap_or(false),
            "build_scope must default to cf_systems_only, got: {:?}", default_val
        );

        // Verify CHECK constraint rejects bad values
        let bad_insert = sqlx::query(
            "INSERT INTO flakes (name, repo_url, branch, build_scope) VALUES ($1,$2,$3,$4)"
        )
        .bind("constraint-test")
        .bind("https://github.com/constraint/test")
        .bind("main")
        .bind("invalid_scope_value")
        .execute(&pool)
        .await;

        assert!(bad_insert.is_err(), "CHECK constraint must reject invalid build_scope value");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_migration_creates_flake_credentials_table() {
        let pool = get_test_pool().await;
        let col_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'flake_credentials'"
        )
        .fetch_one(&pool)
        .await
        .expect("information_schema query failed");

        assert!(col_count >= 7, "flake_credentials table must have at least 7 columns, found {col_count}");

        // Verify auth_type CHECK constraint rejects bad values
        // (We need a flake to satisfy FK — use a subquery)
        let flake = make_flake(&pool, "test-cred-constraint", "cf_systems_only").await;
        let bad_insert = sqlx::query(
            "INSERT INTO flake_credentials (flake_id, auth_type) VALUES ($1, $2)"
        )
        .bind(flake.id)
        .bind("invalid_auth_type")
        .execute(&pool)
        .await;

        assert!(bad_insert.is_err(), "CHECK constraint must reject invalid auth_type value");
    }
}
