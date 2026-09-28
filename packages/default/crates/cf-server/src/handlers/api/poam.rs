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
    http::{HeaderMap, StatusCode, header},
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
use crate::services::register_poam_tabular::{self, ExportError};

pub(super) fn error_response(error: PoamError) -> Response {
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
        PoamError::ConflictDetails(code, message, details) => {
            (StatusCode::CONFLICT, code, message, Some(details))
        }
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

pub(super) async fn actor(
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

pub(super) fn csrf(headers: &HeaderMap) -> Result<(), Response> {
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

/// Selects a complete POA&M register download and its existing list filters.
#[derive(Deserialize)]
pub struct PoamExportQuery {
    /// Chooses `csv` or `xlsx`.
    pub format: String,
    /// Reuses the server-side register filters; page bounds do not limit export.
    #[serde(flatten)]
    pub filters: PoamListQuery,
}

/// Downloads the complete authorized POA&M selection as CSV or Excel XLSX.
///
/// The reader rechecks current roles and scope and collects all pages and
/// persisted finding links in one snapshot. An ambiguous or hidden source
/// scope fails the whole request instead of producing partial evidence.
pub async fn export(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<PoamExportQuery>, QueryRejection>,
) -> Response {
    let Ok(Query(query)) = query else {
        return error_response(PoamError::Validation(
            "invalid_query",
            "Invalid POA&M export query".into(),
        ));
    };
    if query.format != "csv" && query.format != "xlsx" {
        return error_response(PoamError::Validation(
            "invalid_format",
            "Unsupported export format".into(),
        ));
    }
    let actor = match actor(&pool, user, &headers).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let output = match register_poam_tabular::export(&pool, &actor, &query.filters, &SystemClock)
        .await
    {
        Ok(output) => output,
        Err(error) => {
            let (status, code) = match error {
                ExportError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
                ExportError::InvalidQuery => (StatusCode::BAD_REQUEST, "invalid_query"),
                ExportError::TooManyRows => (StatusCode::UNPROCESSABLE_ENTITY, "export_limit"),
                ExportError::AmbiguousScope => {
                    (StatusCode::UNPROCESSABLE_ENTITY, "incomplete_scope")
                }
                ExportError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "export_unavailable"),
            };
            return (
                status,
                Json(json!({"error":code,"message":error.to_string()})),
            )
                .into_response();
        }
    };
    let download = if query.format == "csv" {
        output.csv
    } else {
        output.xlsx
    };
    (
        [
            (header::CONTENT_TYPE, download.content_type),
            (
                header::CONTENT_DISPOSITION,
                if query.format == "csv" {
                    "attachment; filename=poam-register.csv"
                } else {
                    "attachment; filename=poam-register.xlsx"
                },
            ),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        download.bytes,
    )
        .into_response()
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
    match poam::list_register(&pool, &actor, &query, &SystemClock).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(e),
    }
}

/// Returns the bounded safe assignee catalog to a POA&M mutator.
///
/// Returns a structured error response when authentication, actor lookup,
/// Operator/Admin authorization, or catalog loading fails.
pub async fn assignee_catalog(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
) -> Response {
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match poam::assignee_catalog(&pool, &actor).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
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

/// Selects exact-CVE relationships for one visible system.
#[derive(Deserialize)]
pub struct CveRelationshipsQuery {
    /// Identifies the system whose current exact occurrences are requested.
    pub system_id: Uuid,
    /// Limits historical POA&Ms independently for each occurrence.
    pub history_limit: Option<i64>,
    /// Skips this many historical POA&Ms for each occurrence.
    pub history_offset: Option<i64>,
}

/// Selects the canonical package identity for a fleet CVE drawer.
#[derive(Deserialize)]
pub struct FleetCveDetailQuery {
    /// Gives the canonical package pname that completes exact subject identity.
    pub package: String,
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

/// Returns server-issued exact-CVE occurrence and remediation relationships.
///
/// Returns a structured error response when authentication, visibility,
/// evidence resolution, pagination, or persistence loading fails.
pub async fn cve_relationships(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<CveRelationshipsQuery>, QueryRejection>,
) -> Response {
    let query = match query_body(query, "invalid_query", "Malformed CVE relationship query") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match poam::cve_relationships(
        &pool,
        &actor,
        query.system_id,
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

/// Returns the visible exact and legacy fleet inventory for one CVE and package.
///
/// Returns a structured error response for malformed inputs, hidden scope,
/// unavailable inventory, or persistence failures. Legacy rows are read-only;
/// fleet mutations resolve exact evidence again in the mutation transaction.
pub async fn fleet_cve_detail(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<FleetCveDetailQuery>, QueryRejection>,
) -> Response {
    let cve_id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let query = match query_body(query, "invalid_query", "Malformed fleet CVE detail query") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    match poam::fleet_cve_inventory_detail(&pool, &actor, &cve_id, &query.package).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}

/// Applies one atomic environment-scoped fleet CVE triage request.
///
/// Returns a structured error response for CSRF, authorization, exact-evidence,
/// lifecycle, assignee, or bounded ownership conflicts.
pub async fn triage_fleet_cve(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<crate::api::models::FleetCveTriageRequest>, JsonRejection>,
) -> Response {
    let cve_id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(response) = csrf(&headers) {
        return response;
    }
    let body = match json_body(body, "Malformed fleet CVE triage request") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    match poam::triage_fleet_cve(&pool, &actor, &cve_id, body, &SystemClock).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}

/// Returns host and environment triage state for one System Detail CVE row.
///
/// The service derives the selected hostname, current environment, exact
/// environment subjects, both direct dispositions, and host-precedence
/// effective state. A hidden system or a row without current exact evidence
/// returns not found.
pub async fn system_cve_triage_detail(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<(Uuid, String)>, PathRejection>,
    query: Result<Query<FleetCveDetailQuery>, QueryRejection>,
) -> Response {
    let (system_id, cve_id) = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let query = match query_body(query, "invalid_query", "Malformed system CVE triage query") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    match poam::system_cve_triage_detail(&pool, &actor, system_id, &cve_id, &query.package).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}

/// Applies one host- or environment-scoped triage action from System Detail.
///
/// The body selects only the safe scope enum. It cannot supply environment,
/// system, or host-list identities. The handler enforces CSRF before the shared
/// service validates authorization and exact evidence.
pub async fn triage_system_cve(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<(Uuid, String)>, PathRejection>,
    body: Result<Json<crate::api::models::SystemCveTriageRequest>, JsonRejection>,
) -> Response {
    let (system_id, cve_id) = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(response) = csrf(&headers) {
        return response;
    }
    let body = match json_body(body, "Malformed system CVE triage request") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    match poam::triage_system_cve(&pool, &actor, system_id, &cve_id, body, &SystemClock).await {
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

/// Creates a POA&M from a server-issued current exact-CVE occurrence.
///
/// Returns a structured error response when CSRF, authentication, body,
/// authorization, current evidence, or atomic uniqueness validation fails.
pub async fn create_cve(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    body: Result<Json<CreateCvePoamRequest>, JsonRejection>,
) -> Response {
    if let Err(error) = csrf(&headers) {
        return error;
    }
    let body = match json_body(body, "Malformed exact-CVE POA&M request") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match poam::create_cve(&pool, &actor, body, &SystemClock).await {
        Ok(value) => (StatusCode::CREATED, Json(value)).into_response(),
        Err(error) => error_response(error),
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

/// Links a current exact-CVE occurrence to one exact-CVE POA&M.
///
/// Returns a structured error response when CSRF, authentication, revision,
/// family compatibility, evidence, or persistence validation fails.
pub async fn link_cve_finding(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    body: Result<Json<AddCveFindingRequest>, JsonRejection>,
) -> Response {
    let id = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(error) = csrf(&headers) {
        return error;
    }
    let body = match json_body(body, "Malformed exact-CVE link request") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match poam::link_cve_finding(&pool, &actor, id, body, &SystemClock).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
    }
}

/// Retires one active exact-CVE finding link.
///
/// Returns a structured error response when CSRF, authentication, revision,
/// minimum-finding, visibility, or persistence validation fails.
pub async fn unlink_cve_finding(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    query: Result<Query<RevisionRequest>, QueryRejection>,
) -> Response {
    let (id, finding_id) = match path_body(path) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(error) = csrf(&headers) {
        return error;
    }
    let revision = match query_body(query, "invalid_revision", "Malformed POA&M revision") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let actor = match actor(&pool, user, &headers).await {
        Ok(value) => value,
        Err(error) => return error,
    };
    match poam::unlink_cve_finding(
        &pool,
        &actor,
        id,
        finding_id,
        revision.revision,
        &SystemClock,
    )
    .await
    {
        Ok(value) => Json(value).into_response(),
        Err(error) => error_response(error),
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
            .route("/api/v1/poams/export", get(export))
            .route(
                "/api/v1/register/export",
                get(crate::handlers::api::register_export::export),
            )
            .route(
                "/api/v1/acceptances/export",
                get(crate::handlers::api::acceptance_register::export),
            )
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

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified disposable PG35457"]
    async fn mixed_register_download_serves_four_real_formats(pool: PgPool) {
        let base = server(pool.clone()).await;
        let client = reqwest::Client::new();
        let url = format!(
            "{base}/api/v1/register/export?poam_status=completed&acceptance_status=accepted_or_converted&format=csv"
        );
        assert_eq!(client.get(&url).send().await.unwrap().status(), 401);
        let token = session(&pool, AuthRole::Admin).await;
        let unknown_filter = client
            .get(format!("{url}&unrecognized_scope=all"))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(unknown_filter.status(), 400);
        let empty_oscal = client
            .get(format!(
                "{base}/api/v1/register/export?poam_status=completed&acceptance_status=pending&format=oscal-json"
            ))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(empty_oscal.status(), 204);
        assert!(empty_oscal.bytes().await.unwrap().is_empty());
        let user_id: Uuid =
            sqlx::query_scalar("SELECT user_id FROM user_sessions WHERE session_token_hash=$1")
                .bind(hash_token(&token))
                .fetch_one(&pool)
                .await
                .unwrap();
        let environment: Uuid =
            sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
                .bind(format!("mixed-{}", Uuid::new_v4()))
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("INSERT INTO cves(id) VALUES('CVE-2099-54321')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,review_date,accepted_by,accepted_at) VALUES('CVE-2099-54321','sample',$1,'accepted','Reviewed source risk','2099-01-10',$2,now())")
            .bind(environment).bind(user_id).execute(&pool).await.unwrap();
        for (format, media_type, marker) in [
            ("csv", "text/csv; charset=utf-8", "CVE-2099-54321"),
            (
                "xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "PK",
            ),
            (
                "oscal-json",
                "application/json",
                "plan-of-action-and-milestones",
            ),
            (
                "oscal-xml",
                "application/xml",
                "plan-of-action-and-milestones",
            ),
        ] {
            let response = client.get(format!("{base}/api/v1/register/export?poam_status=completed&acceptance_status=accepted_or_converted&format={format}"))
                .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
                .send().await.unwrap();
            assert_eq!(response.status(), 200, "{format}");
            assert_eq!(response.headers()["content-type"], media_type);
            assert_eq!(response.headers()["cache-control"], "private, no-store");
            let bytes = response.bytes().await.unwrap();
            if format == "xlsx" {
                assert!(bytes.starts_with(marker.as_bytes()));
            } else {
                assert!(String::from_utf8(bytes.to_vec()).unwrap().contains(marker));
            }
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified disposable PG35457"]
    async fn poam_download_checks_auth_and_format(pool: PgPool) {
        let base = server(pool.clone()).await;
        let client = reqwest::Client::new();
        let url = format!("{base}/api/v1/poams/export?format=csv&status=completed");
        assert_eq!(client.get(&url).send().await.unwrap().status(), 401);
        let token = session(&pool, AuthRole::Admin).await;
        let invalid = client
            .get(format!("{base}/api/v1/poams/export?format=xml"))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(invalid.status(), 400);
        let csv = client
            .get(&url)
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(csv.status(), 200);
        assert_eq!(csv.headers()["content-type"], "text/csv; charset=utf-8");
        assert_eq!(csv.headers()["cache-control"], "private, no-store");
        let csv_bytes = csv.bytes().await.unwrap();
        assert!(csv_bytes.starts_with(b"\"Source type\","));
        let xlsx = client
            .get(format!(
                "{base}/api/v1/poams/export?format=xlsx&status=completed"
            ))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(xlsx.status(), 200);
        assert!(xlsx.bytes().await.unwrap().starts_with(b"PK"));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified disposable PG35457"]
    async fn acceptance_download_checks_session_filters_and_file_headers(pool: PgPool) {
        let base = server(pool.clone()).await;
        let client = reqwest::Client::new();
        let url = format!("{base}/api/v1/acceptances/export?format=csv&source=cve_environment");
        assert_eq!(client.get(&url).send().await.unwrap().status(), 401);
        let token = session(&pool, AuthRole::Admin).await;
        let user_id: Uuid =
            sqlx::query_scalar("SELECT user_id FROM user_sessions WHERE session_token_hash=$1")
                .bind(hash_token(&token))
                .fetch_one(&pool)
                .await
                .unwrap();
        let environment: Uuid =
            sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
                .bind(format!("export-http-{}", Uuid::new_v4()))
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("INSERT INTO cves(id) VALUES('CVE-2099-12345')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,review_date,accepted_by,accepted_at) VALUES('CVE-2099-12345','sample',$1,'accepted','=1+1','2099-01-10',$2,now())")
            .bind(environment).bind(user_id).execute(&pool).await.unwrap();
        let response = client
            .get(&url)
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers()["content-type"],
            "text/csv; charset=utf-8"
        );
        assert_eq!(
            response.headers()["content-disposition"],
            "attachment; filename=acceptance-register.csv"
        );
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        let bytes = response.bytes().await.unwrap();
        let mut reader = csv::Reader::from_reader(bytes.as_ref());
        let records = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(&records[0][0], "CVE decision");
        assert_eq!(&records[0][8], "CVE-2099-12345");
        assert_eq!(&records[0][10], "2099-01-10");
        assert_eq!(&records[0][15], "'=1+1");
        assert!(records[0][13].is_empty());
        let xlsx = client
            .get(format!(
                "{base}/api/v1/acceptances/export?format=xlsx&source=cve_environment"
            ))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(xlsx.status(), 200);
        assert_eq!(
            xlsx.headers()["content-disposition"],
            "attachment; filename=acceptance-register.xlsx"
        );
        assert_eq!(
            xlsx.headers()["content-type"],
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        assert!(xlsx.bytes().await.unwrap().starts_with(b"PK"));
        let invalid = client
            .get(format!("{base}/api/v1/acceptances/export?format=xml"))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(invalid.status(), 400);
    }
}
