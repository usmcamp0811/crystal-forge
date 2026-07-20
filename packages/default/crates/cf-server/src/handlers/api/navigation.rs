//! API handlers for sidebar navigation badge counts and per-occurrence
//! dismissal.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::models::{ApiError, NavigationBadges};
use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::{
    authenticated_user_roles, has_admin_role, has_viewer_or_above_role,
};
use crate::queries::attention;
use crate::queries::navigation::fetch_navigation_badges;
use crate::queries::systems::get_user_environment_membership_ids;

const VALID_CATEGORIES: &[&str] = &[
    "systems",
    "flakes",
    "environments",
    "builds",
    "evals",
    "cves",
];

async fn resolve_navigation_scope(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    roles: &[crate::models::auth_identity::AuthRole],
) -> (bool, Vec<Uuid>) {
    let is_admin = has_admin_role(roles);
    let member_environment_ids = if is_admin {
        Vec::new()
    } else {
        match get_user_environment_membership_ids(pool, user_id).await {
            Ok(ids) => ids.into_iter().collect(),
            Err(e) => {
                tracing::warn!(
                    "Failed to load environment memberships for navigation (user {user_id}): {e:#}"
                );
                Vec::new()
            }
        }
    };
    (is_admin, member_environment_ids)
}

/// GET /api/v1/navigation/badges
///
/// Returns badge counts for all sidebar navigation entries. Each attention
/// count is the number of eligible undismissed canonical occurrences in the
/// last 24 hours for the requesting user. Requires viewer-or-above access.
///
/// Systems and Environments counts are scoped to the requesting user's
/// environment memberships (admins see the fleet-wide total), matching the
/// same visibility rule as `GET /api/v1/systems` and `GET /api/v1/environments`.
pub async fn get_navigation_badges(
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden_viewer();
    };
    if !has_viewer_or_above_role(&roles) {
        return forbidden_viewer();
    }

    let (is_admin, member_environment_ids) =
        resolve_navigation_scope(&state.pool, user_id, &roles).await;

    match fetch_navigation_badges(&state.pool, user_id, is_admin, &member_environment_ids).await {
        Ok(badges) => (StatusCode::OK, Json(badges)).into_response(),
        Err(e) => {
            tracing::error!("Failed to fetch navigation badges: {e:#}");
            // Return zeros rather than a 500 — the sidebar degrading gracefully
            // is better than an error flash on every page load.
            (StatusCode::OK, Json(NavigationBadges::default())).into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AcknowledgeNavigationCategoryRequest {
    pub category: String,
    /// The `observed_at` cursor from the `NavigationBadges` response the client
    /// was displaying when the user dismissed. Only occurrences opened at or
    /// before this cursor may be dismissed.
    pub observed_at: DateTime<Utc>,
    /// Exact server canonical occurrence IDs represented by the rendered
    /// dataset or action. The server validates each id belongs to the requested
    /// category and is visible to the requesting user.
    #[serde(default)]
    pub occurrence_ids: Vec<Uuid>,
}

/// POST /api/v1/navigation/acknowledge
///
/// Persists dismissal of the supplied occurrence IDs for the requesting user.
/// Returns the refreshed `NavigationBadges` state so the client can update the
/// sidebar without an extra round-trip.
pub async fn acknowledge_navigation_category(
    State(state): State<CFState>,
    headers: HeaderMap,
    Json(payload): Json<AcknowledgeNavigationCategoryRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden_viewer();
    };
    if !has_viewer_or_above_role(&roles) {
        return forbidden_viewer();
    }

    if !VALID_CATEGORIES.contains(&payload.category.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message: format!(
                    "Unknown alert category '{}'; expected one of {:?}",
                    payload.category, VALID_CATEGORIES
                ),
                details: None,
            }),
        )
            .into_response();
    }

    let (is_admin, member_environment_ids) =
        resolve_navigation_scope(&state.pool, user_id, &roles).await;

    let dismissal_result = attention::dismiss_occurrences(
        &state.pool,
        user_id,
        &payload.category,
        payload.observed_at,
        &payload.occurrence_ids,
        is_admin,
        &member_environment_ids,
    )
    .await;

    let counts = match dismissal_result {
        Ok(counts) => counts,
        Err(e) => {
            let status = if e.to_string().contains("not found")
                || e.to_string().contains("belongs to category")
                || e.to_string().contains("opened after")
            {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            tracing::error!(
                "Failed to dismiss occurrences for user {} category {}: {e:#}",
                user_id,
                payload.category
            );
            return (
                status,
                Json(ApiError {
                    error: "dismissal_failed".to_string(),
                    message: "Failed to dismiss occurrences".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let badges = match fetch_navigation_badges(
        &state.pool,
        user_id,
        is_admin,
        &member_environment_ids,
    )
    .await
    {
        Ok(badges) => badges,
        Err(e) => {
            // If dismissal succeeded but the refresh query failed, return the
            // counts we already know so the client can update optimistically.
            tracing::error!(
                "Dismissal succeeded but failed to refresh badges for user {}: {e:#}",
                user_id
            );
            let mut fallback = NavigationBadges::default();
            fallback.observed_at = Some(payload.observed_at);
            fallback.systems_attention = counts.systems_attention;
            fallback.flakes_errored = counts.flakes_errored;
            fallback.environments_attention = counts.environments_attention;
            fallback.builds_failed_new = counts.builds_failed_new;
            fallback.evals_failed_new = counts.evals_failed_new;
            fallback.cves_critical_new = counts.cves_critical_new;
            return (StatusCode::OK, Json(fallback)).into_response();
        }
    };

    (StatusCode::OK, Json(badges)).into_response()
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
