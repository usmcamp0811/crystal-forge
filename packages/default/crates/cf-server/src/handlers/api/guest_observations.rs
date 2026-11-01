//! Reads existing parent evidence; never contacts agents or claims refresh success.

use crate::{
    auth::models::Role,
    handlers::api::rbac::authenticated_user_roles,
    models::auth_identity::AuthRole,
    queries::{
        guest_observations,
        systems::{find_system_access_row, get_user_environment_membership_ids},
    },
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use sqlx::PgPool;
use uuid::Uuid;

/// Returns the latest report for an authorized guest's current association.
/// A JSON null means unknown, detached, revoked, or not external. Historical rows
/// remain stored. Observation time, not HTTP success, establishes evidence age.
pub async fn latest(
    State(pool): State<PgPool>,
    Path(guest_id): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let role = if roles.contains(&AuthRole::Admin) {
        Role::Admin
    } else if roles.contains(&AuthRole::Operator) {
        Role::Operator
    } else if roles.contains(&AuthRole::Viewer) {
        Role::Viewer
    } else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let memberships = match get_user_environment_membership_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let access = match find_system_access_row(&pool, guest_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !role.can_access_system_environment(access.environment_id, &memberships) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match guest_observations::latest(&pool, guest_id, access.environment_id).await {
        Ok(observation) => Json(observation).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
