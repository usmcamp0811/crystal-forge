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
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{ApiError, EnvironmentSummary};
use crate::handlers::api::rbac::{authenticated_user_roles, has_admin_role};
use crate::queries::environments::{find_environment_for_user, list_environments_for_user};

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
        use chrono::Utc;

        let summary = EnvironmentSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            name: "production".to_string(),
            description: Some("Live fleet".to_string()),
            is_active: true,
            system_count: 12,
        };

        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["name"], "production");
        assert_eq!(json["system_count"], 12);
        assert_eq!(json["is_active"], true);
    }

    #[test]
    fn environment_summary_with_no_description_serializes() {
        let summary = EnvironmentSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            name: "staging".to_string(),
            description: None,
            is_active: true,
            system_count: 0,
        };

        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["description"], serde_json::Value::Null);
        assert_eq!(json["system_count"], 0);
    }
}
