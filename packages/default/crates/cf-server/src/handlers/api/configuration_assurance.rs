//! Exposes read-only configuration evidence under viewer/environment RBAC.

use crate::{
    handlers::api::rbac::authenticated_user_roles, models::auth_identity::AuthRole,
    queries::configuration_assurance,
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use sqlx::PgPool;
use uuid::Uuid;

/// Returns configuration evidence without establishing Current runtime authority.
///
/// Admins read all registrations. Operators and viewers require current
/// environment membership. Missing and inaccessible systems return 404;
/// unauthorized roles return 403. Database failures return 500.
pub async fn get(
    State(pool): State<PgPool>,
    Path(system_id): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let admin = roles.contains(&AuthRole::Admin);
    if !admin && !roles.contains(&AuthRole::Operator) && !roles.contains(&AuthRole::Viewer) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match configuration_assurance::fetch(&pool, system_id, (!admin).then_some(user_id)).await {
        Ok(Some(evidence)) => Json(evidence).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%system_id, error = %error, "configuration assurance read failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
