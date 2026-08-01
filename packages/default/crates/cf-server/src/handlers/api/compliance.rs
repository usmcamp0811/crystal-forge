//! Compliance API handlers.
//!
//! These endpoints expose compliance bundles and rollups derived from existing
//! Crystal Forge systems, environments, deployment policies, and CVE posture.

use axum::{
    Json,
    extract::{Multipart, Path, State},
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
use crate::compliance::digest::{
    BundleMembershipEntry, BundleVersionCanonical, load_bundle_membership,
};
use crate::compliance::interchange::InterchangeLimits;
use crate::compliance::xccdf::parser::parse_xccdf;
use crate::compliance::xccdf::xml_writer::write_bundle_xccdf;
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
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    if !has_admin_role(&roles) {
        return forbidden();
    }

    match update_bundle_row(&pool, bundle_id, payload, Some(user_id)).await {
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
/// Accepts multipart XML upload, parses XCCDF 1.2 and CF-XCCDF content,
/// classifies the document, and returns typed metadata without durable writes.
pub async fn xccdf_preview(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    if !has_admin_role(&roles) {
        return forbidden();
    }

    let limits = InterchangeLimits::default();
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        filename = field.file_name().map(String::from);
        match field.bytes().await {
            Ok(bytes) => {
                if bytes.len() > limits.max_xml_bytes {
                    return (
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(ApiError {
                            error: "File too large".into(),
                            message: format!(
                                "XML file exceeds {} byte limit",
                                limits.max_xml_bytes
                            ),
                            details: None,
                        }),
                    )
                        .into_response();
                }
                file_bytes = Some(bytes.to_vec());
            }
            Err(e) => {
                return internal_error(&format!("Failed to read upload: {e}"));
            }
        }
    }

    let bytes = match file_bytes {
        Some(b) => b,
        None => return bad_request("No file attached"),
    };

    match parse_xccdf(&bytes, filename.as_deref(), &limits) {
        Ok(parsed) => {
            // Format rules for the response.
            let rule_summaries: Vec<serde_json::Value> = parsed
                .rules
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.id,
                        "title": r.title,
                        "severity": r.severity,
                        "is_native": r.cf_policy_meta.is_some(),
                    })
                })
                .collect();

            let response = serde_json::json!({
                "sha256": parsed.source_sha256,
                "filename": parsed.source_filename,
                "xccdf_version": parsed.xccdf_version,
                "document_class": format!("{:?}", parsed.class).to_lowercase(),
                "fidelity": format!("{:?}", parsed.fidelity).to_lowercase(),
                "fidelity_losses": parsed.fidelity_losses,
                "benchmark": parsed.benchmark.map(|bm| serde_json::json!({
                    "id": bm.id,
                    "title": bm.title,
                    "version": bm.version,
                    "status": bm.status,
                    "platforms": bm.platforms,
                })),
                "profiles": parsed.profiles.iter().map(|p| serde_json::json!({
                    "id": p.id,
                    "title": p.title,
                    "rule_count": p.select_ids.len(),
                })).collect::<Vec<_>>(),
                "rules": rule_summaries,
                "rule_count": parsed.rules.len(),
                "profile_count": parsed.profiles.len(),
                "cf_bundle_meta": parsed.cf_bundle_meta.map(|m| serde_json::json!({
                    "bundle_id": m.bundle_id,
                    "bundle_version_id": m.bundle_version_id,
                    "publication_state": m.publication_state,
                })),
                "errors": parsed.errors.iter().map(|e| serde_json::json!({
                    "code": e.code, "summary": e.summary, "blocking": e.blocking,
                })).collect::<Vec<_>>(),
                "warnings": parsed.warnings.iter().map(|w| serde_json::json!({
                    "code": w.code, "summary": w.summary,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            tracing::error!("XCCDF parse error: {e:#}");
            internal_error(&format!("Failed to parse XCCDF: {e}"))
        }
    }
}

/// `POST /api/v1/compliance/xccdf/import`
///
/// Accepts the same file plus an import plan and commits atomically.
/// Full implementation arrives in phase 4.
pub async fn xccdf_import(State(pool): State<PgPool>, headers: HeaderMap) -> impl IntoResponse {
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
/// Exports the bundle version as an XCCDF 1.2 XML document.
pub async fn export_bundle_xccdf(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(version_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    // Load the bundle version to build the canonical representation.
    let version_row: Option<(String, String, Option<String>, Option<String>, String, String)> =
        sqlx::query_as(
            r#"
            SELECT name, framework, framework_version, description, layer, owner
            FROM compliance_bundle_versions WHERE id = $1
            "#,
        )
        .bind(version_id)
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();

    let Some((name, framework, fw_ver, desc, layer, owner)) = version_row else {
        return not_found();
    };

    // Load membership.
    let members = match load_bundle_membership_txless(&pool, version_id).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to load bundle membership for export: {e:#}");
            return internal_error("Failed to load bundle membership");
        }
    };

    let canonical = BundleVersionCanonical {
        name,
        framework,
        framework_version: fw_ver,
        description: desc,
        layer,
        owner,
        members,
    };

    match write_bundle_xccdf(&canonical) {
        Ok(xml) => {
            let safe_filename = safe_bundle_xml_filename(&canonical.name);
            (
                StatusCode::OK,
                [
                    ("content-type", "application/xml"),
                    ("content-disposition", &format!("attachment; filename=\"{}\"", safe_filename)),
                ],
                xml,
            ).into_response()
        }
        Err(e) => {
            tracing::error!("XCCDF export write error: {e:#}");
            internal_error("Failed to generate XCCDF export")
        }
    }
}

/// Load bundle membership without a transaction (for read-only export).
async fn load_bundle_membership_txless(
    pool: &PgPool,
    version_id: Uuid,
) -> Result<Vec<BundleMembershipEntry>, anyhow::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        policy_version_id: Uuid,
        selected: bool,
    }
    let rows: Vec<Row> = sqlx::query_as(
        r#"
        SELECT policy_version_id, selected
        FROM compliance_bundle_version_policies
        WHERE bundle_version_id = $1
        ORDER BY policy_order ASC
        "#,
    )
    .bind(version_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| BundleMembershipEntry {
            policy_version_id: r.policy_version_id,
            selected: r.selected,
        })
        .collect())
}

fn safe_bundle_xml_filename(name: &str) -> String {
    let safe = name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>();
    format!("{}.xml", if safe.is_empty() { "bundle" } else { &safe })
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
