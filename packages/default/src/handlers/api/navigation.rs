//! API handlers for sidebar navigation badge counts and their per-user
//! acknowledgment baseline.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::api::models::{ApiError, NavigationBadges};
use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::{
    authenticated_user_roles, has_admin_role, has_viewer_or_above_role, require_viewer_or_above,
};
use crate::queries::navigation::{fetch_navigation_badges, upsert_user_alert_acknowledgment};
use crate::queries::systems::get_user_environment_membership_ids;

const VALID_CATEGORIES: &[&str] = &[
    "systems",
    "flakes",
    "environments",
    "builds",
    "evals",
    "cves",
];

/// GET /api/v1/navigation/badges
///
/// Returns badge counts for all sidebar navigation entries, computed relative
/// to the requesting user's last acknowledgment of each category. Requires
/// viewer-or-above access, matching the summarized surfaces. The UI should
/// poll this endpoint approximately every 30 seconds.
///
/// Systems and Environments counts are scoped to the requesting user's
/// environment memberships (admins see the fleet-wide total), matching the
/// same visibility rule as `GET /api/v1/systems` and `GET /api/v1/environments`
/// — otherwise a non-admin operator/viewer could see an attention badge for
/// systems/environments they cannot actually see in those views.
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

    let is_admin = has_admin_role(&roles);
    let member_environment_ids = if is_admin {
        Vec::new()
    } else {
        match get_user_environment_membership_ids(&state.pool, user_id).await {
            Ok(ids) => ids.into_iter().collect(),
            Err(e) => {
                tracing::warn!(
                    "Failed to load environment memberships for navigation badges (user {user_id}): {e:#}"
                );
                Vec::new()
            }
        }
    };

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
    /// The `observed_at` cursor from the NavigationBadges response the client
    /// was displaying when the user acknowledged. The server uses this as
    /// `last_seen_at` so only failures that arrived *after* the rendered
    /// snapshot count as new. Falls back to `NOW()` for older clients that
    /// do not send the field (or if parsing fails), which is less precise but
    /// still safe.
    pub observed_at: Option<DateTime<Utc>>,
    /// Current attention count for this category at acknowledgment time
    /// (used as the count-diff baseline for systems/environments; ignored by
    /// timestamp-based categories which use `observed_at` as their cutoff).
    #[serde(default)]
    pub current_count: i64,
    /// MD5 fingerprint of the alerting-ID set from the badge response the
    /// client was showing. Echoed back from `NavigationBadges.systems_fingerprint`
    /// or `.environments_fingerprint` so replacement failures re-surface.
    #[serde(default)]
    pub fingerprint: Option<String>,
}

/// POST /api/v1/navigation/acknowledge
///
/// Records that the requesting user has acknowledged (visited, or opened the
/// failures tab for) a given alert category. Persists per-user so the
/// corresponding badge stays hidden across page refresh, browser restart, and
/// re-login until something new appears — see `queries::navigation` for the
/// per-category "new since" computation this feeds.
pub async fn acknowledge_navigation_category(
    State(state): State<CFState>,
    headers: HeaderMap,
    Json(payload): Json<AcknowledgeNavigationCategoryRequest>,
) -> impl IntoResponse {
    let Some(user_id) = require_viewer_or_above(&state.pool, &headers).await else {
        return forbidden_viewer();
    };

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

    let observed_at = payload.observed_at.unwrap_or_else(Utc::now);
    match upsert_user_alert_acknowledgment(
        &state.pool,
        user_id,
        &payload.category,
        observed_at,
        payload.current_count,
        payload.fingerprint.as_deref(),
    )
    .await
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
        Err(e) => {
            tracing::error!(
                "Failed to record alert acknowledgment for user {} category {}: {e:#}",
                user_id,
                payload.category
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to record acknowledgment".to_string(),
                    details: None,
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
