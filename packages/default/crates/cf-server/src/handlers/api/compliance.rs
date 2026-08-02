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
    let mut accumulated = Vec::new();
    let mut filename: Option<String> = None;
    let mut total_bytes: usize = 0;
    let mut received_file = false;

    // Read multipart incrementally.
    //
    // * Only the field named exactly "file" is accepted as the upload field.
    //   Any field with a filename but a different name is rejected (400).
    // * Exactly one upload file is accepted; a second file field is rejected (400).
    // * Non-file fields are drained to satisfy the route-level body limit
    //   without contributing to the accumulated bytes.
    while let Ok(Some(mut field)) = multipart.next_field().await {
        let field_name = field.name().map(String::from);
        let has_filename = field.file_name().is_some();

        if has_filename {
            // Enforce field name == "file".
            if field_name.as_deref() != Some("file") {
                return bad_request(
                    "Upload field must be named 'file'; unexpected field name in multipart",
                );
            }
            // Reject a second file field.
            if received_file {
                return bad_request("Exactly one file field named 'file' is required");
            }
            received_file = true;
            filename = field.file_name().map(String::from);
            // Check media type from filename for 415.
            if let Some(ref fname) = filename {
                let lower = fname.to_lowercase();
                if !lower.ends_with(".xml") && !lower.ends_with(".zip") {
                    return (
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                        Json(ApiError {
                            error: "Unsupported file type".into(),
                            message: "Only .xml and .zip files are accepted".into(),
                            details: None,
                        }),
                    )
                        .into_response();
                }
            }
        }

        // Read incrementally.
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    if !has_filename {
                        // Drain non-file fields without accumulating.
                        continue;
                    }
                    total_bytes += chunk.len();
                    if total_bytes > limits.max_xml_bytes {
                        return (
                            StatusCode::PAYLOAD_TOO_LARGE,
                            Json(ApiError {
                                error: "File too large".into(),
                                message: format!(
                                    "Upload exceeds {} byte limit",
                                    limits.max_xml_bytes
                                ),
                                details: None,
                            }),
                        )
                            .into_response();
                    }
                    accumulated.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => {
                    if is_body_limit_error(&e) {
                        return (
                            StatusCode::PAYLOAD_TOO_LARGE,
                            Json(ApiError {
                                error: "Request too large".into(),
                                message: "Multipart request exceeds the route body limit".into(),
                                details: None,
                            }),
                        )
                            .into_response();
                    }
                    return internal_error(&format!("Failed to read upload: {e}"));
                }
            }
        }
    }

    let bytes = accumulated;
    if bytes.is_empty() {
        return bad_request("No file field named 'file' was attached");
    };

    match parse_xccdf(&bytes, filename.as_deref(), &limits) {
        Ok(parsed) => {
            // 422 for blocking validation errors.
            if parsed.errors.iter().any(|e| e.blocking) {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({
                        "error": "XCCDF validation failed",
                        "sha256": parsed.source_sha256,
                        "errors": parsed.errors.iter().map(|e| serde_json::json!({
                            "code": e.code, "summary": e.summary, "blocking": e.blocking,
                        })).collect::<Vec<_>>(),
                        "warnings": parsed.warnings.iter().map(|w| serde_json::json!({
                            "code": w.code, "summary": w.summary,
                        })).collect::<Vec<_>>(),
                    })),
                )
                    .into_response();
            }
            // 200 for successful parse with non-blocking warnings only.
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
    let version_row: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
    )> = match sqlx::query_as(
        r#"
            SELECT name, framework, framework_version, description, layer, owner
            FROM compliance_bundle_versions WHERE id = $1
            "#,
    )
    .bind(version_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(row) => row,
        Err(error) => {
            tracing::error!(%error, %version_id, "failed to load bundle version for export");
            return internal_error("Failed to load bundle version");
        }
    };

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
                    (
                        "content-disposition",
                        &format!("attachment; filename=\"{}\"", safe_filename),
                    ),
                ],
                xml,
            )
                .into_response()
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
    let safe = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
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

/// Detect whether a multipart chunk-read failure is the route-level
/// `DefaultBodyLimit` rejecting an over-limit request body.
///
/// Axum wraps the request body in `http_body::Limited`; when the limit is
/// exceeded mid-stream the error surfaces here instead of at extraction time
/// (the handler authenticates before reading fields). Walk the error source
/// chain for the limit message rather than exposing it as a 500.
fn is_body_limit_error(err: &axum::extract::multipart::MultipartError) -> bool {
    let mut current: Option<&(dyn std::error::Error + 'static)> =
        Some(err as &(dyn std::error::Error + 'static));
    while let Some(e) = current {
        if e.to_string().contains("length limit exceeded") {
            return true;
        }
        current = e.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::session::{SESSION_COOKIE_NAME, hash_token};
    use crate::compliance::interchange::{MAX_XCCDF_MULTIPART_BYTES, MAX_XCCDF_XML_BYTES};
    use crate::models::auth_identity::AuthRole;
    use crate::queries::auth_identity::{create_user_session, sync_user_role};
    use crate::queries::users::insert_user;
    use axum::Router;
    use axum::extract::DefaultBodyLimit;
    use axum::routing::post;
    use chrono::Utc;

    const BOUNDARY: &str = "XCFTESTBOUNDARY";

    fn minimal_xccdf() -> &'static str {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2"
    id="xccdf_org.crystalforge_benchmark_test">
  <status>draft</status>
  <title>Test Benchmark</title>
  <version>0.1.0</version>
  <Rule id="xccdf_org.crystalforge_rule_test">
    <title>Test Rule</title>
  </Rule>
</Benchmark>"#
    }

    async fn test_pool_from_env() -> PgPool {
        let db_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for XCCDF preview endpoint tests");
        PgPool::connect(&db_url)
            .await
            .expect("failed to connect to DATABASE_URL")
    }

    /// Create an admin user with a live session and return the session token.
    async fn admin_session_token(pool: &PgPool) -> String {
        let suffix = Uuid::new_v4().simple().to_string();
        let user = insert_user(
            pool,
            &format!("{suffix}@example.com"),
            Some("XCCDF Preview Tester"),
        )
        .await
        .expect("insert_user should succeed");
        sync_user_role(pool, user.id, AuthRole::Admin)
            .await
            .expect("sync_user_role should succeed");
        let token = format!("session-{suffix}");
        create_user_session(
            pool,
            user.id,
            hash_token(&token),
            Utc::now() + chrono::Duration::hours(1),
            Some("test-agent".to_string()),
            Some("127.0.0.1".to_string()),
        )
        .await
        .expect("create_user_session should succeed");
        token
    }

    /// Serve the preview route exactly as production wires it, including the
    /// route-level body limit layer, and return the base URL.
    async fn spawn_preview_server(pool: PgPool) -> String {
        let app = Router::new()
            .route(
                "/api/v1/compliance/xccdf/preview",
                post(xccdf_preview).layer(DefaultBodyLimit::max(MAX_XCCDF_MULTIPART_BYTES)),
            )
            .with_state(pool);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve preview app");
        });
        format!("http://{addr}")
    }

    fn push_file_field(body: &mut Vec<u8>, name: &str, filename: &str, content: &[u8]) {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(content);
        body.extend_from_slice(b"\r\n");
    }

    fn push_text_field(body: &mut Vec<u8>, name: &str, content: &[u8]) {
        body.extend_from_slice(
            format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(content);
        body.extend_from_slice(b"\r\n");
    }

    fn finish_multipart(body: &mut Vec<u8>) {
        body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    }

    async fn post_multipart(base: &str, token: &str, body: Vec<u8>) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{base}/api/v1/compliance/xccdf/preview"))
            .header(
                "content-type",
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .body(body)
            .send()
            .await
            .expect("preview request completes")
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_body_above_route_limit() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        // File field stays under the file limit; a junk field pushes the total
        // request body past the route-level limit.
        let mut body = Vec::new();
        push_file_field(&mut body, "file", "test.xml", minimal_xccdf().as_bytes());
        push_text_field(&mut body, "junk", &vec![b'x'; MAX_XCCDF_MULTIPART_BYTES]);
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 413);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_file_above_file_limit() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let mut body = Vec::new();
        push_file_field(
            &mut body,
            "file",
            "big.xml",
            &vec![b'x'; MAX_XCCDF_XML_BYTES + 1],
        );
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 413);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_accepts_valid_file_below_limits() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let mut body = Vec::new();
        push_file_field(&mut body, "file", "test.xml", minimal_xccdf().as_bytes());
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 200);
        let json: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(json["document_class"], "foreignxccdf");
        assert_eq!(json["rule_count"], 1);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_unsupported_file_type() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let mut body = Vec::new();
        push_file_field(&mut body, "file", "test.txt", minimal_xccdf().as_bytes());
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 415);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_blocking_validation_errors_with_422() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let dtd = r#"<?xml version="1.0"?>
<!DOCTYPE Benchmark [<!ENTITY x "y">]>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2" id="xccdf_test_benchmark">
  <status>draft</status>
  <title>Test</title>
  <version>0.1</version>
</Benchmark>"#;
        let mut body = Vec::new();
        push_file_field(&mut body, "file", "test.xml", dtd.as_bytes());
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 422);
        let json: serde_json::Value = response.json().await.expect("json body");
        let codes: Vec<&str> = json["errors"]
            .as_array()
            .expect("errors array")
            .iter()
            .filter_map(|e| e["code"].as_str())
            .collect();
        assert!(codes.contains(&"DTD_FORBIDDEN"));
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_missing_file_with_400() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let mut body = Vec::new();
        push_text_field(&mut body, "note", b"no file attached");
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 400);
    }

    // ── Multipart field-count tests ───────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_two_file_fields_with_400() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let mut body = Vec::new();
        push_file_field(&mut body, "file", "a.xml", minimal_xccdf().as_bytes());
        push_file_field(&mut body, "file", "b.xml", minimal_xccdf().as_bytes());
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 400);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_accepts_non_file_field_plus_one_file_field() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        // A leading non-file (text) field must be silently drained; only the
        // "file" upload field is processed.
        let mut body = Vec::new();
        push_text_field(&mut body, "note", b"metadata only");
        push_file_field(&mut body, "file", "test.xml", minimal_xccdf().as_bytes());
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 200);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_file_field_with_wrong_name() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        // File field named "upload" instead of "file" must be rejected.
        let mut body = Vec::new();
        push_file_field(&mut body, "upload", "test.xml", minimal_xccdf().as_bytes());
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 400);
    }
}
