//! Environments API handlers.
//!
//! Exposes the environment registry to authenticated clients, scoped by
//! user membership:
//!
//! - Admins see all environments.
//! - Operators and Viewers see only environments they are members of.
//!
//! # Endpoints
//!
//! - `GET /api/v1/environments` — returns `Vec<EnvironmentSummary>`
//! - `GET /api/v1/environments/:id` — returns a single `EnvironmentSummary`

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sqlx::Error as SqlxError;
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{
    ApiError, CreateEnvironmentRequest, DeploymentPolicySummary, EnvironmentSummary, 
    UpdateEnvironmentPoliciesRequest, UpdateEnvironmentRequest,
};
use crate::auth::models::Role;
use crate::handlers::api::rbac::{authenticated_user_roles, has_admin_role};
use crate::models::auth_identity::AuthRole;
use crate::queries::environments::{
    count_systems_in_environment, create_environment as create_environment_row, delete_environment,
    find_environment_for_user, get_environment_required_policy_ids, get_environment_with_policies,
    list_deployment_policies, list_environments_for_user, set_environment_required_policies,
    update_environment_metadata,
};

/// `GET /api/v1/environments`
///
/// Returns all environments visible to the authenticated user.
///
/// Admins see every environment; operators and viewers see only the
/// environments they have been assigned to via `user_environment_memberships`.
pub async fn list_environments(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    // Admins bypass membership filter — pass None so the query returns all.
    let scoped_user_id = if has_admin_role(&roles) {
        None
    } else {
        Some(user_id)
    };

    match list_environments_for_user(&pool, scoped_user_id).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(_) => internal_error("Failed to load environments"),
    }
}

/// `GET /api/v1/environments/:id`
///
/// Returns a single environment by ID, scoped to the authenticated user.
///
/// Returns 404 if the environment does not exist or the user is not a member.
pub async fn get_environment(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(environment_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let scoped_user_id = if has_admin_role(&roles) {
        None
    } else {
        Some(user_id)
    };

    match find_environment_for_user(&pool, environment_id, scoped_user_id).await {
        Ok(Some(env)) => (StatusCode::OK, Json(env)).into_response(),
        Ok(None) => not_found(),
        Err(_) => internal_error("Failed to load environment"),
    }
}

/// `POST /api/v1/environments`
///
/// Creates a new environment. Admin role required.
pub async fn create_environment(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<CreateEnvironmentRequest>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_manage_environments() {
        return forbidden_manage();
    }

    let name = payload.name.trim();
    if name.is_empty() {
        return bad_request("Environment name is required");
    }

    if name.len() > 50 {
        return bad_request("Environment name must be 50 characters or fewer");
    }

    let description = payload.description.as_deref().map(str::trim).filter(|v| !v.is_empty());
    let color_hex = payload.color_hex.trim();
    if !looks_like_hex_color(color_hex) {
        return bad_request("Environment color must be a valid #RRGGBB value");
    }

    match create_environment_row(&pool, name, description, color_hex, payload.is_active).await {
        Ok(env) => (StatusCode::CREATED, Json(env)).into_response(),
        Err(err) => {
            if is_unique_violation(&err) {
                conflict("Environment name already exists")
            } else {
                internal_error("Failed to create environment")
            }
        }
    }
}

/// `DELETE /api/v1/environments/:id`
///
/// Deletes an environment when it has no assigned systems. Admin role required.
pub async fn delete_environment_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(environment_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_manage_environments() {
        return forbidden_manage();
    }

    let assigned_systems = match count_systems_in_environment(&pool, environment_id).await {
        Ok(count) => count,
        Err(_) => return internal_error("Failed to validate environment usage"),
    };

    if assigned_systems > 0 {
        return conflict("Cannot delete environment while systems are still assigned");
    }

    match delete_environment(&pool, environment_id).await {
        Ok(0) => not_found(),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => internal_error("Failed to delete environment"),
    }
}

/// `PATCH /api/v1/environments/:id`
///
/// Updates environment metadata. Admin role required.
pub async fn update_environment_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(environment_id): Path<Uuid>,
    Json(payload): Json<UpdateEnvironmentRequest>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_manage_environments() {
        return forbidden_manage();
    }

    let name = payload.name.trim();
    if name.is_empty() {
        return bad_request("Environment name is required");
    }

    if name.len() > 50 {
        return bad_request("Environment name must be 50 characters or fewer");
    }

    let description = payload.description.as_deref().map(str::trim).filter(|v| !v.is_empty());
    let color_hex = payload.color_hex.trim();
    if !looks_like_hex_color(color_hex) {
        return bad_request("Environment color must be a valid #RRGGBB value");
    }

    match update_environment_metadata(&pool, environment_id, name, description, color_hex).await {
        Ok(Some(env)) => (StatusCode::OK, Json(env)).into_response(),
        Ok(None) => not_found(),
        Err(err) => {
            if is_unique_violation(&err) {
                conflict("Environment name already exists")
            } else {
                internal_error("Failed to update environment")
            }
        }
    }
}

/// `GET /api/v1/environments/:id`
///
/// Returns a single environment by ID with its required policies, scoped to the authenticated user.
///
/// Returns 404 if the environment does not exist or the user is not a member.
pub async fn get_environment_with_policies_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(environment_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let scoped_user_id = if has_admin_role(&roles) {
        None
    } else {
        Some(user_id)
    };

    // First check access to the environment
    match find_environment_for_user(&pool, environment_id, scoped_user_id).await {
        Ok(Some(_env)) => {
            // Now get with policies
            match get_environment_with_policies(&pool, environment_id).await {
                Ok(Some(env_with_policies)) => {
                    (StatusCode::OK, Json(env_with_policies)).into_response()
                }
                Ok(None) => not_found(),
                Err(_) => internal_error("Failed to load environment policies"),
            }
        }
        Ok(None) => not_found(),
        Err(_) => internal_error("Failed to load environment"),
    }
}

/// `GET /api/v1/policies`
///
/// Returns all available deployment policies.
/// These can be assigned as required policies to environments.
pub async fn list_policies_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    match list_deployment_policies(&pool).await {
        Ok(policies) => (StatusCode::OK, Json(policies)).into_response(),
        Err(_) => internal_error("Failed to load policies"),
    }
}

/// `PATCH /api/v1/environments/:id/policies`
///
/// Updates environment required policies (the baseline). Admin role required.
/// Environment policies serve as the baseline for all systems in that environment.
/// Systems can add more policies on top, but cannot remove the baseline.
pub async fn update_environment_policies_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(environment_id): Path<Uuid>,
    Json(payload): Json<UpdateEnvironmentPoliciesRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_manage_environments() {
        return forbidden_manage();
    }

    // Verify the environment exists
    match find_environment_for_user(&pool, environment_id, None).await {
        Ok(Some(_env)) => {
            // Persist the policy associations to the database
            match set_environment_required_policies(
                &pool,
                environment_id,
                &payload.required_policy_ids,
                Some(user_id),
            )
            .await
            {
                Ok(_) => {
                    tracing::info!(
                        "Updated environment {} policies to {:?}",
                        environment_id,
                        payload.required_policy_ids
                    );
                    
                    // Return the updated environment with policies
                    match get_environment_with_policies(&pool, environment_id).await {
                        Ok(Some(env_with_policies)) => {
                            (StatusCode::OK, Json(env_with_policies)).into_response()
                        }
                        Ok(None) => not_found(),
                        Err(_) => internal_error("Failed to fetch updated environment"),
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to update environment policies: {}", e);
                    internal_error("Failed to update environment policies")
                }
            }
        }
        Ok(None) => not_found(),
        Err(_) => internal_error("Failed to update environment policies"),
    }
}

fn highest_role(roles: &[AuthRole]) -> Option<Role> {
    if roles.contains(&AuthRole::Admin) {
        Some(Role::Admin)
    } else if roles.contains(&AuthRole::Operator) {
        Some(Role::Operator)
    } else if roles.contains(&AuthRole::Viewer) {
        Some(Role::Viewer)
    } else {
        None
    }
}

fn is_unique_violation(err: &anyhow::Error) -> bool {
    err.downcast_ref::<SqlxError>()
        .and_then(|sqlx_err| sqlx_err.as_database_error())
        .and_then(|db_err| db_err.code())
        .is_some_and(|code| code == "23505")
}

fn looks_like_hex_color(value: &str) -> bool {
    if value.len() != 7 {
        return false;
    }
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

// ─────────────────────────────────────────────────────────────────────────────
// Error Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn forbidden() -> axum::response::Response {
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

fn not_found() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "not_found".to_string(),
            message: "Environment not found".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn bad_request(message: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "validation_error".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn conflict(message: &str) -> axum::response::Response {
    (
        StatusCode::CONFLICT,
        Json(ApiError {
            error: "conflict".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn forbidden_manage() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Admin privileges are required".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn internal_error(message: &str) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "internal_error".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use sqlx::postgres::PgPoolOptions;

    fn make_test_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct")
    }

    #[tokio::test]
    async fn list_environments_requires_authenticated_role() {
        let pool = make_test_pool();

        let response = list_environments(State(pool), HeaderMap::new())
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_environment_requires_authenticated_role() {
        let pool = make_test_pool();

        let response = get_environment(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn environment_summary_serializes_correctly() {
        let summary = EnvironmentSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            name: "production".to_string(),
            description: Some("Live fleet".to_string()),
            color_hex: "#0F766E".to_string(),
            is_active: true,
            system_count: 12,
        };

        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["name"], "production");
        assert_eq!(json["color_hex"], "#0F766E");
        assert_eq!(json["system_count"], 12);
        assert_eq!(json["is_active"], true);
    }

    #[test]
    fn environment_summary_with_no_description_serializes() {
        let summary = EnvironmentSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            name: "staging".to_string(),
            description: None,
            color_hex: "#B45309".to_string(),
            is_active: true,
            system_count: 0,
        };

        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["description"], serde_json::Value::Null);
        assert_eq!(json["system_count"], 0);
    }
}
