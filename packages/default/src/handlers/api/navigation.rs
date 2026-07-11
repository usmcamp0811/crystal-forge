//! API handler for sidebar navigation badge counts.

use axum::{Json, extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse};

use crate::api::models::NavigationBadges;
use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::require_viewer_or_above;
use crate::queries::navigation::fetch_navigation_badges;

/// GET /api/v1/navigation/badges
///
/// Returns badge counts for all sidebar navigation entries. Requires
/// viewer-or-above access, matching the summarized surfaces. The UI should
/// poll this endpoint approximately every 30 seconds.
pub async fn get_navigation_badges(
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_viewer_or_above(&state.pool, &headers).await.is_none() {
        return forbidden_viewer();
    }

    match fetch_navigation_badges(&state.pool).await {
        Ok(badges) => (StatusCode::OK, Json(badges)).into_response(),
        Err(e) => {
            tracing::error!("Failed to fetch navigation badges: {e:#}");
            // Return zeros rather than a 500 — the sidebar degrading gracefully
            // is better than an error flash on every page load.
            (StatusCode::OK, Json(NavigationBadges::default())).into_response()
        }
    }
}

fn forbidden_viewer() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(crate::api::models::ApiError {
            error: "forbidden".to_string(),
            message: "Viewer, operator, or admin privileges are required".to_string(),
            details: None,
        }),
    )
        .into_response()
}
