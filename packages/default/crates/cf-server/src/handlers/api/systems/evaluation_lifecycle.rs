//! Mutates explicit operator authority for evaluation display metadata.
//!
//! Classification is independent of policy resolution and deployment. Each PUT
//! atomically records its authenticated actor and an audit event. Historical
//! evaluations keep the classification captured before execution.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    authenticated_user_roles, forbidden, forbidden_mutation, highest_role, internal_error,
    not_found, require_csrf,
};
use crate::auth::models::Role;

/// Replaces one system's explicit display classification.
///
/// The `greenfield` key is required. Null explicitly sets unknown; an omitted
/// key or any other type is rejected. Extra fields cannot inject audit payloads.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetEvaluationLifecycleRequest {
    /// True means greenfield, false means established, and null means unknown.
    #[serde(deserialize_with = "deserialize_greenfield")]
    pub greenfield: Option<bool>,
}

fn deserialize_greenfield<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    Option::<bool>::deserialize(deserializer)
}

/// Returns the explicit classification accepted by the operator mutation.
#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluationLifecycleResponse {
    /// Current registered system identity.
    pub system_id: Uuid,
    /// Accepted display classification; null explicitly means unknown.
    pub greenfield: Option<bool>,
}

/// Replaces a current system's display-only lifecycle classification.
///
/// Requires an authenticated operator/admin and the existing CSRF contract.
/// Operators require current environment membership; hidden and missing systems
/// return the same 404. Viewers receive 403. Success returns 200 only after the
/// classification and audit event commit together. Database failures return 500
/// with no partial mutation. This action never queues evaluation or deployment.
pub async fn put_evaluation_lifecycle(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(request): Json<SetEvaluationLifecycleRequest>,
) -> Response {
    let Some((actor, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    let Some(role) = highest_role(&roles) else {
        return forbidden();
    };
    if !role.can_mutate_systems() {
        return forbidden_mutation();
    }
    if let Err(response) = require_csrf(&headers) {
        return response;
    }
    match set_classification(&pool, actor, role, system_id, request.greenfield).await {
        Ok(true) => (
            StatusCode::OK,
            Json(EvaluationLifecycleResponse {
                system_id,
                greenfield: request.greenfield,
            }),
        )
            .into_response(),
        Ok(false) => not_found(),
        Err(_) => internal_error("Failed to update evaluation lifecycle"),
    }
}

// SECURITY: Use the authenticated actor and Role policy, never body-supplied
// authority. Lock the current system before reading its scope; a concurrent move
// or deletion must wait. Membership share locks retain operator authority until
// commit. Classification, actor/timestamp and audit are one atomic transaction.
// A denied/missing target writes nothing. UUID audit identifiers survive normal
// user cleanup without storing credentials, headers, config, or request bodies.
async fn set_classification(
    pool: &PgPool,
    actor: Uuid,
    role: Role,
    system_id: Uuid,
    greenfield: Option<bool>,
) -> Result<bool, sqlx::Error> {
    if !role.can_mutate_systems() {
        return Ok(false);
    }
    let mut tx = pool.begin().await?;
    let Some((environment,)) = sqlx::query_as::<_, (Option<Uuid>,)>(
        "SELECT environment_id FROM systems WHERE id = $1 FOR UPDATE",
    )
    .bind(system_id)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(false);
    };
    let memberships: Vec<Uuid> = sqlx::query_scalar(
        "SELECT environment_id FROM user_environment_memberships
         WHERE user_id = $1 AND environment_id = $2 FOR SHARE",
    )
    .bind(actor)
    .bind(environment)
    .fetch_all(&mut *tx)
    .await?;
    if !role.can_access_system_environment(environment, &memberships.into_iter().collect()) {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO system_evaluation_lifecycle (system_id, greenfield, classified_by)
         VALUES ($1, $2, $3) ON CONFLICT (system_id) DO UPDATE
         SET greenfield = EXCLUDED.greenfield, classified_by = EXCLUDED.classified_by,
             classified_at = CURRENT_TIMESTAMP",
    )
    .bind(system_id)
    .bind(greenfield)
    .bind(actor)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO admin_audit_events
             (actor_user_id, actor_identifier, action, target, metadata)
         VALUES ($1, $2, 'system_evaluation_lifecycle_updated', $3, $4)",
    )
    .bind(actor)
    .bind(actor.to_string())
    .bind(system_id.to_string())
    .bind(serde_json::json!({"system_id": system_id, "greenfield": greenfield}))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

#[cfg(test)]
mod tests;
