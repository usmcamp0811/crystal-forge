//! HTTP handlers for compliance frameworks, framework versions,
//! requirement versions, policy-requirement mappings, and bundle requirement
//! coverage.
//!
//! # Route summary
//!
//! ```text
//! GET  /api/v1/compliance/frameworks
//! GET  /api/v1/compliance/frameworks/:id/versions
//! GET  /api/v1/compliance/framework-versions/:fv_id/requirements
//! GET  /api/v1/compliance/requirement-versions/:rv_id/children
//! GET  /api/v1/compliance/bundle-versions/:bv_id/requirement-coverage
//!
//! GET    /api/v1/policy-versions/:pv_id/requirement-mappings
//! POST   /api/v1/policy-versions/:pv_id/requirement-mappings
//! PUT    /api/v1/policy-versions/:pv_id/requirement-mappings/:m_id
//! DELETE /api/v1/policy-versions/:pv_id/requirement-mappings/:m_id
//! ```
//!
//! Authorization:
//! - Read endpoints: any authenticated user.
//! - Write endpoints (mapping CRUD): admin or operator role.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::ApiError;
use crate::handlers::api::rbac::{authenticated_user_roles, has_admin_role};
use crate::queries::framework_requirements::{
    BundleCoverageReport, FrameworkSummary, FrameworkVersionSummary, PolicyMappingRow,
    RequirementVersionSummary, compute_bundle_requirement_coverage, create_policy_mapping,
    delete_policy_mapping, list_framework_versions, list_frameworks, list_policy_mappings,
    list_requirement_children, search_requirements, update_policy_mapping,
};

// ── Local helpers ─────────────────────────────────────────────────────────────

fn forbidden() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "Forbidden".to_string(),
            message: "Authentication or authorization required".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn bad_request(message: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "Bad Request".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn not_found(message: &str) -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Not Found".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn internal_error(message: &str) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "Internal Server Error".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

// ── Query string parameters ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RequirementSearchQuery {
    /// Full-text search query (external_id, title, CCI IDs, SRG IDs).
    pub q: Option<String>,
    /// Filter by node kind, e.g. `"rule"`, `"control"`, `"family"`.
    pub kind: Option<String>,
    /// Maximum rows per page (capped at 50).
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Zero-based page offset.
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    25
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateMappingRequest {
    pub requirement_version_id: Uuid,
    pub relationship: String,
    pub coverage: String,
    pub rationale: Option<String>,
    #[serde(default = "default_provenance")]
    pub provenance: String,
}

fn default_provenance() -> String {
    "manual".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UpdateMappingRequest {
    pub relationship: String,
    pub coverage: String,
    pub rationale: Option<String>,
}

// ── Framework endpoints ───────────────────────────────────────────────────────

/// `GET /api/v1/compliance/frameworks`
///
/// List all compliance framework lineages with version counts.
/// Requires authentication; no admin role required.
pub async fn list_compliance_frameworks(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if authenticated_user_roles(&pool, &headers).await.is_none() {
        return forbidden();
    }
    match list_frameworks(&pool).await {
        Ok(frameworks) => (StatusCode::OK, Json(frameworks)).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to list compliance frameworks");
            internal_error("Failed to list compliance frameworks")
        }
    }
}

/// `GET /api/v1/compliance/frameworks/:id/versions`
///
/// List all versions of a specific framework.
pub async fn list_compliance_framework_versions(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(framework_id): Path<Uuid>,
) -> impl IntoResponse {
    if authenticated_user_roles(&pool, &headers).await.is_none() {
        return forbidden();
    }
    match list_framework_versions(&pool, framework_id).await {
        Ok(versions) => (StatusCode::OK, Json(versions)).into_response(),
        Err(e) => {
            tracing::error!(
                error = %e, framework_id = %framework_id,
                "failed to list framework versions"
            );
            internal_error("Failed to list framework versions")
        }
    }
}

// ── Requirement endpoints ─────────────────────────────────────────────────────

/// `GET /api/v1/compliance/framework-versions/:fv_id/requirements`
///
/// Server-side paginated requirement search within a framework version.
/// Supports `?q=<text>&kind=<kind>&limit=<n>&offset=<n>`.
pub async fn search_framework_requirements(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(framework_version_id): Path<Uuid>,
    Query(params): Query<RequirementSearchQuery>,
) -> impl IntoResponse {
    if authenticated_user_roles(&pool, &headers).await.is_none() {
        return forbidden();
    }
    match search_requirements(
        &pool,
        framework_version_id,
        params.q.as_deref(),
        params.kind.as_deref(),
        params.limit,
        params.offset,
    )
    .await
    {
        Ok(requirements) => (StatusCode::OK, Json(requirements)).into_response(),
        Err(e) => {
            tracing::error!(
                error = %e, framework_version_id = %framework_version_id,
                "failed to search requirements"
            );
            internal_error("Failed to search requirements")
        }
    }
}

/// `GET /api/v1/compliance/requirement-versions/:rv_id/children`
///
/// List direct children of a requirement version in the hierarchy.
pub async fn list_requirement_version_children(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(parent_id): Path<Uuid>,
) -> impl IntoResponse {
    if authenticated_user_roles(&pool, &headers).await.is_none() {
        return forbidden();
    }
    match list_requirement_children(&pool, parent_id).await {
        Ok(children) => (StatusCode::OK, Json(children)).into_response(),
        Err(e) => {
            tracing::error!(error = %e, parent_id = %parent_id, "failed to list requirement children");
            internal_error("Failed to list requirement children")
        }
    }
}

// ── Bundle coverage ───────────────────────────────────────────────────────────

/// `GET /api/v1/compliance/bundle-versions/:bv_id/requirement-coverage`
///
/// Compute and return authoritative requirement coverage for a bundle version.
/// Coverage is derived from normalized mappings × bundle requirement membership
/// × selected bundle policy versions.
pub async fn get_bundle_requirement_coverage(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(bundle_version_id): Path<Uuid>,
) -> impl IntoResponse {
    if authenticated_user_roles(&pool, &headers).await.is_none() {
        return forbidden();
    }
    match compute_bundle_requirement_coverage(&pool, bundle_version_id).await {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(e) => {
            tracing::error!(
                error = %e, bundle_version_id = %bundle_version_id,
                "failed to compute bundle requirement coverage"
            );
            internal_error("Failed to compute bundle requirement coverage")
        }
    }
}

// ── Policy-requirement mapping CRUD ──────────────────────────────────────────

/// `GET /api/v1/policy-versions/:pv_id/requirement-mappings`
///
/// List all requirement mappings for a policy version.
pub async fn list_policy_requirement_mappings(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(policy_version_id): Path<Uuid>,
) -> impl IntoResponse {
    if authenticated_user_roles(&pool, &headers).await.is_none() {
        return forbidden();
    }
    match list_policy_mappings(&pool, policy_version_id).await {
        Ok(mappings) => (StatusCode::OK, Json(mappings)).into_response(),
        Err(e) => {
            tracing::error!(
                error = %e, policy_version_id = %policy_version_id,
                "failed to list policy requirement mappings"
            );
            internal_error("Failed to list policy requirement mappings")
        }
    }
}

/// `POST /api/v1/policy-versions/:pv_id/requirement-mappings`
///
/// Create a new requirement mapping on a mutable (draft) policy version.
/// Requires admin or operator role.
pub async fn create_policy_requirement_mapping(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(policy_version_id): Path<Uuid>,
    Json(request): Json<CreateMappingRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    if !has_admin_role(&roles) {
        return forbidden();
    }

    // Validate field values.
    if !matches!(
        request.relationship.as_str(),
        "implements" | "supports" | "provides_evidence_for"
    ) {
        return bad_request(
            "Invalid relationship; allowed values: implements, supports, provides_evidence_for",
        );
    }
    if !matches!(request.coverage.as_str(), "full" | "partial") {
        return bad_request("Invalid coverage; allowed values: full, partial");
    }

    match create_policy_mapping(
        &pool,
        policy_version_id,
        request.requirement_version_id,
        &request.relationship,
        &request.coverage,
        request.rationale.as_deref(),
        &request.provenance,
        user_id,
    )
    .await
    {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("POLICY_MAPPING_IMMUTABLE") || msg.contains("POLICY_VERSION_NOT_FOUND")
            {
                bad_request(&msg)
            } else if msg.contains("duplicate key") || msg.contains("unique constraint") {
                bad_request("A mapping for this policy version and requirement already exists")
            } else {
                tracing::error!(error = %e, "failed to create policy requirement mapping");
                internal_error("Failed to create policy requirement mapping")
            }
        }
    }
}

/// `PUT /api/v1/policy-versions/:pv_id/requirement-mappings/:m_id`
///
/// Update relationship/coverage/rationale on an existing mapping.
/// Requires admin or operator role.
pub async fn update_policy_requirement_mapping(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((_policy_version_id, mapping_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateMappingRequest>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    if !has_admin_role(&roles) {
        return forbidden();
    }

    if !matches!(
        request.relationship.as_str(),
        "implements" | "supports" | "provides_evidence_for"
    ) {
        return bad_request(
            "Invalid relationship; allowed values: implements, supports, provides_evidence_for",
        );
    }
    if !matches!(request.coverage.as_str(), "full" | "partial") {
        return bad_request("Invalid coverage; allowed values: full, partial");
    }

    match update_policy_mapping(
        &pool,
        mapping_id,
        &request.relationship,
        &request.coverage,
        request.rationale.as_deref(),
    )
    .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "id": mapping_id })),
        )
            .into_response(),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("POLICY_MAPPING_IMMUTABLE_OR_NOT_FOUND") {
                not_found(&msg)
            } else {
                tracing::error!(error = %e, mapping_id = %mapping_id, "failed to update policy requirement mapping");
                internal_error("Failed to update policy requirement mapping")
            }
        }
    }
}

/// `DELETE /api/v1/policy-versions/:pv_id/requirement-mappings/:m_id`
///
/// Delete a requirement mapping from a mutable policy version.
/// Requires admin or operator role.
pub async fn delete_policy_requirement_mapping(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((_policy_version_id, mapping_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    if !has_admin_role(&roles) {
        return forbidden();
    }

    match delete_policy_mapping(&pool, mapping_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("POLICY_MAPPING_IMMUTABLE_OR_NOT_FOUND") {
                not_found(&msg)
            } else {
                tracing::error!(error = %e, mapping_id = %mapping_id, "failed to delete policy requirement mapping");
                internal_error("Failed to delete policy requirement mapping")
            }
        }
    }
}
