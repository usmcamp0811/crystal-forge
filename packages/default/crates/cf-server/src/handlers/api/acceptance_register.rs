//! Serves the authenticated read-only projection of source-owned acceptances.
//!
//! Register visibility does not authorize waiver or CVE disposition mutations.

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use sqlx::PgPool;

use crate::{
    auth::extractors::RequireAuth,
    queries::acceptance_register::{self, AcceptanceListQuery, AcceptanceReadError},
};

/// Returns an authorized page of policy waivers and CVE disposition decisions.
///
/// The decision's source type and UUID are stable identities, but the server
/// must recheck source-specific permissions and evidence before any mutation.
pub async fn list(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    query: Result<Query<AcceptanceListQuery>, QueryRejection>,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"invalid_query","message":"Invalid acceptance query"})),
            )
                .into_response();
        }
    };
    match acceptance_register::list(&pool, user.user_id, &query).await {
        Ok(page) => Json(page).into_response(),
        Err(AcceptanceReadError::Validation(message)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_query","message":message})),
        )
            .into_response(),
        Err(AcceptanceReadError::Forbidden) => (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"forbidden","message":"Insufficient permissions"})),
        )
            .into_response(),
        Err(error) => {
            tracing::error!(error=%error, "Acceptance register read failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"internal_error","message":"Acceptance register read failed"})),
            )
                .into_response()
        }
    }
}
