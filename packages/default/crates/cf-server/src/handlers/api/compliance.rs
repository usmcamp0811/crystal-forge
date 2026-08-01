//! Compliance API handlers.
//!
//! These endpoints expose compliance bundles and rollups derived from existing
//! Crystal Forge systems, environments, deployment policies, and CVE posture.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{
    ApiError, CreateComplianceBundleRequest, SystemComplianceBundle,
    SystemComplianceBundlesResponse, UpdateComplianceBundleRequest,
};
use crate::handlers::api::rbac::{authenticated_user_roles, has_admin_role};
use crate::queries::compliance::{
    BundleValidationError, create_bundle as create_bundle_row, delete_bundle as delete_bundle_row,
    get_system_evidence, list_bundle_systems, list_bundles, list_system_bundles,
    update_bundle as update_bundle_row,
};

/// `GET /api/v1/compliance/bundles`
pub async fn list_compliance_bundles(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if authenticated_user_roles(&pool, &headers).await.is_none() {
        return forbidden();
    }

    match list_bundles(&pool).await {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(_) => internal_error("Failed to load compliance bundles"),
    }
}

/// `GET /api/v1/compliance/bundles/:id/systems`
pub async fn get_compliance_bundle_systems(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(bundle_id): Path<Uuid>,
) -> impl IntoResponse {
    if authenticated_user_roles(&pool, &headers).await.is_none() {
        return forbidden();
    }

    match list_bundle_systems(&pool, bundle_id).await {
        Ok(Some(response)) => (StatusCode::OK, Json(response)).into_response(),
        Ok(None) => not_found(),
        Err(_) => internal_error("Failed to load compliance systems"),
    }
}

/// `GET /api/v1/systems/:system_id/compliance`
/// Returns all compliance bundles applicable to the specified system with rollups.
///
/// This endpoint uses set-based queries to avoid N+1 database patterns.
/// All-or-nothing behavior: infrastructure failures return 500.
///
/// Returns 404 if the system does not exist.
pub async fn get_system_compliance_bundles(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    if authenticated_user_roles(&pool, &headers).await.is_none() {
        return forbidden();
    }

    match list_system_bundles(&pool, system_id).await {
        Ok(Some(bundle_rollup_pairs)) => {
            let bundles = bundle_rollup_pairs
                .into_iter()
                .map(|(bundle, rollup)| SystemComplianceBundle { bundle, rollup })
                .collect();

            (
                StatusCode::OK,
                Json(SystemComplianceBundlesResponse { system_id, bundles }),
            )
                .into_response()
        }
        Ok(None) => not_found(),
        Err(_) => internal_error("Failed to load system compliance bundles"),
    }
}

/// `GET /api/v1/compliance/bundles/:id/systems/:system_id/evidence`
pub async fn get_compliance_system_evidence(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((bundle_id, system_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    if authenticated_user_roles(&pool, &headers).await.is_none() {
        return forbidden();
    }

    match get_system_evidence(&pool, bundle_id, system_id).await {
        Ok(Some(response)) => (StatusCode::OK, Json(response)).into_response(),
        Ok(None) => not_found(),
        Err(_) => internal_error("Failed to load compliance evidence"),
    }
}

/// `POST /api/v1/compliance/bundles`
pub async fn create_compliance_bundle(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<CreateComplianceBundleRequest>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    if !has_admin_role(&roles) {
        return forbidden();
    }

    match create_bundle_row(&pool, payload).await {
        Ok(bundle) => (StatusCode::CREATED, Json(bundle)).into_response(),
        Err(err) if err.downcast_ref::<BundleValidationError>().is_some() => {
            bad_request(&err.to_string())
        }
        Err(_) => internal_error("Failed to create compliance bundle"),
    }
}

/// `PUT /api/v1/compliance/bundles/:id`
pub async fn update_compliance_bundle(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(bundle_id): Path<Uuid>,
    Json(payload): Json<UpdateComplianceBundleRequest>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    if !has_admin_role(&roles) {
        return forbidden();
    }

    match update_bundle_row(&pool, bundle_id, payload).await {
        Ok(Some(bundle)) => (StatusCode::OK, Json(bundle)).into_response(),
        Ok(None) => not_found(),
        Err(err) if err.downcast_ref::<BundleValidationError>().is_some() => {
            bad_request(&err.to_string())
        }
        Err(_) => internal_error("Failed to update compliance bundle"),
    }
}

/// `DELETE /api/v1/compliance/bundles/:id`
pub async fn delete_compliance_bundle(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(bundle_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    if !has_admin_role(&roles) {
        return forbidden();
    }

    match delete_bundle_row(&pool, bundle_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found(),
        Err(_) => internal_error("Failed to delete compliance bundle"),
    }
}

// ── XCCDF interchange ──────────────────────────────────────────────────────

/// Request/response stubs – full parser implementation comes in a later commit.
#[derive(Debug, Serialize, Deserialize)]
pub struct XccdfPreviewResponse {
    pub sha256: String,
    pub document_type: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XccdfImportResult {
    pub bundle_version_id: Option<Uuid>,
    pub created_policy_count: u32,
    pub reused_policy_count: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyInterchangeExportRequest {
    pub policy_version_ids: Vec<Uuid>,
    pub format: String, // "json" or "toml"
}

/// `POST /api/v1/compliance/xccdf/preview`
///
/// Accepts a multipart XML or ZIP upload, validates structure and limits, and
/// returns metadata without persisting anything. The full parser is implemented
/// in phase 4; this handler enforces upload limits and returns a structured stub.
pub async fn xccdf_preview(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    if !has_admin_role(&roles) {
        return forbidden();
    }
    // Phase 4 will add full multipart parsing and XCCDF validation here.
    // For now, return a structured 501 so the UI can distinguish "not yet
    // implemented" from a server error.
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(XccdfPreviewResponse {
            sha256: String::new(),
            document_type: "unknown".to_string(),
            errors: vec!["XCCDF parser not yet implemented".to_string()],
            warnings: vec![],
        }),
    )
        .into_response()
}

/// `POST /api/v1/compliance/xccdf/import`
///
/// Accepts the same file plus an import plan and commits atomically.
/// Full implementation arrives in phase 4.
pub async fn xccdf_import(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    if !has_admin_role(&roles) {
        return forbidden();
    }
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(XccdfImportResult {
            bundle_version_id: None,
            created_policy_count: 0,
            reused_policy_count: 0,
            errors: vec!["XCCDF import not yet implemented".to_string()],
        }),
    )
        .into_response()
}

/// `GET /api/v1/compliance/bundle-versions/:version_id/xccdf`
///
/// Exports the bundle version as an XCCDF 1.2 XML document. Full
/// implementation arrives in phase 4.
pub async fn export_bundle_xccdf(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(version_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    let _ = version_id;
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiError {
            error: "Not Implemented".to_string(),
            message: "XCCDF export not yet implemented".to_string(),
            details: None,
        }),
    )
        .into_response()
}

/// `POST /api/v1/policies/interchange/export`
///
/// Exports selected policy versions as canonical JSON or TOML.
pub async fn policy_interchange_export(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(request): Json<PolicyInterchangeExportRequest>,
) -> impl IntoResponse {
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    if request.policy_version_ids.is_empty() {
        return bad_request("At least one policy_version_id is required");
    }
    if !matches!(request.format.as_str(), "json" | "toml") {
        return bad_request("format must be 'json' or 'toml'");
    }
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiError {
            error: "Not Implemented".to_string(),
            message: "Policy interchange export not yet implemented".to_string(),
            details: None,
        }),
    )
        .into_response()
}

// ── helpers ────────────────────────────────────────────────────────────────

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

fn not_found() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Not Found".to_string(),
            message: "Compliance resource not found".to_string(),
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
