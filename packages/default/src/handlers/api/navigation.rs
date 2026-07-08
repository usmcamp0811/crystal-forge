//! API handler for sidebar navigation badge counts.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

use crate::api::models::NavigationBadges;
use crate::auth::extractors::RequireAuth;
use crate::handlers::agent_request::CFState;
use crate::queries::navigation::fetch_navigation_badges;

/// GET /api/v1/navigation/badges
///
/// Returns badge counts for all sidebar navigation entries.  Requires an
/// authenticated session; no elevated role required.  The UI should poll
/// this endpoint approximately every 30 seconds.
pub async fn get_navigation_badges(
    State(state): State<CFState>,
    _user: RequireAuth,
) -> impl IntoResponse {
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
