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
    TrustPolicyVersionRequest, TrustBundleVersionRequest,
    PublishPolicyVersionRequest, PublishBundleVersionRequest,
    CreatePolicyDraftRequest, CreateBundleDraftRequest,
};
use crate::compliance::interchange::{InterchangeLimits, MAX_XCCDF_UPLOAD_BYTES};
use crate::compliance::xccdf::export_models::{
    GroupProjectionError, ImportedCheckError, ImportedFixError, XccdfBundleExport,
    XccdfGroupExport, XccdfPolicyExport, XccdfSourceMapping,
};
use crate::compliance::xccdf::import_models::XccdfImportPlan;
use crate::compliance::xccdf::importer::{
    build_policy_records, check_document_class, validate_cf_native_document, validate_import_plan,
    validate_sha256_match,
};
use crate::compliance::xccdf::package::{ProcessingError, process_xccdf_bytes};
use crate::compliance::xccdf::reconciliation::NativeReconcileFailure;
use crate::compliance::xccdf::xml_writer::{XccdfWriterError, write_bundle_xccdf_export};
use crate::handlers::api::rbac::{authenticated_user_roles, has_admin_role};
use crate::queries::compliance::{
    BundleValidationError, create_bundle as create_bundle_row, delete_bundle as delete_bundle_row,
    get_system_evidence, list_bundle_systems, list_bundles, list_system_bundles,
    update_bundle as update_bundle_row,
};
use crate::queries::compliance_interchange;

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

// ── Trust and Publication (Phase 1) ────────────────────────────────────────

/// `POST /api/v1/policy-versions/:version_id/trust`
/// Trust or reject a policy version. Only admin users can perform this operation.
pub async fn trust_policy_version(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(version_id): Path<Uuid>,
    Json(payload): Json<crate::api::models::TrustPolicyVersionRequest>,
) -> impl IntoResponse {
    use crate::api::models::{TrustPolicyVersionRequest, TrustPolicyVersionResponse};

    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    if !has_admin_role(&roles) {
        return forbidden();
    }

    let trust_state = if payload.trusted { "trusted" } else { "rejected" };

    // Update the version with trust state
    let result = sqlx::query_as::<_, (Uuid, String, String)>(
        r#"UPDATE deployment_policy_versions
           SET trust_state = $2, trusted_by = $3, trusted_at = CURRENT_TIMESTAMP,
               trust_review_note = $4
           WHERE id = $1
           RETURNING id, publication_state, trust_state"#,
    )
    .bind(version_id)
    .bind(trust_state)
    .bind(user_id)
    .bind(&payload.review_note)
    .fetch_optional(&pool)
    .await;

    match result {
        Ok(Some((version_id, publication_state, trust_state))) => {
            // Load the full version to get trusted_by and trusted_at
            let full_result = sqlx::query_as::<_, (Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>)>(
                r#"SELECT trusted_by, trusted_at FROM deployment_policy_versions WHERE id = $1"#,
            )
            .bind(version_id)
            .fetch_one(&pool)
            .await;

            match full_result {
                Ok((trusted_by, trusted_at)) => {
                    let response = TrustPolicyVersionResponse {
                        version_id,
                        publication_state,
                        trust_state,
                        trusted_by,
                        trusted_at,
                    };
                    (StatusCode::OK, Json(response)).into_response()
                }
                Err(_) => internal_error("Failed to load trust state after update"),
            }
        }
        Ok(None) => not_found(),
        Err(_) => internal_error("Failed to update policy version trust state"),
    }
}

/// `POST /api/v1/compliance/bundle-versions/:version_id/trust`
/// Trust or reject a bundle version. Only admin users can perform this operation.
pub async fn trust_bundle_version(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(version_id): Path<Uuid>,
    Json(payload): Json<crate::api::models::TrustBundleVersionRequest>,
) -> impl IntoResponse {
    use crate::api::models::{TrustBundleVersionRequest, TrustBundleVersionResponse};

    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    if !has_admin_role(&roles) {
        return forbidden();
    }

    let trust_state = if payload.trusted { "trusted" } else { "rejected" };

    // Update the version with trust state
    let result = sqlx::query_as::<_, (Uuid, String, String)>(
        r#"UPDATE compliance_bundle_versions
           SET trust_state = $2, trusted_by = $3, trusted_at = CURRENT_TIMESTAMP,
               trust_review_note = $4
           WHERE id = $1
           RETURNING id, publication_state, trust_state"#,
    )
    .bind(version_id)
    .bind(trust_state)
    .bind(user_id)
    .bind(&payload.review_note)
    .fetch_optional(&pool)
    .await;

    match result {
        Ok(Some((version_id, publication_state, trust_state))) => {
            // Load the full version to get trusted_by and trusted_at
            let full_result = sqlx::query_as::<_, (Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>)>(
                r#"SELECT trusted_by, trusted_at FROM compliance_bundle_versions WHERE id = $1"#,
            )
            .bind(version_id)
            .fetch_one(&pool)
            .await;

            match full_result {
                Ok((trusted_by, trusted_at)) => {
                    let response = TrustBundleVersionResponse {
                        version_id,
                        publication_state,
                        trust_state,
                        trusted_by,
                        trusted_at,
                    };
                    (StatusCode::OK, Json(response)).into_response()
                }
                Err(_) => internal_error("Failed to load trust state after update"),
            }
        }
        Ok(None) => not_found(),
        Err(_) => internal_error("Failed to update bundle version trust state"),
    }
}

/// `POST /api/v1/policy-versions/:version_id/publish`
/// Publish a policy version, making it immutable. Only admin users can publish.
pub async fn publish_policy_version(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(version_id): Path<Uuid>,
    Json(payload): Json<crate::api::models::PublishPolicyVersionRequest>,
) -> impl IntoResponse {
    use crate::api::models::PublishPolicyVersionResponse;

    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    if !has_admin_role(&roles) {
        return forbidden();
    }

    // Load the version to check current state and digest
    let version_result = sqlx::query_as::<_, (Uuid, String, String, String)>(
        r#"SELECT id, publication_state, semantic_digest, policy_type
           FROM deployment_policy_versions WHERE id = $1"#,
    )
    .bind(version_id)
    .fetch_optional(&pool)
    .await;

    let (_, publication_state, digest, _policy_type) = match version_result {
        Ok(Some(v)) => v,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load policy version"),
    };

    // Verify digest if provided
    if let Some(expected_digest) = &payload.expected_semantic_digest {
        if expected_digest != &digest {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": "Semantic digest mismatch",
                    "message": "The provided digest does not match the stored digest. The version may have been modified.",
                    "code": "DIGEST_MISMATCH"
                })),
            )
                .into_response();
        }
    }

    // Reject if already published
    if publication_state == "accepted" {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "Already published",
                "message": "This policy version is already published and immutable. Create a draft to edit.",
                "code": "ALREADY_PUBLISHED"
            })),
        )
            .into_response();
    }

    // Publish: set publication_state to 'accepted' and published_at
    let publish_result = sqlx::query_as::<_, (Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"UPDATE deployment_policy_versions
           SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP
           WHERE id = $1 AND publication_state != 'accepted'
           RETURNING id, publication_state, semantic_digest, published_at"#,
    )
    .bind(version_id)
    .fetch_optional(&pool)
    .await;

    match publish_result {
        Ok(Some((id, state, digest, published_at))) => {
            // Also update the current_published_version_id pointer on the policy lineage
            let _update_pointer = sqlx::query(
                r#"UPDATE deployment_policies
                   SET current_published_version_id = $1
                   WHERE id = (SELECT policy_id FROM deployment_policy_versions WHERE id = $2)"#,
            )
            .bind(id)
            .bind(id)
            .execute(&pool)
            .await;

            let response = PublishPolicyVersionResponse {
                version_id: id,
                publication_state: state,
                published_at,
                semantic_digest: digest,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(None) => not_found(),
        Err(_) => internal_error("Failed to publish policy version"),
    }
}

/// `POST /api/v1/policies/:policy_id/drafts`
/// Create a new draft policy version from the published version.
pub async fn create_policy_draft(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(policy_id): Path<Uuid>,
    Json(_payload): Json<crate::api::models::CreatePolicyDraftRequest>,
) -> impl IntoResponse {
    use crate::api::models::CreatePolicyDraftResponse;

    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    if !has_admin_role(&roles) {
        return forbidden();
    }

    // Load the current published version
    let published_result = sqlx::query_as::<_, (Uuid, String, String, String, String, serde_json::Value, serde_json::Value, serde_json::Value, String, String)>(
        r#"SELECT id, version, name, description, policy_type, config, compliance_metadata, dependencies, semantic_digest, implementation_state
           FROM deployment_policy_versions
           WHERE policy_id = $1 AND publication_state = 'accepted'
           ORDER BY published_at DESC LIMIT 1"#,
    )
    .bind(policy_id)
    .fetch_optional(&pool)
    .await;

    let (
        published_id,
        _published_version,
        name,
        description,
        policy_type,
        config,
        compliance_metadata,
        dependencies,
        _digest,
        implementation_state,
    ) = match published_result {
        Ok(Some(v)) => v,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "No published version",
                    "message": "Policy has no published version to derive from",
                    "code": "NO_PUBLISHED_VERSION"
                })),
            )
                .into_response()
        }
        Err(_) => return internal_error("Failed to load published policy version"),
    };

    // Create a new draft version derived from the published version
    let new_version_id = uuid::Uuid::new_v4();
    let new_version_string = "draft-1"; // Simplified; real impl would increment properly

    let create_result = sqlx::query_as::<_, (Uuid, String)>(
        r#"INSERT INTO deployment_policy_versions
           (id, policy_id, version, name, description, policy_type, config,
            compliance_metadata, dependencies, semantic_digest, publication_state,
            derived_from_version_id, implementation_state)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'draft', $11, $12)
           RETURNING id, version"#,
    )
    .bind(new_version_id)
    .bind(policy_id)
    .bind(new_version_string)
    .bind(&name)
    .bind(&description)
    .bind(&policy_type)
    .bind(&config)
    .bind(&compliance_metadata)
    .bind(&dependencies)
    .bind(_digest) // Recompute in real impl
    .bind(published_id)
    .bind(&implementation_state)
    .fetch_optional(&pool)
    .await;

    match create_result {
        Ok(Some((new_id, new_version))) => {
            // Update the current_draft_version_id pointer
            let _update_pointer = sqlx::query(
                r#"UPDATE deployment_policies SET current_draft_version_id = $1 WHERE id = $2"#,
            )
            .bind(new_id)
            .bind(policy_id)
            .execute(&pool)
            .await;

            let response = CreatePolicyDraftResponse {
                version_id: new_id,
                version: new_version,
                publication_state: "draft".to_string(),
                derived_from_version_id: published_id,
            };
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Ok(None) => internal_error("Failed to create draft (no row returned)"),
        Err(_) => internal_error("Failed to create policy draft"),
    }
}

/// `POST /api/v1/compliance/bundle-versions/:version_id/publish`
/// Publish a bundle version, making it immutable. Atomically publishes included draft policies if requested.
pub async fn publish_bundle_version(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(version_id): Path<Uuid>,
    Json(payload): Json<crate::api::models::PublishBundleVersionRequest>,
) -> impl IntoResponse {
    use crate::api::models::PublishBundleVersionResponse;

    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    if !has_admin_role(&roles) {
        return forbidden();
    }

    // Load the bundle version
    let bundle_result = sqlx::query_as::<_, (Uuid, Uuid, String, String, String)>(
        r#"SELECT id, bundle_id, publication_state, semantic_digest, version
           FROM compliance_bundle_versions WHERE id = $1"#,
    )
    .bind(version_id)
    .fetch_optional(&pool)
    .await;

    let (_, bundle_id, publication_state, digest, _version) = match bundle_result {
        Ok(Some(v)) => v,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load bundle version"),
    };

    // Verify digest if provided
    if let Some(expected_digest) = &payload.expected_semantic_digest {
        if expected_digest != &digest {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": "Semantic digest mismatch",
                    "message": "The provided digest does not match the stored digest. The version may have been modified.",
                    "code": "DIGEST_MISMATCH"
                })),
            )
                .into_response();
        }
    }

    // Reject if already published
    if publication_state == "accepted" {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "Already published",
                "message": "This bundle version is already published and immutable. Create a draft to edit.",
                "code": "ALREADY_PUBLISHED"
            })),
        )
            .into_response();
    }

    // Start a transaction to ensure atomicity
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return internal_error("Failed to start transaction"),
    };

    // Verify all included policies are published or can be auto-published
    let policies_result = sqlx::query_as::<_, (Uuid, String)>(
        r#"SELECT pv.id, pv.publication_state
           FROM compliance_bundle_version_policies cbvp
           JOIN deployment_policy_versions pv ON cbvp.policy_version_id = pv.id
           WHERE cbvp.bundle_version_id = $1"#,
    )
    .fetch_all(&mut *tx)
    .await;

    let policies = match policies_result {
        Ok(p) => p,
        Err(_) => {
            let _ = tx.rollback().await;
            return internal_error("Failed to load bundle policies");
        }
    };

    let mut auto_published_count = 0;

    // Check and optionally auto-publish draft policies
    for (policy_id, state) in &policies {
        if state == "draft" && payload.auto_publish_draft_policies.unwrap_or(false) {
            let auto_result = sqlx::query(
                r#"UPDATE deployment_policy_versions
                   SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP
                   WHERE id = $1"#,
            )
            .bind(policy_id)
            .execute(&mut *tx)
            .await;

            if auto_result.is_ok() {
                auto_published_count += 1;
            }
        } else if state != "accepted" && state != "draft" {
            let _ = tx.rollback().await;
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": "Invalid policy state",
                    "message": "Bundle contains policies that are not draft or published",
                    "code": "INVALID_POLICY_STATE"
                })),
            )
                .into_response();
        }
    }

    // Publish the bundle
    let publish_result = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
        r#"UPDATE compliance_bundle_versions
           SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP
           WHERE id = $1
           RETURNING id, publication_state, published_at"#,
    )
    .bind(version_id)
    .fetch_optional(&mut *tx)
    .await;

    match publish_result {
        Ok(Some((id, state, published_at))) => {
            // Update the current_published_version_id pointer
            let _update_result = sqlx::query(
                r#"UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2"#,
            )
            .bind(id)
            .bind(bundle_id)
            .execute(&mut *tx)
            .await;

            // Commit the transaction
            if let Err(_) = tx.commit().await {
                return internal_error("Failed to commit bundle publication");
            }

            let response = PublishBundleVersionResponse {
                version_id: id,
                publication_state: state,
                published_at,
                semantic_digest: digest,
                published_policy_count: policies.len() as i32,
                auto_published_policy_count: auto_published_count,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(None) => {
            let _ = tx.rollback().await;
            not_found()
        }
        Err(_) => {
            let _ = tx.rollback().await;
            internal_error("Failed to publish bundle version")
        }
    }
}

/// `POST /api/v1/compliance/bundles/:bundle_id/drafts`
/// Create a new draft bundle version from the published version.
pub async fn create_bundle_draft(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(bundle_id): Path<Uuid>,
    Json(_payload): Json<crate::api::models::CreateBundleDraftRequest>,
) -> impl IntoResponse {
    use crate::api::models::CreateBundleDraftResponse;

    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    if !has_admin_role(&roles) {
        return forbidden();
    }

    // Load the current published bundle version
    let published_result = sqlx::query_as::<_, (Uuid, String, String, String, String, String, String, String)>(
        r#"SELECT id, version, name, framework, framework_version, description, layer, owner
           FROM compliance_bundle_versions
           WHERE bundle_id = $1 AND publication_state = 'accepted'
           ORDER BY published_at DESC LIMIT 1"#,
    )
    .bind(bundle_id)
    .fetch_optional(&pool)
    .await;

    let (published_id, _published_version, name, framework, framework_version, description, layer, owner) =
        match published_result {
            Ok(Some(v)) => v,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "error": "No published version",
                        "message": "Bundle has no published version to derive from",
                        "code": "NO_PUBLISHED_VERSION"
                    })),
                )
                    .into_response()
            }
            Err(_) => return internal_error("Failed to load published bundle version"),
        };

    // Create a new draft bundle version
    let new_version_id = uuid::Uuid::new_v4();
    let new_version_string = "draft-1"; // Simplified

    let create_result = sqlx::query_as::<_, (Uuid, String)>(
        r#"INSERT INTO compliance_bundle_versions
           (id, bundle_id, version, name, framework, framework_version, description, layer, owner,
            publication_state, derived_from_version_id, semantic_digest)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'draft', $10, '')
           RETURNING id, version"#,
    )
    .bind(new_version_id)
    .bind(bundle_id)
    .bind(new_version_string)
    .bind(&name)
    .bind(&framework)
    .bind(&framework_version)
    .bind(&description)
    .bind(&layer)
    .bind(&owner)
    .bind(published_id)
    .fetch_optional(&pool)
    .await;

    match create_result {
        Ok(Some((new_id, new_version))) => {
            // Copy membership from published version to draft
            let _copy_membership = sqlx::query(
                r#"INSERT INTO compliance_bundle_version_policies
                   (bundle_version_id, policy_version_id, policy_order, selected)
                   SELECT $1, policy_version_id, policy_order, selected
                   FROM compliance_bundle_version_policies
                   WHERE bundle_version_id = $2"#,
            )
            .bind(new_id)
            .bind(published_id)
            .execute(&pool)
            .await;

            // Update the current_draft_version_id pointer
            let _update_pointer = sqlx::query(
                r#"UPDATE compliance_bundles SET current_draft_version_id = $1 WHERE id = $2"#,
            )
            .bind(new_id)
            .bind(bundle_id)
            .execute(&pool)
            .await;

            let response = CreateBundleDraftResponse {
                version_id: new_id,
                version: new_version,
                publication_state: "draft".to_string(),
                derived_from_version_id: published_id,
            };
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Ok(None) => internal_error("Failed to create draft (no row returned)"),
        Err(_) => internal_error("Failed to create bundle draft"),
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

    let upload = match read_multipart_upload(&mut multipart).await {
        Ok(upload) => upload,
        Err(error) => return multipart_read_error_response(error),
    };

    if upload.bytes.is_empty() {
        return bad_request("No file field named 'file' was attached");
    }

    let pkg = match process_xccdf_bytes(upload.bytes, upload.filename, &limits) {
        Ok(pkg) => pkg,
        Err(e) => return processing_error_response(e),
    };

    let p = &pkg.provenance;
    let original_sha256 = p.sha256.clone();
    let package_source_json = build_preview_source_json(p);
    let parsed = pkg.parsed;
    let xml_filename = p.selected_entry.clone();

    if parsed.errors.iter().any(|e| e.blocking) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "XCCDF validation failed",
                "sha256": original_sha256,
                "source": package_source_json,
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
        "sha256": original_sha256,
        "source": package_source_json,
        "filename": xml_filename,
        "xccdf_namespace_version": parsed.xccdf_namespace_version,
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

/// `POST /api/v1/compliance/xccdf/import`
///
/// Accepts one XML or ZIP file (field name "file") and one JSON import plan
/// (field name "plan") via multipart upload.  Reparses the package, validates
/// the plan, and commits all durable records in a single atomic transaction.
pub async fn xccdf_import(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    if !has_admin_role(&roles) {
        return forbidden();
    }

    let limits = InterchangeLimits::default();

    let (file_upload, plan_bytes) = match read_multipart_file_and_plan(&mut multipart).await {
        Ok(pair) => pair,
        Err(err) => return multipart_read_error_response(err),
    };

    if file_upload.bytes.is_empty() {
        return bad_request("No 'file' field was attached");
    }

    let plan: XccdfImportPlan = match serde_json::from_slice(&plan_bytes) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "IMPORT_PLAN_INVALID".into(),
                    message: format!("Import plan JSON is malformed: {e}"),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let pkg = match process_xccdf_bytes(file_upload.bytes, file_upload.filename, &limits) {
        Ok(pkg) => pkg,
        Err(e) => return processing_error_response(e),
    };

    if let Some(err) = validate_sha256_match(&plan.expected_sha256, &pkg.provenance.sha256) {
        let status = if err.code == "SOURCE_DIGEST_MISMATCH" {
            StatusCode::CONFLICT
        } else {
            StatusCode::UNPROCESSABLE_ENTITY
        };
        return (
            status,
            Json(ApiError {
                error: err.code.into(),
                message: err.message,
                details: None,
            }),
        )
            .into_response();
    }

    if let Some(err) = check_document_class(&pkg.parsed) {
        let status = if err.code == "CF_NATIVE_DIGEST_MISMATCH" {
            StatusCode::CONFLICT
        } else {
            StatusCode::UNPROCESSABLE_ENTITY
        };
        return (
            status,
            Json(ApiError {
                error: err.code.into(),
                message: err.message,
                details: None,
            }),
        )
            .into_response();
    }

    if matches!(
        pkg.parsed.class,
        crate::compliance::xccdf::models::DocumentClass::CfNativeExact
    ) {
        let (validated, policy_records) = match validate_cf_native_document(&pkg.parsed) {
            Ok(value) => value,
            Err(err) => {
                let status = if err.code == "CF_NATIVE_DIGEST_MISMATCH" {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                };
                return (
                    status,
                    Json(ApiError {
                        error: err.code.into(),
                        message: err.message,
                        details: None,
                    }),
                )
                    .into_response();
            }
        };
        let result = compliance_interchange::commit_cf_native_import(
            &pool,
            user_id,
            pkg,
            validated,
            policy_records,
        )
        .await;
        return match result {
            Ok(committed) => (StatusCode::CREATED, Json(committed)).into_response(),
            Err(error) => {
                if let Some(conflict) = error.downcast_ref::<NativeReconcileFailure>() {
                    let conflicts = conflict
                        .conflicts
                        .iter()
                        .map(|value| {
                            serde_json::json!({
                                "code": value.code(),
                                "conflict": format!("{value:?}"),
                            })
                        })
                        .collect::<Vec<_>>();
                    return (
                        StatusCode::CONFLICT,
                        Json(ApiError {
                            error: "CF_NATIVE_CONFLICT".into(),
                            message: conflict.to_string(),
                            details: Some(serde_json::json!({ "conflicts": conflicts })),
                        }),
                    )
                        .into_response();
                }
                if error.to_string().starts_with("CF_NATIVE_MAPPING_CONFLICT") {
                    return (
                        StatusCode::CONFLICT,
                        Json(ApiError {
                            error: "CF_NATIVE_MAPPING_CONFLICT".into(),
                            message: "source mapping targets a different local object".into(),
                            details: None,
                        }),
                    )
                        .into_response();
                }
                tracing::error!(error = %error, "CF-native XCCDF import commit failed");
                internal_error("Failed to commit XCCDF import")
            }
        };
    }

    let validated = match validate_import_plan(plan, &pkg.parsed) {
        Ok(v) => v,
        Err(err) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ApiError {
                    error: err.code.into(),
                    message: err.message,
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let policy_records = build_policy_records(&validated);

    let result = compliance_interchange::commit_foreign_import(
        &pool,
        user_id,
        pkg,
        validated,
        policy_records,
    )
    .await;

    match result {
        Ok(committed) => (StatusCode::CREATED, Json(committed)).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "XCCDF import commit failed");
            internal_error("Failed to commit XCCDF import")
        }
    }
}

/// `GET /api/v1/compliance/bundle-versions/:version_id/xccdf`
///
/// Exports the bundle version as a complete XCCDF 1.2 XML document with CF
/// extensions. Loads a consistent database snapshot in one read-only
/// transaction and delegates to the typed XML writer.
pub async fn export_bundle_xccdf(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(version_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let snapshot = match load_export_snapshot(&pool, version_id).await {
        Ok(s) => s,
        Err(error) => return export_snapshot_error_response(error, version_id),
    };

    match write_bundle_xccdf_export(&snapshot) {
        Ok(xml) => {
            let safe_filename = safe_bundle_xml_filename(&snapshot.name);
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
        Err(XccdfWriterError::MalformedImportedCheck {
            policy_version_id,
            reason,
        }) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "validation_error".into(),
                message: format!("Policy {policy_version_id} has invalid imported check: {reason}"),
                details: None,
            }),
        )
            .into_response(),
        Err(XccdfWriterError::MalformedImportedFix {
            policy_version_id,
            reason,
        }) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "validation_error".into(),
                message: format!("Policy {policy_version_id} has invalid imported fix: {reason}"),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("XCCDF export write error: {e:#}");
            internal_error("Failed to generate XCCDF export")
        }
    }
}

/// Errors from snapshot loading and validation.
enum ExportSnapshotError {
    NotFound,
    InvalidGroupProjection {
        source: GroupProjectionError,
    },
    InvalidImportedCheck {
        policy_version_id: Uuid,
        source: ImportedCheckError,
    },
    InvalidImportedFix {
        policy_version_id: Uuid,
        source: ImportedFixError,
    },
    Db(anyhow::Error),
}

impl From<anyhow::Error> for ExportSnapshotError {
    fn from(e: anyhow::Error) -> Self {
        Self::Db(e)
    }
}

fn export_snapshot_error_response(
    error: ExportSnapshotError,
    version_id: Uuid,
) -> axum::response::Response {
    match error {
        ExportSnapshotError::NotFound => not_found(),
        ExportSnapshotError::InvalidGroupProjection { source } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "validation_error".into(),
                message: format!("Invalid group structure: {source}"),
                details: None,
            }),
        )
            .into_response(),
        ExportSnapshotError::InvalidImportedCheck {
            policy_version_id,
            source,
        } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "validation_error".into(),
                message: format!("Policy {policy_version_id} has invalid imported check: {source}"),
                details: None,
            }),
        )
            .into_response(),
        ExportSnapshotError::InvalidImportedFix {
            policy_version_id,
            source,
        } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "validation_error".into(),
                message: format!("Policy {policy_version_id} has invalid imported fix: {source}"),
                details: None,
            }),
        )
            .into_response(),
        ExportSnapshotError::Db(error) => {
            tracing::error!(error = %error, %version_id, "failed to load export snapshot");
            internal_error("Failed to load bundle version for export")
        }
    }
}

/// Load a complete, consistent export snapshot from the database.
///
/// All reads execute inside a single `REPEATABLE READ READ ONLY` transaction so
/// the snapshot is a consistent point-in-time view. The bundle version,
/// membership, policy versions, and source-object mappings can never diverge
/// mid-export.
async fn load_export_snapshot(
    pool: &PgPool,
    version_id: Uuid,
) -> Result<XccdfBundleExport, ExportSnapshotError> {
    // Acquire a dedicated connection and pin the isolation level so every
    // subsequent query sees the same database state.
    let mut tx = pool.begin().await.map_err(|e| anyhow::anyhow!("{e:#}"))?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;

    // 1. Load the exact bundle version row.
    #[derive(sqlx::FromRow)]
    struct BundleVersionRow {
        id: Uuid,
        bundle_id: Uuid,
        version: String,
        publication_state: String,
        semantic_digest: String,
        digest_algorithm: String,
        canonicalization_version: String,
        name: String,
        description: Option<String>,
        framework: String,
        framework_version: Option<String>,
        layer: String,
        owner: String,
    }

    let bv: BundleVersionRow = sqlx::query_as(
        r#"
         SELECT id, bundle_id, version, publication_state, semantic_digest,
                digest_algorithm, canonicalization_version,
               name, description, framework, framework_version, layer, owner
        FROM compliance_bundle_versions
        WHERE id = $1
        "#,
    )
    .bind(version_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| anyhow::anyhow!("{e:#}"))?
    .ok_or(ExportSnapshotError::NotFound)?;

    let publication_state = parse_publication_state(&bv.publication_state)?;

    // 2. Load ordered membership with selection state.
    #[derive(sqlx::FromRow)]
    struct MembershipRow {
        policy_version_id: Uuid,
        policy_order: i32,
        selected: bool,
    }

    let membership: Vec<MembershipRow> = sqlx::query_as(
        r#"
        SELECT policy_version_id, policy_order, selected
        FROM compliance_bundle_version_policies
        WHERE bundle_version_id = $1
        ORDER BY policy_order ASC
        "#,
    )
    .bind(version_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| anyhow::anyhow!("{e:#}"))?;

    // 3. Load every policy version referenced by membership in one query.
    let policy_version_ids: Vec<Uuid> = membership.iter().map(|m| m.policy_version_id).collect();

    if policy_version_ids.is_empty() {
        // Empty membership: return a benchmark with no rules.
        return Ok(XccdfBundleExport {
            bundle_id: bv.bundle_id,
            bundle_version_id: bv.id,
            version: bv.version,
            publication_state,
            semantic_digest: bv.semantic_digest,
            digest_algorithm: bv.digest_algorithm.clone(),
            canonicalization_version: bv.canonicalization_version.clone(),
            name: bv.name,
            description: bv.description,
            framework: bv.framework,
            framework_version: bv.framework_version,
            layer: bv.layer,
            owner: bv.owner,
            groups: vec![],
            policies: vec![],
        });
    }

    #[derive(sqlx::FromRow)]
    struct PolicyVersionRow {
        id: Uuid,
        policy_id: Uuid,
        version: String,
        publication_state: String,
        semantic_digest: String,
        digest_algorithm: String,
        canonicalization_version: String,
        name: String,
        description: Option<String>,
        policy_type: String,
        implementation_state: String,
        execution_phase: String,
        config: serde_json::Value,
        compliance_metadata: serde_json::Value,
        dependencies: serde_json::Value,
        opaque_xml: Option<String>,
        enabled_by_default: bool,
    }

    let policy_rows: Vec<PolicyVersionRow> = sqlx::query_as(
        r#"
         SELECT id, policy_id, version, publication_state, semantic_digest,
                digest_algorithm, canonicalization_version,
               name, description, policy_type, implementation_state,
               execution_phase, config, compliance_metadata, dependencies,
               opaque_xml, enabled_by_default
        FROM deployment_policy_versions
        WHERE id = ANY($1)
        "#,
    )
    .bind(&policy_version_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| anyhow::anyhow!("{e:#}"))?;

    // Reject missing policy versions: every membership entry must resolve.
    if policy_rows.len() != policy_version_ids.len() {
        let found: std::collections::HashSet<Uuid> = policy_rows.iter().map(|r| r.id).collect();
        let missing: Vec<Uuid> = policy_version_ids
            .iter()
            .copied()
            .filter(|id| !found.contains(id))
            .collect();
        return Err(ExportSnapshotError::Db(anyhow::anyhow!(
            "Bundle version {} has {} membership entries pointing to missing policy version(s): {:?}",
            version_id,
            missing.len(),
            missing
        )));
    }

    // Build a map from policy_version_id → row.
    let mut policy_map: std::collections::HashMap<Uuid, PolicyVersionRow> =
        policy_rows.into_iter().map(|r| (r.id, r)).collect();

    // 4. Load source-object mappings for all policy versions in one query.
    #[derive(sqlx::FromRow)]
    struct SourceMappingRow {
        policy_version_id: Option<Uuid>,
        object_kind: String,
        source_identity: String,
        fidelity: String,
    }

    let mapping_rows: Vec<SourceMappingRow> = sqlx::query_as(
        r#"
        SELECT
            com.policy_version_id,
            com.object_kind,
            com.source_identity,
            com.fidelity
        FROM compliance_source_object_mappings com
        WHERE com.policy_version_id = ANY($1)
        ORDER BY com.object_kind, com.source_identity
        "#,
    )
    .bind(&policy_version_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| anyhow::anyhow!("{e:#}"))?;

    // Group mappings by policy_version_id.
    let mut mappings_by_policy: std::collections::HashMap<Uuid, Vec<XccdfSourceMapping>> =
        std::collections::HashMap::new();
    for m in mapping_rows {
        if let Some(pvid) = m.policy_version_id {
            mappings_by_policy
                .entry(pvid)
                .or_default()
                .push(XccdfSourceMapping {
                    object_kind: m.object_kind,
                    source_identity: m.source_identity,
                    fidelity: m.fidelity,
                });
        }
    }

    // 5. Assemble the export model, preserving membership order.
    let mut policies = Vec::with_capacity(membership.len());
    for member in &membership {
        let pv = policy_map
            .remove(&member.policy_version_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Internal inconsistency: policy version {} in membership but not in query results",
                    member.policy_version_id
                )
            })?;

        let impl_state = parse_implementation_state(&pv.implementation_state)?;
        let pub_state = parse_publication_state(&pv.publication_state)?;

        policies.push(XccdfPolicyExport {
            policy_id: pv.policy_id,
            policy_version_id: pv.id,
            version: pv.version,
            publication_state: pub_state,
            semantic_digest: pv.semantic_digest,
            digest_algorithm: pv.digest_algorithm,
            canonicalization_version: pv.canonicalization_version,
            name: pv.name,
            description: pv.description,
            policy_type: pv.policy_type,
            execution_phase: pv.execution_phase,
            implementation_state: impl_state,
            enabled_default: pv.enabled_by_default,
            selected: member.selected,
            policy_order: member.policy_order,
            config: pv.config,
            compliance_metadata: pv.compliance_metadata,
            dependencies: pv.dependencies,
            opaque_xml: pv.opaque_xml,
            source_mappings: mappings_by_policy
                .remove(&member.policy_version_id)
                .unwrap_or_default(),
        });
    }

    // All reads are complete. Commit the transaction to release the snapshot.
    tx.commit().await.map_err(|e| anyhow::anyhow!("{e:#}"))?;

    let groups = build_export_groups(&policies)
        .map_err(|source| ExportSnapshotError::InvalidGroupProjection { source })?;

    // Prevalidate imported standard checks and fixes. Fail-fast before the
    // writer starts emitting XML so that errors surface cleanly as HTTP 422
    // rather than mid-write failures.
    for pv in &policies {
        if let Some(standard_check) = pv.parse_standard_check().map_err(|source| {
            ExportSnapshotError::InvalidImportedCheck {
                policy_version_id: pv.policy_version_id,
                source,
            }
        })? {
            drop(standard_check);
        }
        if let Some(standard_fix) =
            pv.parse_standard_fix()
                .map_err(|source| ExportSnapshotError::InvalidImportedFix {
                    policy_version_id: pv.policy_version_id,
                    source,
                })?
        {
            drop(standard_fix);
        }
    }

    Ok(XccdfBundleExport {
        bundle_id: bv.bundle_id,
        bundle_version_id: bv.id,
        version: bv.version,
        publication_state,
        semantic_digest: bv.semantic_digest,
        digest_algorithm: bv.digest_algorithm,
        canonicalization_version: bv.canonicalization_version,
        name: bv.name,
        description: bv.description,
        framework: bv.framework,
        framework_version: bv.framework_version,
        layer: bv.layer,
        owner: bv.owner,
        groups,
        policies,
    })
}

/// Build a deterministic recursive group tree from imported group metadata.
/// Foreign source IDs are retained in `source_id`; generated IDs are always
/// NCName-safe XCCDF 1.2 IDs and therefore cannot inherit identifiers such as
/// `V-268078` from XCCDF 1.1/STIG content.
///
/// ## Validation rules
///
/// - Authored policies with no `group_id` are always roots with no children.
/// - Empty orphan groups (declared parent missing, no policies assigned) are
///   rejected with `GroupProjectionError::EmptyOrphan`.
/// - Full-DAG cycle detection traverses every node; cycles not originating
///   from any root are rejected with `GroupProjectionError::CycleNotFromRoot`.
/// - Every generated ID is NCName-safe: only ASCII alphanumeric and underscore.
fn build_export_groups(
    policies: &[XccdfPolicyExport],
) -> Result<Vec<XccdfGroupExport>, GroupProjectionError> {
    use std::collections::{BTreeMap, BTreeSet};

    struct GroupNode {
        source_id: Option<String>,
        parent_source_id: Option<String>,
        title: String,
        description: Option<String>,
        order: i32,
        policy_ids: Vec<Uuid>,
    }

    let mut nodes: BTreeMap<String, GroupNode> = BTreeMap::new();
    for policy in policies {
        let metadata = &policy.compliance_metadata;
        let source_id = metadata
            .get("group_id")
            .and_then(|value| value.as_str())
            .map(str::to_owned);
        let key = source_id
            .clone()
            .unwrap_or_else(|| format!("authored-type:{}", policy.policy_type));
        let node = nodes.entry(key.clone()).or_insert_with(|| GroupNode {
            source_id: source_id.clone(),
            parent_source_id: metadata
                .get("parent_group_id")
                .and_then(|value| value.as_str())
                .map(str::to_owned),
            title: metadata
                .get("group_title")
                .and_then(|value| value.as_str())
                .unwrap_or(&policy.policy_type)
                .to_owned(),
            description: metadata
                .get("group_description")
                .and_then(|value| value.as_str())
                .map(str::to_owned),
            order: metadata
                .get("group_order")
                .and_then(|value| value.as_i64())
                .unwrap_or(policy.policy_order as i64) as i32,
            policy_ids: Vec::new(),
        });
        node.policy_ids.push(policy.policy_version_id);
    }

    // ── Root key set ──────────────────────────────────────────────────────
    let root_keys: BTreeSet<String> = nodes
        .iter()
        .filter(|(_, n)| {
            n.parent_source_id
                .as_deref()
                .map(|pid| !nodes.contains_key(pid))
                .unwrap_or(true)
        })
        .map(|(k, _)| k.clone())
        .collect();

    // ── Empty orphan detection ────────────────────────────────────────────
    // Non-root imported nodes whose parent does not exist AND have no directly
    // assigned policies are rejected because they carry no useful information.
    for (key, node) in &nodes {
        if root_keys.contains(key) {
            continue;
        }
        if let Some(ref parent) = node.parent_source_id {
            if !nodes.contains_key(parent) && node.policy_ids.is_empty() {
                if let Some(ref sid) = node.source_id {
                    return Err(GroupProjectionError::EmptyOrphan {
                        group_source_id: sid.clone(),
                    });
                }
            }
        }
    }

    // ── Parent→children index for cycle detection ─────────────────────────
    let mut children_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, node) in &nodes {
        if let Some(ref parent) = node.parent_source_id {
            if let Some(parent_key) = nodes
                .iter()
                .find(|(_, n)| n.source_id.as_deref() == Some(parent.as_str()))
                .map(|(k, _)| k.clone())
            {
                children_of.entry(parent_key).or_default().push(key.clone());
            }
        }
    }

    // ── Full-DAG cycle detection (downward DFS from every root) ───────────
    let mut global_visited = BTreeSet::new();
    for root_key in &root_keys {
        let mut stack = vec![(root_key.clone(), BTreeSet::new())];
        while let Some((current, mut ancestors)) = stack.pop() {
            if ancestors.contains(&current) {
                return Err(GroupProjectionError::CycleNotFromRoot(current));
            }
            if global_visited.contains(&current) {
                continue;
            }
            global_visited.insert(current.clone());
            ancestors.insert(current.clone());
            if let Some(children) = children_of.get(&current) {
                for child in children {
                    stack.push((child.clone(), ancestors.clone()));
                }
            }
        }
    }

    // Any node not visited from a root is part of a closed cycle with no root
    // entry point. These are unrecoverable without persisted group records.
    if global_visited.len() != nodes.len() {
        let unvisited = nodes
            .keys()
            .find(|key| !global_visited.contains(*key))
            .expect("lengths differ")
            .clone();
        return Err(GroupProjectionError::CycleNotFromRoot(unvisited));
    }

    fn generated_id(source_id: Option<&str>, policy_ids: &[Uuid]) -> String {
        let source = source_id.unwrap_or("authored");
        let slug: String = source
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let suffix = policy_ids
            .first()
            .map(|id| id.simple().to_string())
            .unwrap_or_else(|| "empty".to_string());
        format!("xccdf_crystalforge_group_{slug}_{suffix}")
    }

    fn build_node(
        key: &str,
        nodes: &BTreeMap<String, GroupNode>,
        visiting: &mut BTreeSet<String>,
    ) -> XccdfGroupExport {
        let node = &nodes[key];

        // Authored groups (no source_id) are always leaf roots. They must not
        // become parents of every other ungrouped node via None == None matching.
        // Imported groups (with source_id) may have children.
        let children = if let Some(parent_source_id) = node.source_id.as_deref() {
            if visiting.contains(key) {
                // Cycle detected: return without children to break the loop.
                Vec::new()
            } else {
                visiting.insert(key.to_owned());
                let mut children: Vec<XccdfGroupExport> = nodes
                    .iter()
                    .filter(|(child_key, child)| {
                        // The child must point its parent at THIS node's source_id.
                        child_key.as_str() != key
                            && child
                                .parent_source_id
                                .as_deref()
                                .is_some_and(|p| p == parent_source_id)
                    })
                    .map(|(child_key, _)| build_node(child_key, nodes, visiting))
                    .collect();
                visiting.remove(key);
                children.sort_by_key(|c| c.order);
                children
            }
        } else {
            Vec::new()
        };

        XccdfGroupExport {
            generated_id: generated_id(node.source_id.as_deref(), &node.policy_ids),
            source_id: node.source_id.clone(),
            title: node.title.clone(),
            description: node.description.clone(),
            order: node.order,
            children,
            policies: node.policy_ids.clone(),
        }
    }

    let mut visiting = BTreeSet::new();
    let mut roots: Vec<XccdfGroupExport> = nodes
        .iter()
        .filter(|(_, node)| {
            node.parent_source_id
                .as_deref()
                .map(|pid| !nodes.contains_key(pid))
                .unwrap_or(true)
        })
        .map(|(key, _)| build_node(key, &nodes, &mut visiting))
        .collect();
    roots.sort_by_key(|g| g.order);
    Ok(roots)
}

fn parse_publication_state(
    s: &str,
) -> Result<crate::compliance::canonical::PublicationState, anyhow::Error> {
    use crate::compliance::canonical::PublicationState;
    match s {
        "incomplete" => Ok(PublicationState::Incomplete),
        "draft" => Ok(PublicationState::Draft),
        "interim" => Ok(PublicationState::Interim),
        "accepted" => Ok(PublicationState::Accepted),
        "deprecated" => Ok(PublicationState::Deprecated),
        other => anyhow::bail!("Unknown publication state: {other}"),
    }
}

fn parse_implementation_state(
    s: &str,
) -> Result<crate::compliance::canonical::ImplementationState, anyhow::Error> {
    use crate::compliance::canonical::ImplementationState;
    match s {
        "native" => Ok(ImplementationState::Native),
        "manual" => Ok(ImplementationState::Manual),
        "external" => Ok(ImplementationState::External),
        "unbound" => Ok(ImplementationState::Unbound),
        "opaque" => Ok(ImplementationState::Opaque),
        other => anyhow::bail!("Unknown implementation state: {other}"),
    }
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

/// Read a multipart body containing a "file" field and a "plan" JSON field.
///
/// Accepts the fields in any order. Rejects duplicate fields, unknown file-type
/// fields with the wrong name, and bodies that exceed the upload limit.
async fn read_multipart_file_and_plan(
    multipart: &mut Multipart,
) -> Result<(MultipartUpload, Vec<u8>), MultipartReadError> {
    let mut file: Option<MultipartUpload> = None;
    let mut plan: Option<Vec<u8>> = None;

    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) if is_body_limit_error(&e) => return Err(MultipartReadError::TooLarge),
            Err(_) => return Err(MultipartReadError::Malformed),
        };

        let field_name = field.name().map(String::from);
        let has_filename = field.file_name().is_some();

        if has_filename {
            if field_name.as_deref() != Some("file") {
                return Err(MultipartReadError::InvalidFieldName);
            }
            if file.is_some() {
                return Err(MultipartReadError::MultipleFiles);
            }
            let filename = field.file_name().map(String::from);
            let mut bytes = Vec::new();
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        if bytes.len() + chunk.len() > MAX_XCCDF_UPLOAD_BYTES {
                            return Err(MultipartReadError::TooLarge);
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) if is_body_limit_error(&e) => return Err(MultipartReadError::TooLarge),
                    Err(_) => return Err(MultipartReadError::Malformed),
                }
            }
            file = Some(MultipartUpload { bytes, filename });
        } else if field_name.as_deref() == Some("plan") {
            if plan.is_some() {
                return Err(MultipartReadError::MultipleFiles); // reuse variant for duplicate plan
            }
            let mut bytes = Vec::new();
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        // Plans are expected to be small; cap at 1 MiB.
                        if bytes.len() + chunk.len() > 1024 * 1024 {
                            return Err(MultipartReadError::TooLarge);
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) if is_body_limit_error(&e) => return Err(MultipartReadError::TooLarge),
                    Err(_) => return Err(MultipartReadError::Malformed),
                }
            }
            plan = Some(bytes);
        }
        // Unknown non-file fields are drained and ignored.
    }

    let file = file.ok_or_else(|| MultipartReadError::InvalidFieldName)?;
    let plan = plan.ok_or_else(|| MultipartReadError::InvalidFieldName)?;
    Ok((file, plan))
}

/// Convert a [`ProcessingError`] into an HTTP response.
fn processing_error_response(e: ProcessingError) -> axum::response::Response {
    match e {
        ProcessingError::UnknownContentType => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(ApiError {
                error: "Unsupported content".into(),
                message: "Uploaded bytes are neither an XML document nor a ZIP archive".into(),
                details: None,
            }),
        )
            .into_response(),
        ProcessingError::ContentExtensionMismatch { reason } => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(ApiError {
                error: "Content/extension mismatch".into(),
                message: reason.into(),
                details: None,
            }),
        )
            .into_response(),
        ProcessingError::TooLarge {
            subject,
            actual,
            maximum,
        } => (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ApiError {
                error: "File too large".into(),
                message: format!(
                    "{subject} upload ({actual} bytes) exceeds the {maximum} byte limit"
                ),
                details: None,
            }),
        )
            .into_response(),
        ProcessingError::ZipExtraction {
            code,
            message,
            http_status,
            candidates,
        } => {
            let status = if http_status == 413 {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            let mut body = serde_json::json!({
                "error": "ZIP extraction failed",
                "errors": [{ "code": code, "summary": message, "blocking": true }],
            });
            if !candidates.is_empty() {
                body["candidates"] = serde_json::json!(candidates);
            }
            (status, Json(body)).into_response()
        }
        ProcessingError::BlockingDiagnostics { parsed } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "XCCDF validation failed",
                "errors": parsed.errors.iter().map(|e| serde_json::json!({
                    "code": e.code, "summary": e.summary, "blocking": e.blocking,
                })).collect::<Vec<_>>(),
                "warnings": parsed.warnings.iter().map(|w| serde_json::json!({
                    "code": w.code, "summary": w.summary,
                })).collect::<Vec<_>>(),
            })),
        )
            .into_response(),
        ProcessingError::Internal(e) => {
            tracing::error!(error = %e, "XCCDF package processing failed");
            internal_error("Failed to process XCCDF package")
        }
    }
}

/// Build the `"source"` JSON object for the preview response.
fn build_preview_source_json(
    p: &crate::compliance::xccdf::package::PackageProvenance,
) -> serde_json::Value {
    use crate::compliance::xccdf::zip_extractor::PackageKind;
    match p.package_kind {
        PackageKind::Xml => serde_json::json!({
            "package_kind": "direct_xml",
            "original_filename": p.filename,
            "original_size": p.size_bytes,
            "original_sha256": p.sha256,
        }),
        PackageKind::Zip => serde_json::json!({
            "package_kind": "zip_package",
            "original_filename": p.filename,
            "original_size": p.size_bytes,
            "original_sha256": p.sha256,
            "selected_entry": p.selected_entry,
            "selected_xml_sha256": p.selected_xml_sha256,
            "archive_file_count": p.archive_file_count,
        }),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct MultipartUpload {
    bytes: Vec<u8>,
    filename: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum MultipartReadError {
    TooLarge,
    Malformed,
    InvalidFieldName,
    MultipleFiles,
}

async fn read_multipart_upload(
    multipart: &mut Multipart,
) -> Result<MultipartUpload, MultipartReadError> {
    let mut accumulated = Vec::new();
    let mut filename = None;
    let mut received_file = false;

    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) if is_body_limit_error(&error) => {
                return Err(MultipartReadError::TooLarge);
            }
            Err(_) => return Err(MultipartReadError::Malformed),
        };
        let field_name = field.name().map(String::from);
        let has_filename = field.file_name().is_some();

        if has_filename {
            if field_name.as_deref() != Some("file") {
                return Err(MultipartReadError::InvalidFieldName);
            }
            if received_file {
                return Err(MultipartReadError::MultipleFiles);
            }
            received_file = true;
            filename = field.file_name().map(String::from);
        }

        loop {
            match field.chunk().await {
                Ok(Some(chunk)) if has_filename => {
                    if accumulated.len() + chunk.len() > MAX_XCCDF_UPLOAD_BYTES {
                        return Err(MultipartReadError::TooLarge);
                    }
                    accumulated.extend_from_slice(&chunk);
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(error) if is_body_limit_error(&error) => {
                    return Err(MultipartReadError::TooLarge);
                }
                Err(_) => return Err(MultipartReadError::Malformed),
            }
        }
    }

    Ok(MultipartUpload {
        bytes: accumulated,
        filename,
    })
}

fn multipart_read_error_response(error: MultipartReadError) -> axum::response::Response {
    match error {
        MultipartReadError::TooLarge => (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ApiError {
                error: "Request too large".into(),
                message: "Multipart request exceeds the upload limit".into(),
                details: None,
            }),
        )
            .into_response(),
        MultipartReadError::Malformed => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "Malformed multipart request".into(),
                message: "Unable to decode multipart upload".into(),
                details: None,
            }),
        )
            .into_response(),
        MultipartReadError::InvalidFieldName => {
            bad_request("Upload field must be named 'file'; unexpected field name in multipart")
        }
        MultipartReadError::MultipleFiles => {
            bad_request("Exactly one file field named 'file' is required")
        }
    }
}

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
    use axum::body::Body;
    use axum::extract::DefaultBodyLimit;
    use axum::extract::FromRequest;
    use axum::http::Request;
    use axum::routing::{get, post};
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
    async fn multipart_reader_classifies_truncated_body_as_400() {
        let request = Request::builder()
            .header(
                "content-type",
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .body(Body::from(
                b"--XCFTESTBOUNDARY\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\ntruncated"
                    .to_vec(),
            ))
            .expect("request");
        let mut multipart = Multipart::from_request(request, &())
            .await
            .expect("multipart extractor accepts request stream");

        assert_eq!(
            read_multipart_upload(&mut multipart).await,
            Err(MultipartReadError::Malformed)
        );
        assert_eq!(
            multipart_read_error_response(MultipartReadError::Malformed).status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn export_invalid_imported_check_maps_to_422() {
        let response = export_snapshot_error_response(
            ExportSnapshotError::InvalidImportedCheck {
                policy_version_id: Uuid::nil(),
                source: ImportedCheckError::InvalidBoolean {
                    attribute: "negate".into(),
                    value: "yes".into(),
                },
            },
            Uuid::nil(),
        );
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn export_invalid_imported_fix_maps_to_422() {
        let response = export_snapshot_error_response(
            ExportSnapshotError::InvalidImportedFix {
                policy_version_id: Uuid::nil(),
                source: ImportedFixError::InvalidId,
            },
            Uuid::nil(),
        );
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn export_invalid_group_projection_maps_to_422() {
        let response = export_snapshot_error_response(
            ExportSnapshotError::InvalidGroupProjection {
                source: GroupProjectionError::EmptyOrphan {
                    group_source_id: "orphan".into(),
                },
            },
            Uuid::nil(),
        );
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn export_internal_error_maps_to_500() {
        let response = export_snapshot_error_response(
            ExportSnapshotError::Db(anyhow::anyhow!("database unavailable")),
            Uuid::nil(),
        );
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
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

        // Content must start with '<' so it is identified as XML before the
        // size check fires (opaque binary bytes would get 415 from content
        // detection before reaching the size limit).
        let mut big_xml = b"<".to_vec();
        big_xml.extend(vec![b'x'; MAX_XCCDF_XML_BYTES]); // total > XML limit

        let mut body = Vec::new();
        push_file_field(&mut body, "file", "big.xml", &big_xml);
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

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_malformed_multipart_with_400() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let response = post_multipart(
            &base,
            &token,
            b"--XCFTESTBOUNDARY\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\ntruncated"
                .to_vec(),
        )
        .await;
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

    // ── Phase 1: Trust and Publication tests ──────────────────────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn trust_policy_version_succeeds_for_admin() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;

        // Create a policy with a version
        let policy_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();

        sqlx::query(
            r#"INSERT INTO deployment_policies (id, name, policy_type, enabled, config)
               VALUES ($1, 'test-policy', 'custom_check', false, '{}')"#,
        )
        .bind(policy_id)
        .execute(&pool)
        .await
        .expect("insert policy");

        sqlx::query(
            r#"INSERT INTO deployment_policy_versions
               (id, policy_id, version, name, policy_type, config, semantic_digest, publication_state)
               VALUES ($1, $2, '1.0.0', 'test-policy', 'custom_check', '{}', 'test-digest', 'draft')"#,
        )
        .bind(version_id)
        .bind(policy_id)
        .execute(&pool)
        .await
        .expect("insert version");

        // Trust the version via the handler
        let base = spawn_preview_server(pool.clone()).await;
        let client = reqwest::Client::new();
        let request_body = serde_json::json!({
            "trusted": true,
            "review_note": "Reviewed and approved"
        });

        let response = client
            .post(format!("{base}/api/v1/policy-versions/{version_id}/trust"))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .json(&request_body)
            .send()
            .await
            .expect("trust request completes");

        assert_eq!(response.status().as_u16(), 200, "trust should succeed");

        // Verify the version is marked as trusted in the database
        let trusted_state: (String,) = sqlx::query_as(
            r#"SELECT trust_state FROM deployment_policy_versions WHERE id = $1"#,
        )
        .bind(version_id)
        .fetch_one(&pool)
        .await
        .expect("fetch trust_state");

        assert_eq!(trusted_state.0, "trusted");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn publish_policy_version_succeeds() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;

        // Create a policy with a draft version
        let policy_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let test_digest = "abc123";

        sqlx::query(
            r#"INSERT INTO deployment_policies (id, name, policy_type, enabled, config)
               VALUES ($1, 'test-policy', 'custom_check', false, '{}')"#,
        )
        .bind(policy_id)
        .execute(&pool)
        .await
        .expect("insert policy");

        sqlx::query(
            r#"INSERT INTO deployment_policy_versions
               (id, policy_id, version, name, policy_type, config, semantic_digest, publication_state)
               VALUES ($1, $2, '1.0.0', 'test-policy', 'custom_check', '{}', $3, 'draft')"#,
        )
        .bind(version_id)
        .bind(policy_id)
        .bind(test_digest)
        .execute(&pool)
        .await
        .expect("insert version");

        // Publish the version
        let base = spawn_preview_server(pool.clone()).await;
        let client = reqwest::Client::new();
        let request_body = serde_json::json!({
            "expected_semantic_digest": test_digest
        });

        let response = client
            .post(format!("{base}/api/v1/policy-versions/{version_id}/publish"))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .json(&request_body)
            .send()
            .await
            .expect("publish request completes");

        assert_eq!(response.status().as_u16(), 200, "publish should succeed");

        // Verify the version is now published
        let pub_state: (String, bool) = sqlx::query_as(
            r#"SELECT publication_state, published_at IS NOT NULL FROM deployment_policy_versions WHERE id = $1"#,
        )
        .bind(version_id)
        .fetch_one(&pool)
        .await
        .expect("fetch publication_state");

        assert_eq!(pub_state.0, "accepted");
        assert!(pub_state.1, "published_at should be set");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn publish_policy_version_rejects_digest_mismatch() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;

        let policy_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let test_digest = "abc123";

        sqlx::query(
            r#"INSERT INTO deployment_policies (id, name, policy_type, enabled, config)
               VALUES ($1, 'test-policy', 'custom_check', false, '{}')"#,
        )
        .bind(policy_id)
        .execute(&pool)
        .await
        .expect("insert policy");

        sqlx::query(
            r#"INSERT INTO deployment_policy_versions
               (id, policy_id, version, name, policy_type, config, semantic_digest, publication_state)
               VALUES ($1, $2, '1.0.0', 'test-policy', 'custom_check', '{}', $3, 'draft')"#,
        )
        .bind(version_id)
        .bind(policy_id)
        .bind(test_digest)
        .execute(&pool)
        .await
        .expect("insert version");

        let base = spawn_preview_server(pool).await;
        let client = reqwest::Client::new();
        let request_body = serde_json::json!({
            "expected_semantic_digest": "wrong_digest"
        });

        let response = client
            .post(format!("{base}/api/v1/policy-versions/{version_id}/publish"))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .json(&request_body)
            .send()
            .await
            .expect("publish request completes");

        // Should reject with 422 UNPROCESSABLE_ENTITY
        assert_eq!(response.status().as_u16(), 422, "digest mismatch should reject");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn publish_policy_already_published_returns_409() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;

        let policy_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let test_digest = "abc123";

        sqlx::query(
            r#"INSERT INTO deployment_policies (id, name, policy_type, enabled, config)
               VALUES ($1, 'test-policy', 'custom_check', false, '{}')"#,
        )
        .bind(policy_id)
        .execute(&pool)
        .await
        .expect("insert policy");

        sqlx::query(
            r#"INSERT INTO deployment_policy_versions
               (id, policy_id, version, name, policy_type, config, semantic_digest, publication_state, published_at)
               VALUES ($1, $2, '1.0.0', 'test-policy', 'custom_check', '{}', $3, 'accepted', CURRENT_TIMESTAMP)"#,
        )
        .bind(version_id)
        .bind(policy_id)
        .bind(test_digest)
        .execute(&pool)
        .await
        .expect("insert already published version");

        let base = spawn_preview_server(pool).await;
        let client = reqwest::Client::new();
        let request_body = serde_json::json!({
            "expected_semantic_digest": test_digest
        });

        let response = client
            .post(format!("{base}/api/v1/policy-versions/{version_id}/publish"))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .json(&request_body)
            .send()
            .await
            .expect("publish request completes");

        // Should reject with 409 CONFLICT
        assert_eq!(response.status().as_u16(), 409, "already published should return conflict");
    }

    // ── ZIP upload tests ──────────────────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_accepts_zip_containing_single_xml() {
        use std::io::Write;
        use zip::CompressionMethod;
        use zip::write::{FileOptions, SimpleFileOptions};

        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        // Build a ZIP containing exactly one XCCDF XML file.
        let mut zip_bytes = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            let opts: SimpleFileOptions =
                FileOptions::default().compression_method(CompressionMethod::Stored);
            w.start_file("benchmark.xml", opts).expect("start_file");
            w.write_all(minimal_xccdf().as_bytes()).expect("write xml");
            w.finish().expect("zip finish");
        }

        let mut body = Vec::new();
        push_file_field(&mut body, "file", "package.zip", &zip_bytes);
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 200);
        let json: serde_json::Value = response.json().await.expect("json body");
        // The XML was extracted and parsed; filename reflects the inner entry.
        assert_eq!(json["document_class"], "foreignxccdf");
        assert_eq!(json["rule_count"], 1);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_zip_with_no_xml() {
        use std::io::Write;
        use zip::CompressionMethod;
        use zip::write::{FileOptions, SimpleFileOptions};

        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let mut zip_bytes = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            let opts: SimpleFileOptions =
                FileOptions::default().compression_method(CompressionMethod::Stored);
            w.start_file("readme.txt", opts).expect("start_file");
            w.write_all(b"no xml here").expect("write txt");
            w.finish().expect("zip finish");
        }

        let mut body = Vec::new();
        push_file_field(&mut body, "file", "package.zip", &zip_bytes);
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
        assert!(codes.contains(&"ZIP_NO_XCCDF"), "got codes: {codes:?}");
    }

    // ── Content/extension mismatch and byte-detection tests ───────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_zip_bytes_named_xml() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let zip_bytes = {
            use std::io::Write;
            let mut buf = Vec::new();
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Stored);
            w.start_file("benchmark.xml", opts).unwrap();
            w.write_all(minimal_xccdf().as_bytes()).unwrap();
            w.finish().unwrap();
            buf
        };

        let mut body = Vec::new();
        // ZIP content but named .xml — content/extension mismatch.
        push_file_field(&mut body, "file", "package.xml", &zip_bytes);
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(
            response.status().as_u16(),
            415,
            "expected 415 for content/extension mismatch"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_xml_bytes_named_zip() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let mut body = Vec::new();
        // XML content but named .zip — content/extension mismatch.
        push_file_field(&mut body, "file", "package.zip", minimal_xccdf().as_bytes());
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(
            response.status().as_u16(),
            415,
            "expected 415 for content/extension mismatch"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_rejects_unknown_binary_bytes() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let mut body = Vec::new();
        push_file_field(&mut body, "file", "data.bin", b"\xFF\xFE\x00\x00garbage");
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(
            response.status().as_u16(),
            415,
            "expected 415 for unknown bytes"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_accepts_zip_bytes_with_no_extension() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let zip_bytes = {
            use std::io::Write;
            let mut buf = Vec::new();
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Stored);
            w.start_file("benchmark.xml", opts).unwrap();
            w.write_all(minimal_xccdf().as_bytes()).unwrap();
            w.finish().unwrap();
            buf
        };

        let mut body = Vec::new();
        // No extension — content detection should identify it as ZIP.
        push_file_field(&mut body, "file", "package", &zip_bytes);
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        // Files with no extension are accepted based on content signature.
        // ZIP bytes containing a valid XCCDF document should preview successfully.
        assert_eq!(
            response.status().as_u16(),
            200,
            "extensionless ZIP with valid XCCDF should be accepted"
        );
        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["document_class"], "foreignxccdf");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_accepts_xml_bytes_with_no_extension() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let mut body = Vec::new();
        push_file_field(&mut body, "file", "package", minimal_xccdf().as_bytes());
        finish_multipart(&mut body);

        // Files with no extension are accepted based on content signature.
        let response = post_multipart(&base, &token, body).await;
        assert_eq!(
            response.status().as_u16(),
            200,
            "extensionless XML with valid XCCDF should be accepted"
        );
        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["document_class"], "foreignxccdf");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_zip_response_includes_package_provenance() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let zip_bytes = {
            use std::io::Write;
            let mut buf = Vec::new();
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Stored);
            w.start_file("benchmark.xml", opts).unwrap();
            w.write_all(minimal_xccdf().as_bytes()).unwrap();
            w.finish().unwrap();
            buf
        };

        let mut body = Vec::new();
        push_file_field(&mut body, "file", "package.zip", &zip_bytes);
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(response.status().as_u16(), 200);
        let json: serde_json::Value = response.json().await.expect("json body");

        // sha256 must identify the original ZIP, not the extracted XML.
        assert!(json["sha256"].is_string());
        let source = &json["source"];
        assert_eq!(source["package_kind"], "zip_package");
        assert_eq!(source["original_filename"], "package.zip");
        assert!(source["original_sha256"].is_string());
        assert_eq!(source["selected_entry"], "benchmark.xml");
        assert!(source["selected_xml_sha256"].is_string());
        // The two digests must differ (ZIP ≠ inner XML).
        assert_ne!(json["sha256"], source["selected_xml_sha256"]);
    }

    // ── XCCDF 1.1 / real DISA STIG acceptance test ───────────────────────────

    /// XCCDF 1.1.4 fixture that mirrors the structure of the real
    /// U_Anduril_NixOS_V1R2_STIG.zip distributed by public.cyber.mil.
    ///
    /// The real package contains:
    ///  - `U_Anduril_NixOS_V1R2_Manual_STIG/U_Anduril_NixOS_STIG_V1R2_Manual-xccdf.xml`
    ///    (361 KB, xmlns="http://checklists.nist.gov/xccdf/1.1")
    ///  - several PDF and XSL auxiliary files
    ///
    /// This fixture reproduces the namespace, element structure, and
    /// auxiliary-file layout without the copyrighted content.
    fn nixos_stig_xccdf_1_1() -> Vec<u8> {
        let xccdf = r#"<?xml version="1.0" encoding="utf-8"?><?xml-stylesheet type='text/xsl' href='STIG_unclass.xsl'?>
<Benchmark xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
    xmlns:cpe="http://cpe.mitre.org/language/2.0"
    xmlns:xhtml="http://www.w3.org/1999/xhtml"
    xmlns:dsig="http://www.w3.org/2000/09/xmldsig#"
    xsi:schemaLocation="http://checklists.nist.gov/xccdf/1.1 http://nvd.nist.gov/schema/xccdf-1.1.4.xsd"
    id="Anduril_NixOS_STIG_fixture" xml:lang="en"
    xmlns="http://checklists.nist.gov/xccdf/1.1">
  <status date="2025-08-19">accepted</status>
  <title>Anduril NixOS Security Technical Implementation Guide (Reduced Fixture)</title>
  <description>Reduced fixture derived from U_Anduril_NixOS_V1R2_STIG for CI testing.</description>
  <notice id="terms-of-use" xml:lang="en"/>
  <reference href="https://cyber.mil">
    <dc:publisher>DISA</dc:publisher>
    <dc:source>STIG.DOD.MIL</dc:source>
  </reference>
  <plain-text id="release-info">Release: 2 Benchmark Date: 01 Oct 2025</plain-text>
  <version>1</version>
  <Profile id="MAC-1_Classified">
    <title>I - Mission Critical Classified</title>
    <select idref="SV-268078r1_rule" selected="true"/>
  </Profile>
  <Group id="V-268078">
    <title>GEN000000-fixture</title>
    <Rule id="SV-268078r1_rule" severity="medium">
      <title>The NixOS operating system must be configured correctly.</title>
      <description>Without proper configuration, information cannot be protected.</description>
      <check system="C-268078r1_chk">
        <check-content>Verify the NixOS configuration as required.</check-content>
      </check>
      <fix id="F-268078r1_fix">Apply the required configuration.</fix>
    </Rule>
  </Group>
</Benchmark>"#;
        xccdf.as_bytes().to_vec()
    }

    fn make_nixos_stig_zip() -> Vec<u8> {
        use std::io::Write;
        let xccdf = nixos_stig_xccdf_1_1();
        let mut buf = Vec::new();
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);
        // Mirror the real package directory layout.
        w.add_directory(
            "U_Anduril_NixOS_V1R2_Manual_STIG/",
            zip::write::FileOptions::<()>::default(),
        )
        .unwrap();
        w.start_file("U_Anduril_NixOS_V1R2_Manual_STIG/STIG_unclass.xsl", opts)
            .unwrap();
        w.write_all(b"<xsl:stylesheet xmlns:xsl='http://www.w3.org/1999/XSL/Transform'/>")
            .unwrap();
        w.start_file(
            "U_Anduril_NixOS_V1R2_Manual_STIG/U_Anduril_NixOS_STIG_V1R2_Manual-xccdf.xml",
            opts,
        )
        .unwrap();
        w.write_all(&xccdf).unwrap();
        w.finish().unwrap();
        buf
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn preview_accepts_nixos_stig_structure() {
        // This test validates that the full workflow works for a ZIP whose
        // structure matches the real Anduril NixOS V1R2 STIG:
        //   - XCCDF 1.1.4 namespace
        //   - XML file inside a subdirectory alongside auxiliary files
        //   - DC and CPE auxiliary namespaces
        //
        // Run the untouched real package instead by pointing NIXOS_STIG_ZIP to
        // its local path:
        //   NIXOS_STIG_ZIP=/path/to/U_Anduril_NixOS_V1R2_STIG.zip \
        //   DATABASE_URL=... cargo test preview_accepts_nixos_stig_structure -- --ignored

        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_preview_server(pool).await;

        let zip_bytes = if let Ok(path) = std::env::var("NIXOS_STIG_ZIP") {
            std::fs::read(&path).expect("NIXOS_STIG_ZIP path should be readable")
        } else {
            make_nixos_stig_zip()
        };

        let zip_name = if std::env::var("NIXOS_STIG_ZIP").is_ok() {
            "U_Anduril_NixOS_V1R2_STIG.zip"
        } else {
            "nixos_stig_fixture.zip"
        };

        let mut body = Vec::new();
        push_file_field(&mut body, "file", zip_name, &zip_bytes);
        finish_multipart(&mut body);

        let response = post_multipart(&base, &token, body).await;
        assert_eq!(
            response.status().as_u16(),
            200,
            "NixOS STIG ZIP should preview successfully"
        );

        let json: serde_json::Value = response.json().await.expect("json body");

        // Document detected as foreign XCCDF (no CF extension elements).
        assert_eq!(json["document_class"], "foreignxccdf");

        // XCCDF 1.1 namespace is detected and reported.
        assert_eq!(
            json["xccdf_namespace_version"], "1.1",
            "NixOS STIG uses XCCDF 1.1 namespace"
        );

        // Benchmark content is captured.
        let bm = json["benchmark"].as_object().expect("benchmark object");
        assert!(
            bm["id"].as_str().unwrap().contains("NixOS") || !bm["id"].as_str().unwrap().is_empty()
        );

        // At least one rule found.
        assert!(json["rule_count"].as_u64().unwrap_or(0) >= 1);

        // Source provenance reflects the ZIP package.
        let source = &json["source"];
        assert_eq!(source["package_kind"], "zip_package");
        assert!(source["original_sha256"].is_string());
        assert!(source["selected_entry"].as_str().unwrap().ends_with(".xml"));

        // Original ZIP sha256 differs from the inner XML sha256.
        assert_ne!(json["sha256"], source["selected_xml_sha256"]);
    }

    // ── Export endpoint helpers ───────────────────────────────────────────────

    async fn spawn_export_server(pool: PgPool) -> String {
        let app = Router::new()
            .route(
                "/api/v1/compliance/bundle-versions/:version_id/xccdf",
                get(export_bundle_xccdf),
            )
            .with_state(pool);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve export app");
        });
        format!("http://{addr}")
    }

    /// Insert a minimal but complete chain of test data for export tests.
    /// Returns `(pool, bundle_id, version_id)`.
    async fn create_export_test_data(pool: &PgPool) -> (PgPool, Uuid, Uuid) {
        let bundle_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();
        let policy_version_id = Uuid::new_v4();
        let suffix = Uuid::new_v4().simple().to_string();
        let bundle_name = format!("Test Bundle for Export {suffix}");
        let policy_name = format!("test-export-policy-{suffix}");

        // 1. compliance_bundles row
        sqlx::query(
            r#"INSERT INTO compliance_bundles (id, name, framework, layer, owner)
               VALUES ($1, $2, 'NIST', 'nixos', 'test')"#,
        )
        .bind(bundle_id)
        .bind(&bundle_name)
        .execute(pool)
        .await
        .expect("insert compliance_bundles");

        // 2. compliance_bundle_versions row
        sqlx::query(
            r#"INSERT INTO compliance_bundle_versions
               (id, bundle_id, version, publication_state, semantic_digest,
                name, framework, layer, owner)
               VALUES ($1, $2, '1.0.0', 'draft', 'abc123',
                       $3, 'NIST', 'nixos', 'test')"#,
        )
        .bind(version_id)
        .bind(bundle_id)
        .bind(&bundle_name)
        .execute(pool)
        .await
        .expect("insert compliance_bundle_versions");

        // 3. Point bundle to draft version
        sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = $1 WHERE id = $2")
            .bind(version_id)
            .bind(bundle_id)
            .execute(pool)
            .await
            .expect("update bundle draft version");

        // 4. deployment_policies row (trigger auto-creates deployment_policy_versions)
        sqlx::query(
            r#"INSERT INTO deployment_policies (id, name, policy_type, enabled, config)
               VALUES ($1, $2, 'require_packages', true, '{"packages": ["curl"]}'::jsonb)"#,
        )
        .bind(policy_id)
        .bind(&policy_name)
        .execute(pool)
        .await
        .expect("insert deployment_policies");

        // The trigger creates a draft version; fetch its id.
        let _created_policy_version_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM deployment_policy_versions WHERE policy_id = $1 LIMIT 1",
        )
        .bind(policy_id)
        .fetch_one(pool)
        .await
        .expect("fetch auto-created policy version");

        // 5. Insert a second explicit policy version row for testing
        sqlx::query(
            r#"INSERT INTO deployment_policy_versions
               (id, policy_id, version, publication_state, name, policy_type,
                implementation_state, config, semantic_digest)
               VALUES ($1, $2, '2.0.0', 'draft', $3,
                       'require_packages', 'native', '{"packages": ["curl"]}'::jsonb, 'policy-digest')"#,
        )
        .bind(policy_version_id)
        .bind(policy_id)
        .bind(&policy_name)
        .execute(pool)
        .await
        .expect("insert deployment_policy_versions");

        // 6. compliance_bundle_version_policies linking version → policy version
        sqlx::query(
            r#"INSERT INTO compliance_bundle_version_policies
               (bundle_version_id, policy_version_id, policy_order, selected)
               VALUES ($1, $2, 1, true)"#,
        )
        .bind(version_id)
        .bind(policy_version_id)
        .execute(pool)
        .await
        .expect("insert compliance_bundle_version_policies");

        (pool.clone(), bundle_id, version_id)
    }

    // ── Export endpoint tests ─────────────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn export_returns_200_with_xml_content_type() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let (_pool, _bundle_id, version_id) = create_export_test_data(&pool).await;
        let base = spawn_export_server(pool).await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "{base}/api/v1/compliance/bundle-versions/{version_id}/xccdf"
            ))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .expect("export request completes");

        assert_eq!(resp.status().as_u16(), 200);
        let ct = resp
            .headers()
            .get("content-type")
            .expect("content-type header")
            .to_str()
            .unwrap()
            .to_string();
        assert!(ct.contains("application/xml"), "got content-type: {ct}");

        let body = resp.text().await.expect("text body");
        assert!(
            body.starts_with("<?xml"),
            "body should start with XML declaration, got: {}",
            &body[..body.len().min(80)]
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn export_includes_content_disposition_header() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let (_pool, _bundle_id, version_id) = create_export_test_data(&pool).await;
        let base = spawn_export_server(pool).await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "{base}/api/v1/compliance/bundle-versions/{version_id}/xccdf"
            ))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .expect("export request completes");

        let cd = resp
            .headers()
            .get("content-disposition")
            .expect("content-disposition header")
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            cd.contains("attachment; filename="),
            "Content-Disposition missing attachment; filename=: {cd}"
        );
        assert!(
            cd.trim_end_matches('"').ends_with(".xml"),
            "Content-Disposition filename should end with .xml: {cd}"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn export_returns_valid_xccdf_body() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let (_pool, _bundle_id, version_id) = create_export_test_data(&pool).await;
        let base = spawn_export_server(pool).await;

        let client = reqwest::Client::new();
        let body = client
            .get(format!(
                "{base}/api/v1/compliance/bundle-versions/{version_id}/xccdf"
            ))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .expect("export request completes")
            .text()
            .await
            .expect("text body");

        assert!(
            body.contains("<Benchmark"),
            "body should contain <Benchmark element"
        );
        assert!(
            body.contains("<status>"),
            "body should contain <status> element"
        );
        assert!(
            body.contains("<title>Test Bundle for Export "),
            "body should contain the bundle title"
        );
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn export_returns_404_for_nonexistent_version() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;
        let base = spawn_export_server(pool).await;
        let fake_id = Uuid::new_v4();

        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "{base}/api/v1/compliance/bundle-versions/{fake_id}/xccdf"
            ))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .expect("export request completes");

        assert_eq!(resp.status().as_u16(), 404);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn export_returns_403_without_auth() {
        let pool = test_pool_from_env().await;
        let (_pool, _bundle_id, version_id) = create_export_test_data(&pool).await;
        let base = spawn_export_server(pool).await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "{base}/api/v1/compliance/bundle-versions/{version_id}/xccdf"
            ))
            .send()
            .await
            .expect("export request completes");

        assert_eq!(resp.status().as_u16(), 403);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn export_returns_200_with_no_rules_for_empty_bundle() {
        let pool = test_pool_from_env().await;
        let token = admin_session_token(&pool).await;

        let bundle_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let bundle_name = format!("Empty Export Bundle {}", Uuid::new_v4().simple());

        sqlx::query(
            r#"INSERT INTO compliance_bundles (id, name, framework, layer, owner)
               VALUES ($1, $2, 'NIST', 'nixos', 'test')"#,
        )
        .bind(bundle_id)
        .bind(&bundle_name)
        .execute(&pool)
        .await
        .expect("insert empty bundle");

        sqlx::query(
            r#"INSERT INTO compliance_bundle_versions
               (id, bundle_id, version, publication_state, semantic_digest,
                name, framework, layer, owner)
               VALUES ($1, $2, '1.0.0', 'draft', 'empty-digest',
                       $3, 'NIST', 'nixos', 'test')"#,
        )
        .bind(version_id)
        .bind(bundle_id)
        .bind(&bundle_name)
        .execute(&pool)
        .await
        .expect("insert empty bundle version");

        sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = $1 WHERE id = $2")
            .bind(version_id)
            .bind(bundle_id)
            .execute(&pool)
            .await
            .expect("update empty bundle draft version");

        let base = spawn_export_server(pool).await;

        let client = reqwest::Client::new();
        let body = client
            .get(format!(
                "{base}/api/v1/compliance/bundle-versions/{version_id}/xccdf"
            ))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
            .send()
            .await
            .expect("export request completes")
            .text()
            .await
            .expect("text body");

        assert!(
            body.contains("<Benchmark"),
            "empty bundle should still contain <Benchmark element"
        );
        assert!(
            !body.contains("<Rule"),
            "empty bundle should not contain <Rule> elements"
        );
    }
}
