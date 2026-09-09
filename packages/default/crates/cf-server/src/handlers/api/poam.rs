//! Exposes authenticated HTTP endpoints for POA&M lifecycle workflows.
//!
//! Handlers parse transport inputs, construct the actor scope, enforce CSRF on
//! mutations, and translate service failures into the shared structured POA&M
//! error response. Domain validation and persistence remain in the service.

use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::extractors::RequireAuth;
use crate::handlers::api::{auth_session::validate_csrf, rbac::extract_request_origin};
use crate::models::poam::*;
use crate::queries::poam::user_environment_ids;
use crate::services::poam::{self, PoamActor, PoamError, SystemClock};

fn error_response(error: PoamError) -> Response {
    let (status, code, message, details) = match error {
        PoamError::NotFound => (
            StatusCode::NOT_FOUND,
            "not_found",
            "POA&M resource was not found".into(),
            None,
        ),
        PoamError::Forbidden => (
            StatusCode::FORBIDDEN,
            "forbidden",
            "Insufficient permissions".into(),
            None,
        ),
        PoamError::Validation(code, message) => (StatusCode::BAD_REQUEST, code, message, None),
        PoamError::Conflict(code, message) => (StatusCode::CONFLICT, code, message, None),
        PoamError::Precondition(code, message, details) => {
            (StatusCode::PRECONDITION_FAILED, code, message, details)
        }
        PoamError::Database(error) => {
            tracing::error!(error=%error,"POA&M request failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "POA&M request failed".into(),
                None,
            )
        }
    };
    (
        status,
        Json(json!({"error":code,"message":message,"details":details})),
    )
        .into_response()
}

async fn actor(
    pool: &PgPool,
    user: crate::auth::extractors::AuthenticatedUser,
    headers: &HeaderMap,
) -> Result<PoamActor, Response> {
    let identifier = sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id=$1")
        .bind(user.user_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| error_response(PoamError::Database(anyhow::anyhow!("actor lookup failed"))))?
        .unwrap_or_else(|| user.user_id.to_string());
    let environment_ids = user_environment_ids(pool, user.user_id)
        .await
        .map_err(|e| error_response(PoamError::Database(e)))?;
    Ok(PoamActor {
        user_id: user.user_id,
        identifier,
        is_admin: user.is_admin(),
        can_mutate: user.is_operator_or_higher(),
        environment_ids,
        request_origin: extract_request_origin(headers),
    })
}

fn csrf(headers: &HeaderMap) -> Result<(), Response> {
    validate_csrf(headers).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error":"csrf_validation_failed",
                "message":"CSRF validation failed",
                "details":null
            })),
        )
            .into_response()
    })
}

fn json_body<T>(
    body: Result<Json<T>, JsonRejection>,
    message: &'static str,
) -> Result<T, Response> {
    body.map(|Json(value)| value).map_err(|rejection| {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({
                    "error":"payload_too_large",
                    "message":"Request body exceeds the configured limit",
                    "details":null
                })),
            )
                .into_response()
        } else {
            error_response(PoamError::Validation("invalid_body", message.into()))
        }
    })
}

fn query_body<T>(
    query: Result<Query<T>, QueryRejection>,
    code: &'static str,
    message: &'static str,
) -> Result<T, Response> {
    query
        .map(|Query(value)| value)
        .map_err(|_| error_response(PoamError::Validation(code, message.into())))
}

fn path_body<T>(path: Result<Path<T>, PathRejection>) -> Result<T, Response> {
    path.map(|Path(value)| value).map_err(|_| {
        error_response(PoamError::Validation(
            "invalid_path",
            "Malformed POA&M path parameter".into(),
        ))
    })
}

/// Lists POA&Ms visible to the authenticated actor.
///
/// Returns a structured error response when authentication, query validation,
/// actor lookup, or POA&M listing fails.
pub async fn list(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<PoamListQuery>, QueryRejection>,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(_) => {
            return error_response(PoamError::Validation(
                "invalid_query",
                "Malformed POA&M list query".into(),
            ));
        }
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::list(&pool, &actor, &query, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}

/// Selects finding relationships and optional bounded history pages.
///
/// Omitting both history fields selects the bounded compatibility page for
/// deployed clients. `history_offset` requires `history_limit`.
#[derive(Deserialize)]
pub struct FindingRelationshipsQuery {
    /// Contains comma-separated current composite assessment IDs.
    pub assessment_ids: Option<String>,
    /// Contains comma-separated stable finding IDs.
    pub finding_ids: Option<String>,
    /// Limits historical POA&Ms independently for each requested finding.
    pub history_limit: Option<i64>,
    /// Skips this many historical POA&Ms for each requested finding.
    pub history_offset: Option<i64>,
}

/// Selects assignment relationships and optional bounded history pages.
///
/// Omitting both history fields selects the bounded compatibility page for
/// deployed clients. `history_offset` requires `history_limit`.
#[derive(Deserialize)]
pub struct AssignmentRelationshipsQuery {
    /// Contains comma-separated immutable assignment-version IDs.
    pub ids: String,
    /// Limits POA&Ms independently for each requested assignment version.
    pub history_limit: Option<i64>,
    /// Skips this many POA&Ms for each requested assignment version.
    pub history_offset: Option<i64>,
}

/// Selects a finding observation and page for compatible-POA&M search.
///
/// Callers provide either `assessment_id` alone or the complete stable finding
/// observation fields. Partial combinations are invalid.
#[derive(Deserialize)]
pub struct CompatiblePoamsQuery {
    /// Identifies a current composite assessment.
    pub assessment_id: Option<Uuid>,
    /// Identifies a stable finding for legacy observation lookup.
    pub finding_id: Option<Uuid>,
    /// Identifies the authoritative legacy observation source.
    pub observation_source: Option<FindingObservationSource>,
    /// Identifies the source record within the observation source.
    pub observation_source_id: Option<String>,
    /// Identifies the immutable policy version observed by the source.
    pub observation_policy_version_id: Option<Uuid>,
    /// Binds the request to the exact observed evidence.
    pub observation_token: Option<String>,
    /// Filters compatible POA&Ms by text when present.
    pub q: Option<String>,
    /// Limits the number of returned summaries.
    pub limit: Option<i64>,
    /// Skips this many compatible summaries.
    pub offset: Option<i64>,
}

fn relationship_ids(value: &str, field: &str) -> Result<Vec<Uuid>, Response> {
    let raw = value.split(',').collect::<Vec<_>>();
    if raw.is_empty() || raw.iter().any(|id| id.trim().is_empty()) {
        return Err(error_response(PoamError::Validation(
            "invalid_ids",
            format!("{field} must contain at least one UUID and no empty values"),
        )));
    }
    if raw.len() > 100 {
        return Err(error_response(PoamError::Validation(
            "too_many_ids",
            format!("At most 100 {field} are allowed"),
        )));
    }
    let mut ids = Vec::with_capacity(raw.len());
    for id in raw {
        let id = Uuid::parse_str(id.trim()).map_err(|_| {
            error_response(PoamError::Validation(
                "invalid_ids",
                format!("{field} must be a comma-separated list of UUIDs"),
            ))
        })?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// Returns POA&M relationships for visible assessments or stable findings.
///
/// Returns a structured error response when authentication, query or ID
/// validation, actor lookup, visibility filtering, or relationship loading
/// fails.
pub async fn finding_relationships(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<FindingRelationshipsQuery>, QueryRejection>,
) -> Response {
    let query = match query_body(
        query,
        "invalid_ids",
        "Malformed assessment relationship query",
    ) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let result = match (
        query.assessment_ids.as_deref(),
        query.finding_ids.as_deref(),
    ) {
        (Some(value), None) => match relationship_ids(value, "assessment_ids") {
            Ok(ids) => {
                poam::finding_relationships(
                    &pool,
                    &actor,
                    &ids,
                    query.history_limit,
                    query.history_offset,
                    &SystemClock,
                )
                .await
            }
            Err(response) => return response,
        },
        (None, Some(value)) => match relationship_ids(value, "finding_ids") {
            Ok(ids) => {
                poam::finding_relationships_by_finding(
                    &pool,
                    &actor,
                    &ids,
                    query.history_limit,
                    query.history_offset,
                    &SystemClock,
                )
                .await
            }
            Err(response) => return response,
        },
        _ => {
            return error_response(PoamError::Validation(
                "invalid_ids",
                "Provide exactly one of assessment_ids or finding_ids".into(),
            ));
        }
    };
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}

/// Lists active POA&Ms compatible with one authoritative finding observation.
///
/// Returns a structured error response when authentication, query validation,
/// actor lookup, evidence validation, or compatible search fails.
pub async fn compatible_poams(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<CompatiblePoamsQuery>, QueryRejection>,
) -> Response {
    let query = match query_body(query, "invalid_query", "Malformed compatible-POA&M query") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let result = match (
        query.assessment_id,
        query.finding_id,
        query.observation_source,
        query.observation_source_id,
        query.observation_policy_version_id,
        query.observation_token,
    ) {
        (Some(assessment_id), None, None, None, None, None) => {
            poam::compatible_for_assessment(
                &pool,
                &actor,
                assessment_id,
                query.q.as_deref(),
                query.limit,
                query.offset,
                &SystemClock,
            )
            .await
        }
        (
            None,
            Some(finding_id),
            Some(source),
            Some(source_id),
            Some(policy_version_id),
            Some(token),
        ) => {
            poam::compatible_for_finding(
                &pool,
                &actor,
                finding_id,
                &FindingObservationReference {
                    source,
                    source_id,
                    policy_version_id,
                    token,
                },
                query.q.as_deref(),
                query.limit,
                query.offset,
                &SystemClock,
            )
            .await
        }
        _ => Err(PoamError::Validation(
            "invalid_finding_observation",
            "Provide assessment_id or a complete finding observation reference".into(),
        )),
    };
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}

/// Returns POA&M relationships for visible immutable assignment versions.
///
/// Returns a structured error response when authentication, query or ID
/// validation, actor lookup, visibility filtering, or relationship loading
/// fails.
pub async fn assignment_relationships(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<AssignmentRelationshipsQuery>, QueryRejection>,
) -> Response {
    let query = match query_body(
        query,
        "invalid_ids",
        "Malformed assignment relationship query",
    ) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let ids = match relationship_ids(&query.ids, "ids") {
        Ok(ids) => ids,
        Err(response) => return response,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match poam::assignment_relationships(
        &pool,
        &actor,
        &ids,
        query.history_limit,
        query.history_offset,
        &SystemClock,
    )
    .await
    {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}

/// Returns one visible POA&M with requested bounded history pages.
///
/// Returns a structured error response when authentication, path or query
/// validation, actor lookup, visibility checks, or detail loading fails.
pub async fn get(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<PoamDetailQuery>, QueryRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Query(query) = match query {
        Ok(query) => query,
        Err(_) => {
            return error_response(PoamError::Validation(
                "invalid_query",
                "Malformed POA&M detail query".into(),
            ));
        }
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::detail_with_history(&pool, &actor, id, &query, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Creates a POA&M from a current failing finding.
///
/// Returns a structured error response when CSRF, authentication, body
/// validation, authorization, evidence validation, or creation fails.
pub async fn create(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    body: Result<Json<CreatePoamRequest>, JsonRejection>,
) -> Response {
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let Json(body) = match body {
        Ok(body) => body,
        Err(_) => {
            return error_response(PoamError::Validation(
                "invalid_body",
                "Malformed POA&M request".into(),
            ));
        }
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::create(&pool, &actor, body, &SystemClock).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => error_response(e),
    }
}
/// Updates mutable fields on one POA&M.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, authorization, revision checks, or persistence fails.
pub async fn update(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<UpdatePoamRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let Json(body) = match body {
        Ok(body) => body,
        Err(_) => {
            return error_response(PoamError::Validation(
                "invalid_body",
                "Malformed POA&M request".into(),
            ));
        }
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::update(&pool, &actor, id, body, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Transitions one POA&M between active workflow states.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, authorization, lifecycle checks, or persistence fails.
pub async fn transition(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<TransitionPoamRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let Json(body) = match body {
        Ok(body) => body,
        Err(_) => {
            return error_response(PoamError::Validation(
                "invalid_body",
                "Malformed POA&M status".into(),
            ));
        }
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::transition(&pool, &actor, id, body, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Adds an audited note to one POA&M.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, authorization, revision checks, or persistence fails.
pub async fn note(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<AddNoteRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match json_body(body, "Malformed POA&M note") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::add_note(&pool, &actor, id, body, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Adds a milestone to one POA&M.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, authorization, revision checks, or persistence fails.
pub async fn add_milestone(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<AddMilestoneRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match json_body(body, "Malformed POA&M milestone") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::add_milestone(&pool, &actor, id, body, &SystemClock).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => error_response(e),
    }
}
/// Updates one POA&M milestone.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, authorization, revision checks, or persistence fails.
pub async fn update_milestone(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    body: Result<Json<UpdateMilestoneRequest>, JsonRejection>,
) -> Response {
    let (id, mid) = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match json_body(body, "Malformed POA&M milestone") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::update_milestone(&pool, &actor, id, mid, body, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Removes one POA&M milestone.
///
/// Returns a structured error response when CSRF, authentication, path,
/// query validation, authorization, revision checks, or persistence fails.
pub async fn remove_milestone(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    query: Result<Query<RevisionRequest>, QueryRejection>,
) -> Response {
    let (id, mid) = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match query_body(query, "invalid_revision", "Malformed POA&M revision") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::remove_milestone(&pool, &actor, id, mid, body.revision, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Links a current failing finding to one POA&M.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, authorization, evidence checks, or persistence fails.
pub async fn link_finding(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<AddFindingRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match json_body(body, "Malformed finding link request") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::link_finding(&pool, &actor, id, body, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Retires one active finding link from a POA&M.
///
/// Returns a structured error response when CSRF, authentication, path,
/// query validation, authorization, revision checks, or persistence fails.
pub async fn unlink_finding(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    query: Result<Query<RevisionRequest>, QueryRejection>,
) -> Response {
    let (id, fid) = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match query_body(query, "invalid_revision", "Malformed POA&M revision") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::unlink_finding(&pool, &actor, id, fid, body.revision, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Links an immutable assignment version to one POA&M.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, authorization, compatibility checks, or persistence fails.
pub async fn link_assignment(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<AssignmentReferenceRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match json_body(body, "Malformed assignment link request") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::link_assignment(&pool, &actor, id, body, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Removes an immutable assignment-version reference from one POA&M.
///
/// Returns a structured error response when CSRF, authentication, path,
/// query validation, authorization, revision checks, or persistence fails.
pub async fn unlink_assignment(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    query: Result<Query<RevisionRequest>, QueryRejection>,
) -> Response {
    let (id, aid) = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match query_body(query, "invalid_revision", "Malformed POA&M revision") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::unlink_assignment(&pool, &actor, id, aid, body.revision, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}

/// Selects a text-filtered offset page.
#[derive(Deserialize)]
pub struct SearchQuery {
    /// Filters results by text when present.
    pub q: Option<String>,
    /// Limits the number of returned records.
    pub limit: Option<i64>,
    /// Skips this many matching records.
    pub offset: Option<i64>,
}
/// Lists current failing findings compatible with one POA&M.
///
/// Returns a structured error response when authentication, path or query
/// validation, actor lookup, evidence checks, or candidate loading fails.
pub async fn compatible(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<SearchQuery>, QueryRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let q = match query_body(query, "invalid_query", "Malformed compatible-finding query") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::compatible(
        &pool,
        &actor,
        id,
        q.q.as_deref(),
        q.limit.unwrap_or(25),
        q.offset.unwrap_or(0),
    )
    .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Verifies and closes an awaiting-verification POA&M.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, authorization, closure preconditions, or persistence fails.
pub async fn close(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<RevisionRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match json_body(body, "Malformed POA&M close request") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::close(&pool, &actor, id, body.revision, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Records a sealed verification attempt for one POA&M.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, authorization, verification preconditions, or persistence
/// fails.
pub async fn verify(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<RevisionRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match json_body(body, "Malformed POA&M verification request") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::verify(&pool, &actor, id, body.revision, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Reopens one completed POA&M.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, authorization, reopen preconditions, or persistence fails.
pub async fn reopen(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<RevisionRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match json_body(body, "Malformed POA&M reopen request") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::reopen(&pool, &actor, id, body.revision, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Creates a pending waiver request for a current failing finding.
///
/// Returns a structured error response when CSRF, authentication, body
/// validation, authorization, evidence checks, or persistence fails.
pub async fn create_waiver(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    body: Result<Json<CreateWaiverRequest>, JsonRejection>,
) -> Response {
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let body = match json_body(body, "Malformed finding waiver request") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::create_waiver(&pool, &actor, body).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => error_response(e),
    }
}
/// Lists waiver records for an authenticated administrator.
///
/// Returns a structured error response when authentication, query validation,
/// administrator authorization, actor lookup, or record loading fails.
pub async fn list_waivers(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<WaiverListQuery>, QueryRejection>,
) -> Response {
    let query = match query_body(query, "invalid_query", "Malformed waiver list query") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match poam::list_waivers(&pool, &actor, &query).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}
/// Returns one waiver record to an authenticated administrator.
///
/// Returns a structured error response when authentication, path validation,
/// administrator authorization, actor lookup, or record loading fails.
pub async fn get_waiver(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match poam::waiver(&pool, &actor, id).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}
/// Applies an administrator decision to one waiver.
///
/// Returns a structured error response when CSRF, authentication, path or body
/// validation, administrator authorization, lifecycle checks, or persistence
/// fails.
pub async fn decide_waiver(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<WaiverDecisionRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(e) = csrf(&headers) {
        return e;
    }
    let Json(body) = match body {
        Ok(body) => body,
        Err(_) => {
            return error_response(PoamError::Validation(
                "invalid_body",
                "Malformed waiver decision".into(),
            ));
        }
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !actor.is_admin {
        return error_response(PoamError::Forbidden);
    }
    match poam::decide_waiver(&pool, &actor, id, body, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Returns POA&M dashboard counts visible to the authenticated actor.
///
/// Returns a structured error response when authentication, actor lookup, or
/// aggregate loading fails.
pub async fn dashboard(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
) -> Response {
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::dashboard(&pool, &actor, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Returns the authenticated actor's paginated POA&M watchlist.
///
/// Returns a structured error response when authentication, query validation,
/// actor lookup, or watchlist loading fails.
pub async fn watchlist(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<SearchQuery>, QueryRejection>,
) -> Response {
    let q = match query_body(query, "invalid_query", "Malformed watchlist query") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::watchlist(
        &pool,
        &actor,
        q.limit.unwrap_or(25),
        q.offset.unwrap_or(0),
        &SystemClock,
    )
    .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Selects a bounded batch of resource IDs.
#[derive(Deserialize)]
pub struct BatchQuery {
    /// Contains comma-separated resource UUIDs.
    pub ids: String,
}

fn batch_ids(value: &str) -> Result<Vec<Uuid>, Response> {
    let raw = value.split(',').collect::<Vec<_>>();
    if raw.is_empty() || raw.iter().any(|id| id.trim().is_empty()) {
        return Err(error_response(PoamError::Validation(
            "invalid_ids",
            "ids must contain at least one UUID and no empty values".into(),
        )));
    }
    if raw.len() > 100 {
        return Err(error_response(PoamError::Validation(
            "too_many_ids",
            "At most 100 ids are allowed".into(),
        )));
    }
    let mut ids = raw
        .into_iter()
        .map(|id| {
            Uuid::parse_str(id.trim()).map_err(|_| {
                error_response(PoamError::Validation(
                    "invalid_ids",
                    "ids must be a comma-separated list of UUIDs".into(),
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}
/// Returns POA&M rollups for visible requested systems.
///
/// Returns a structured error response when authentication, query or ID
/// validation, actor lookup, scope expansion, or rollup loading fails.
pub async fn system_rollups(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<BatchQuery>, QueryRejection>,
) -> Response {
    let Query(q) = match query {
        Ok(query) => query,
        Err(_) => {
            return error_response(PoamError::Validation(
                "invalid_ids",
                "Malformed batch query".into(),
            ));
        }
    };
    let ids = match batch_ids(&q.ids) {
        Ok(ids) => ids,
        Err(response) => return response,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::system_rollups(&pool, &actor, &ids, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}
/// Returns POA&M rollups for visible requested bundle lineages.
///
/// Returns a structured error response when authentication, query or ID
/// validation, actor lookup, scope expansion, or rollup loading fails.
pub async fn bundle_rollups(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<BatchQuery>, QueryRejection>,
) -> Response {
    let Query(q) = match query {
        Ok(query) => query,
        Err(_) => {
            return error_response(PoamError::Validation(
                "invalid_ids",
                "Malformed batch query".into(),
            ));
        }
    };
    let ids = match batch_ids(&q.ids) {
        Ok(ids) => ids,
        Err(response) => return response,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match poam::bundle_rollups(&pool, &actor, &ids, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::session::{
        CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
    };
    use crate::handlers::agent_request::CFState;
    use crate::models::auth_identity::AuthRole;
    use crate::queries::auth_identity::{create_user_session, sync_user_role};
    use crate::queries::users::insert_user;
    use crate::queue::QueueNotifier;
    use crate::server::jobs::BackgroundJobRegistry;
    use axum::{Router, routing::get};
    use chrono::Utc;
    use std::sync::Arc;

    async fn session(pool: &PgPool, role: AuthRole) -> String {
        let suffix = Uuid::new_v4().simple().to_string();
        let user = insert_user(
            pool,
            &format!("poam-http-{suffix}@example.invalid"),
            Some("POAM HTTP Test"),
        )
        .await
        .unwrap();
        sync_user_role(pool, user.id, role).await.unwrap();
        let token = format!("session-{suffix}");
        create_user_session(
            pool,
            user.id,
            hash_token(&token),
            Utc::now() + chrono::Duration::hours(1),
            Some("poam-test".into()),
            Some("127.0.0.1".into()),
            "local".into(),
        )
        .await
        .unwrap();
        token
    }

    async fn server(pool: PgPool) -> String {
        let state = CFState::new(
            pool,
            crate::config::ServerConfig::default(),
            Arc::new(QueueNotifier::new()),
            BackgroundJobRegistry::new(),
        );
        let app = Router::new()
            .route("/api/v1/poams", get(list).post(create))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    #[sqlx::test]
    #[ignore = "requires test database creation privileges"]
    async fn http_requires_session_csrf_and_mutator_role(pool: PgPool) {
        let base = server(pool.clone()).await;
        let client = reqwest::Client::new();
        let unauthenticated = client
            .get(format!("{base}/api/v1/poams"))
            .send()
            .await
            .unwrap();
        assert_eq!(unauthenticated.status().as_u16(), 401);

        let viewer = session(&pool, AuthRole::Viewer).await;
        let body = json!({
            "assessment_id": Uuid::new_v4(),
            "title": "HTTP authorization",
            "risk": "high"
        });
        let no_csrf = client
            .post(format!("{base}/api/v1/poams"))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={viewer}"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(no_csrf.status().as_u16(), 403);

        let csrf = "poam-http-csrf";
        let viewer_forbidden = client
            .post(format!("{base}/api/v1/poams"))
            .header(
                "cookie",
                format!("{SESSION_COOKIE_NAME}={viewer}; {CSRF_COOKIE_NAME}={csrf}"),
            )
            .header(CSRF_HEADER_NAME.as_str(), csrf)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(viewer_forbidden.status().as_u16(), 403);

        let admin = session(&pool, AuthRole::Admin).await;
        let authenticated = client
            .post(format!("{base}/api/v1/poams"))
            .header(
                "cookie",
                format!("{SESSION_COOKIE_NAME}={admin}; {CSRF_COOKIE_NAME}={csrf}"),
            )
            .header(CSRF_HEADER_NAME.as_str(), csrf)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(authenticated.status().as_u16(), 404);
    }
}
