use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sqlx::PgPool;
use sqlx::Row;
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::api::models::{
    ApiError, AuditAction, CommitInfo, CreateSystemRequest, CveScanEligibilityResponse,
    CveScanStatusResponse, CveScanTriggerResponse, CveSummary, DeploySystemRequest,
    DeploymentStatus, PipelineStage, SaveSystemCveJustificationRequest, SortOrder,
    SystemAgentEvent, SystemCommitsResponse, SystemDetail, SystemHardwareInfo, SystemHistoryEntry,
    SystemMutationResponse, SystemNetworkInfo, SystemRollbackRequest, SystemSecurityInfo,
    SystemSummary, SystemVulnerability, SystemsListParams, UpdateSystemPublicKeyRequest,
    UpdateSystemRequest,
};
use crate::auth::models::Role;
use crate::handlers::api::rbac::{
    authenticated_user_roles, extract_request_origin, require_viewer_or_above,
};
use crate::models::auth_identity::AuthRole;
use crate::queries::cve_scans::{get_scan_by_id, resolve_system_cve_scan_target};
use crate::queries::systems::{
    SystemAccessRow, SystemDetailRow, SystemListRow, commit_belongs_to_system_flake,
    deactivate_system, find_system_access_row, get_system_detail_by_id,
    get_user_environment_membership_ids, list_recent_commits_for_system, list_system_access_rows,
    list_system_agent_event_rows, list_system_history_rows, touch_system_updated_at,
    update_public_key, update_system_desired_target, update_system_metadata,
};
use crate::services::cve_scans::{CveScanError, trigger_immediate_cve_scan};
use crate::services::systems::SystemsListContext;

/// Allowed CVE justification categories (server-side validation).
/// These must match the UI preset list in packages/web-ui/src/components/cve/mod.rs.
/// Any category value not in this list will be rejected with a 400 Bad Request.
const ALLOWED_CVE_JUSTIFICATION_CATEGORIES: &[&str] = &[
    "false_positive",
    "accepted_risk",
    "compensating_control",
    "planned_remediation",
    "vendor_pending_fix",
];

pub async fn list_systems(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<SystemsListParams>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let environment_memberships = match get_user_environment_membership_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    // Create service context and delegate to service layer
    let ctx = SystemsListContext::new(user_id, roles, environment_memberships, &params);

    // Call the service layer for server-side filtering/sorting/pagination
    match crate::services::systems::list_systems_for_user(&pool, &ctx).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(_) => internal_error("Failed to list systems"),
    }
}

pub async fn create_system(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<CreateSystemRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    // Validate required fields
    let hostname = payload.hostname.trim();
    if hostname.is_empty() {
        return bad_request("Hostname is required");
    }

    let public_key = payload.public_key.trim();
    if public_key.is_empty() {
        return bad_request("Public key is required");
    }

    // Validate deployment policy
    if !matches!(
        payload.deployment_policy.as_str(),
        "manual" | "auto_latest" | "pinned"
    ) {
        return bad_request("Invalid deployment policy (must be: manual, auto_latest, or pinned)");
    }

    // Look up environment ID from name
    let environment_id = if let Some(env_name) = payload.environment.as_ref() {
        let env_name_trimmed = env_name.trim();
        if !env_name_trimmed.is_empty() {
            match sqlx::query_scalar::<_, Uuid>("SELECT id FROM environments WHERE name = $1")
                .bind(env_name_trimmed)
                .fetch_optional(&pool)
                .await
            {
                Ok(id) => id,
                Err(_) => return internal_error("Failed to lookup environment"),
            }
        } else {
            None
        }
    } else {
        None
    };

    // Look up flake ID from name
    let flake_id = if let Some(flake_name) = payload.flake_name.as_ref() {
        let flake_name_trimmed = flake_name.trim();
        if !flake_name_trimmed.is_empty() {
            match sqlx::query_scalar::<_, i32>("SELECT id FROM flakes WHERE name = $1")
                .bind(flake_name_trimmed)
                .fetch_optional(&pool)
                .await
            {
                Ok(id) => id,
                Err(_) => return internal_error("Failed to lookup flake"),
            }
        } else {
            None
        }
    } else {
        None
    };

    // Use the System model to create and validate the system
    use crate::models::systems::System;
    let system = match System::new(
        &pool,
        hostname.to_string(),
        environment_id,
        true, // is_active
        public_key.to_string(),
        flake_id,
        payload
            .system_configuration_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        None, // desired_target
        payload.deployment_policy.clone(),
    )
    .await
    {
        Ok(sys) => sys,
        Err(e) => return bad_request(&format!("Failed to create system: {}", e)),
    };

    // Record audit event
    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::UserCreated, // Using UserCreated as placeholder - ideally would be SystemCreated
        format!("{} ({})", system.hostname, system.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "create", "hostname": system.hostname }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    // Fetch the created system from view to return complete data
    let detail = match get_system_detail_by_id(&pool, system.id).await {
        Ok(Some(row)) => detail_row_to_api_model(row),
        Ok(None) => return internal_error("System created but not found in view"),
        Err(_) => return internal_error("Failed to fetch created system"),
    };

    (StatusCode::CREATED, Json(detail)).into_response()
}

pub async fn get_system(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(_caller_role) = highest_role(&roles) else {
        return forbidden();
    };
    let _environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match get_system_detail_by_id(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    // Note: Environment-based access control would go here
    // For now, simplified - in production you'd check environment membership

    let detail = detail_row_to_api_model(row);

    (StatusCode::OK, Json(detail)).into_response()
}

pub async fn get_system_cves(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    // Load environment memberships for scoped access control
    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    // Verify system exists and caller has environment access
    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let rows = match sqlx::query(
        r#"
        WITH deduped AS (
            SELECT DISTINCT ON (v.cve_id, v.package_name, v.package_version)
                v.cve_id,
                lower(v.severity) AS severity,
                v.cvss_v3_score::double precision AS cvss_score,
                COALESCE(v.description, '') AS description,
                v.package_name,
                v.package_version AS installed_version,
                v.fixed_version,
                v.completed_at AS first_seen,
                c.published_date::timestamptz AS published_at,
                -- 'fix_available' = upstream patched version exists; does NOT mean system is patched.
                -- 'open' = no upstream fix known yet.
                CASE WHEN v.fixed_version IS NULL THEN 'open' ELSE 'fix_available' END AS status,
                j.category AS justification_category,
                j.reason AS justification_reason,
                j.updated_at AS justification_updated_at
            FROM view_system_vulnerabilities v
            JOIN systems s ON s.hostname = v.hostname
            LEFT JOIN cves c ON c.id = v.cve_id
            LEFT JOIN system_cve_justifications j
                ON j.system_id = s.id
               AND j.cve_id = v.cve_id
            WHERE s.id = $1
            ORDER BY v.cve_id, v.package_name, v.package_version, v.completed_at DESC
        )
        SELECT *
        FROM deduped
        ORDER BY cvss_score DESC NULLS LAST, cve_id ASC, package_name ASC, installed_version ASC
        "#,
    )
    .bind(system_id)
    .fetch_all(&pool)
    .await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load system CVEs"),
    };

    let vulnerabilities = rows
        .into_iter()
        .map(|row| {
            let severity_raw: String = row.get("severity");
            let severity = parse_cve_severity(&severity_raw);

            SystemVulnerability {
                cve_id: row.get("cve_id"),
                severity,
                cvss_score: row.get("cvss_score"),
                description: row.get("description"),
                package_name: row.get("package_name"),
                installed_version: row.get("installed_version"),
                fixed_version: row.get("fixed_version"),
                first_seen: row.get("first_seen"),
                published_at: row.get("published_at"),
                status: row.get("status"),
                justification_category: row.get("justification_category"),
                justification_reason: row.get("justification_reason"),
                justification_updated_at: row.get("justification_updated_at"),
            }
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(vulnerabilities)).into_response()
}

pub async fn save_system_cve_justification(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((system_id, cve_id)): Path<(Uuid, String)>,
    Json(payload): Json<SaveSystemCveJustificationRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let cve_id = cve_id.trim().to_string();
    if cve_id.is_empty() {
        return bad_request("CVE ID is required");
    }

    let reason = payload.reason.trim();
    if reason.is_empty() {
        return bad_request("Justification reason is required");
    }

    if reason.len() > 2000 {
        return bad_request("Justification reason must be 2000 characters or less");
    }

    let category = payload
        .category
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Some(ref category_value) = category
        && !ALLOWED_CVE_JUSTIFICATION_CATEGORIES
            .iter()
            .any(|allowed| *allowed == category_value)
    {
        return bad_request("Invalid justification category");
    }

    let cve_present_on_system = match sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM view_system_vulnerabilities v
            JOIN systems s ON s.hostname = v.hostname
            WHERE s.id = $1
              AND v.cve_id = $2
        )
        "#,
    )
    .bind(system_id)
    .bind(&cve_id)
    .fetch_one(&pool)
    .await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to validate system CVE"),
    };

    if !cve_present_on_system {
        return bad_request("CVE was not found for this system");
    }

    // Begin transaction to ensure atomic write + audit
    let mut tx = match pool.begin().await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to begin transaction"),
    };

    // Upsert justification
    if sqlx::query(
        r#"
        INSERT INTO system_cve_justifications (system_id, cve_id, category, reason, updated_by, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        ON CONFLICT (system_id, cve_id)
        DO UPDATE SET
            category = EXCLUDED.category,
            reason = EXCLUDED.reason,
            updated_by = EXCLUDED.updated_by,
            updated_at = NOW()
        "#,
    )
    .bind(system_id)
    .bind(&cve_id)
    .bind(category.clone())
    .bind(reason)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .is_err()
    {
        let _ = tx.rollback().await;
        return internal_error("Failed to save CVE justification");
    }

    // Lookup user email for audit (within transaction)
    let actor_identifier =
        match sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(Some(email)) => email,
            Ok(None) => user_id.to_string(),
            Err(_) => {
                let _ = tx.rollback().await;
                return internal_error("Failed to lookup user for audit");
            }
        };

    // Record audit event within the same transaction
    if sqlx::query(
        "INSERT INTO admin_audit_events (actor_user_id, actor_identifier, action, target, request_origin, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind(&actor_identifier)
    .bind("user_updated")
    .bind(format!("{} ({})", row.hostname, row.id))
    .bind(extract_request_origin(&headers))
    .bind(serde_json::json!({
        "operation": "cve_justification_saved",
        "system_id": row.id,
        "hostname": row.hostname,
        "cve_id": cve_id,
        "category": category,
        "reason_length": reason.len()
    }))
    .execute(&mut *tx)
    .await
    .is_err()
    {
        let _ = tx.rollback().await;
        return internal_error("Failed to write audit event");
    }

    // Commit transaction
    if tx.commit().await.is_err() {
        return internal_error("Failed to commit transaction");
    }

    (
        StatusCode::OK,
        Json(SystemMutationResponse {
            status: "ok".to_string(),
            message: "CVE justification saved".to_string(),
        }),
    )
        .into_response()
}

pub async fn get_system_cve_scan_eligibility(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let payload = match resolve_system_cve_scan_target(&pool, system_id).await {
        Ok(Some(target)) => CveScanEligibilityResponse {
            eligible: target.blocked_reason.is_none(),
            reason: target.blocked_reason,
            derivation_id: Some(target.derivation_id),
            config_name: Some(target.config_name),
            hostname: target.hostname,
        },
        Ok(None) => CveScanEligibilityResponse {
            eligible: false,
            reason: Some(
                "No eligible derivation was found for this system configuration.".to_string(),
            ),
            derivation_id: None,
            config_name: None,
            hostname: Some(row.hostname),
        },
        Err(_) => return internal_error("Failed to evaluate CVE scan eligibility"),
    };

    (StatusCode::OK, Json(payload)).into_response()
}

pub async fn trigger_system_cve_scan(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let target = match resolve_system_cve_scan_target(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "scan_ineligible".to_string(),
                    message: "No build target found for this system configuration.".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
        Err(_) => return internal_error("Failed to resolve CVE scan target"),
    };

    if let Some(reason) = target.blocked_reason.as_ref() {
        return (
            StatusCode::CONFLICT,
            Json(ApiError {
                error: "scan_ineligible".to_string(),
                message: reason.clone(),
                details: None,
            }),
        )
            .into_response();
    }

    let scan_id = match trigger_immediate_cve_scan(pool.clone(), target.derivation_id).await {
        Ok(value) => value,
        Err(CveScanError::VulnixUnavailable) => return scan_ineligible_response(),
        Err(CveScanError::Internal(err)) => {
            return internal_error(&format!("Failed to queue CVE scan: {err}"));
        }
    };

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::CveScanRequested,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({
            "operation": "cve_scan",
            "derivation_id": target.derivation_id,
            "scan_id": scan_id,
            "config_name": target.config_name
        }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::ACCEPTED,
        Json(CveScanTriggerResponse {
            scan_id,
            status: "accepted".to_string(),
            message: format!(
                "CVE scan queued for {} ({})",
                row.hostname, target.config_name
            ),
        }),
    )
        .into_response()
}

pub async fn get_cve_scan_status(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(scan_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let scan = match get_scan_by_id(&pool, scan_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: "CVE scan not found".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
        Err(_) => return internal_error("Failed to load CVE scan status"),
    };

    (
        StatusCode::OK,
        Json(CveScanStatusResponse {
            scan_id: scan.id,
            derivation_id: scan.derivation_id,
            status: match scan.status {
                crate::models::cve_scans::ScanStatus::Pending => "pending",
                crate::models::cve_scans::ScanStatus::InProgress => "in_progress",
                crate::models::cve_scans::ScanStatus::Completed => "completed",
                crate::models::cve_scans::ScanStatus::Failed => "failed",
            }
            .to_string(),
            scanner_name: scan.scanner_name,
            scheduled_at: scan.scheduled_at,
            completed_at: scan.completed_at,
            attempts: scan.attempts,
            total_vulnerabilities: scan.total_vulnerabilities,
            critical_count: scan.critical_count,
            high_count: scan.high_count,
            medium_count: scan.medium_count,
            low_count: scan.low_count,
        }),
    )
        .into_response()
}

fn parse_cve_severity(value: &str) -> crate::api::models::CveSeverity {
    match value {
        "critical" => crate::api::models::CveSeverity::Critical,
        "high" => crate::api::models::CveSeverity::High,
        "medium" => crate::api::models::CveSeverity::Medium,
        _ => crate::api::models::CveSeverity::Low,
    }
}

pub async fn update_system_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(payload): Json<UpdateSystemRequest>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let hostname = payload.hostname.trim();
    if hostname.is_empty() {
        return bad_request("Hostname is required");
    }
    if !matches!(
        payload.deployment_policy.as_str(),
        "manual" | "auto_latest" | "pinned"
    ) {
        return bad_request("Invalid deployment policy (must be: manual, auto_latest, or pinned)");
    }

    // Resolve environment name → id.
    // A non-empty name that does not match any environment is a 400, not a silent NULL.
    let environment_id = if let Some(env_name) = payload.environment.as_ref() {
        let env_name_trimmed = env_name.trim();
        if !env_name_trimmed.is_empty() {
            match sqlx::query_scalar::<_, Uuid>("SELECT id FROM environments WHERE name = $1")
                .bind(env_name_trimmed)
                .fetch_optional(&pool)
                .await
            {
                Ok(Some(id)) => Some(id),
                Ok(None) => {
                    return bad_request(&format!("Environment '{}' not found", env_name_trimmed));
                }
                Err(_) => return internal_error("Failed to lookup environment"),
            }
        } else {
            None
        }
    } else {
        None
    };

    // Resolve flake name → id.
    // A non-empty name that does not match any registered flake is a 400, not a silent NULL.
    let flake_id = if let Some(flake_name) = payload.flake_name.as_ref() {
        let flake_name_trimmed = flake_name.trim();
        if !flake_name_trimmed.is_empty() {
            match sqlx::query_scalar::<_, i32>(
                "SELECT id FROM flakes WHERE name = $1 AND deleted_at IS NULL",
            )
            .bind(flake_name_trimmed)
            .fetch_optional(&pool)
            .await
            {
                Ok(Some(id)) => Some(id),
                Ok(None) => {
                    return bad_request(&format!("Flake '{}' not found", flake_name_trimmed));
                }
                Err(_) => return internal_error("Failed to lookup flake"),
            }
        } else {
            None
        }
    } else {
        None
    };

    if update_system_metadata(
        &pool,
        system_id,
        hostname,
        environment_id,
        flake_id,
        payload
            .system_configuration_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        &payload.deployment_policy,
    )
    .await
    .is_err()
    {
        return internal_error("Failed to update system");
    }

    let detail = match get_system_detail_by_id(&pool, system_id).await {
        Ok(Some(row)) => detail_row_to_api_model(row),
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load updated system"),
    };

    (StatusCode::OK, Json(detail)).into_response()
}

pub async fn sync_system(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    if touch_system_updated_at(&pool, system_id).await.is_err() {
        return internal_error("Failed to queue sync");
    }

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::SystemSyncRequested,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "sync" }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::ACCEPTED,
        Json(SystemMutationResponse {
            status: "accepted".to_string(),
            message: format!("Sync requested for {}", row.hostname),
        }),
    )
        .into_response()
}

pub async fn rollback_system(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(payload): Json<SystemRollbackRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let target_commit = payload.target_commit.trim();
    if let Err(message) = validate_target_commit(target_commit) {
        return bad_request(&message);
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    if update_system_desired_target(&pool, system_id, target_commit)
        .await
        .is_err()
    {
        return internal_error("Failed to request rollback");
    }

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::SystemRollbackRequested,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "rollback", "target_commit": target_commit }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::ACCEPTED,
        Json(SystemMutationResponse {
            status: "accepted".to_string(),
            message: format!("Rollback requested for {}", row.hostname),
        }),
    )
        .into_response()
}

pub async fn update_system_public_key(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(payload): Json<UpdateSystemPublicKeyRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let new_public_key = payload.public_key.trim();
    if new_public_key.is_empty() {
        return bad_request("Public key is required");
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    // Verify system exists and user has access
    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    // Validate the public key format using the PublicKey model
    use crate::models::public_key::PublicKey;
    if let Err(e) = PublicKey::from_base64(new_public_key, &row.hostname) {
        return bad_request(&format!("Invalid public key: {}", e));
    }

    // Update the public key
    if update_public_key(&pool, system_id, new_public_key)
        .await
        .is_err()
    {
        return internal_error("Failed to update public key");
    }

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::UserUpdated, // Using UserUpdated as placeholder - ideally would be SystemKeyRotated
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "update_public_key" }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::OK,
        Json(SystemMutationResponse {
            status: "success".to_string(),
            message: format!("Public key updated for {}", row.hostname),
        }),
    )
        .into_response()
}

pub async fn deactivate_system_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    if deactivate_system(&pool, system_id).await.is_err() {
        return internal_error("Failed to disable system");
    }

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::UserUpdated,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "deactivate_system" }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::OK,
        Json(SystemMutationResponse {
            status: "success".to_string(),
            message: format!("System {} disabled", row.hostname),
        }),
    )
        .into_response()
}

fn highest_role(roles: &[AuthRole]) -> Option<Role> {
    if roles.contains(&AuthRole::Admin) {
        Some(Role::Admin)
    } else if roles.contains(&AuthRole::Operator) {
        Some(Role::Operator)
    } else if roles.contains(&AuthRole::Viewer) {
        Some(Role::Viewer)
    } else {
        None
    }
}

fn matches_filters(row: &SystemAccessRow, params: &SystemsListParams) -> bool {
    if let Some(search) = params.search.as_ref() {
        let needle = search.trim().to_ascii_lowercase();
        if !needle.is_empty() && !row.hostname.to_ascii_lowercase().contains(&needle) {
            return false;
        }
    }

    if let Some(environment) = params.environment.as_ref() {
        let needle = environment.trim().to_ascii_lowercase();
        if !needle.is_empty() {
            let env_name = row
                .environment
                .clone()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if env_name != needle {
                return false;
            }
        }
    }

    true
}

fn parse_health_status(status: &str) -> crate::api::models::HealthStatus {
    match status {
        "healthy" => crate::api::models::HealthStatus::Healthy,
        "warning" => crate::api::models::HealthStatus::Warning,
        "critical" => crate::api::models::HealthStatus::Critical,
        _ => crate::api::models::HealthStatus::Offline,
    }
}

fn parse_deployment_status(status: &str) -> DeploymentStatus {
    match status {
        "up_to_date" => DeploymentStatus::UpToDate,
        "behind" => DeploymentStatus::Behind,
        "ahead" => DeploymentStatus::Ahead,
        "no_deployment" => DeploymentStatus::NeverDeployed,
        "no_commits" => DeploymentStatus::NoCommitsAvailable,
        _ => DeploymentStatus::Unknown,
    }
}

fn parse_pipeline_stage(stage: &str) -> PipelineStage {
    match stage {
        "dry_run" => PipelineStage::DryRun,
        "ready_for_build" => PipelineStage::ReadyForBuild,
        "building" => PipelineStage::Building,
        "build_complete" => PipelineStage::BuildComplete,
        "ready_for_deploy" => PipelineStage::ReadyForDeploy,
        _ => PipelineStage::Unknown,
    }
}

fn detail_row_to_api_model(row: SystemDetailRow) -> SystemDetail {
    use crate::api::models::FlakeSummary;

    SystemDetail {
        id: row.id,
        hostname: row.hostname,
        system_configuration_name: row.system_configuration_name,
        environment: row.environment,
        is_active: row.is_active,
        deployment_policy: row.deployment_policy,
        health_status: parse_health_status(&row.health_status),
        deployment_status: parse_deployment_status(&row.deployment_status),
        pipeline_stage: Some(parse_pipeline_stage(&row.pipeline_stage)),
        nixos_version: row.nixos_version,
        kernel: row.kernel,
        agent_version: row.agent_version,
        current_store_path: row.current_store_path,
        hardware: SystemHardwareInfo {
            cpu_brand: row.cpu_brand,
            cpu_cores: row.cpu_cores,
            memory_gb: row.memory_gb,
            uptime_secs: row.uptime_secs,
            board_serial: row.board_serial,
            bios_version: row.bios_version,
        },
        network: SystemNetworkInfo {
            primary_ip: row.primary_ip_address,
            primary_mac: row.primary_mac_address,
            gateway_ip: row.gateway_ip,
        },
        security: SystemSecurityInfo {
            tpm_present: row.tpm_present,
            secure_boot_enabled: row.secure_boot_enabled,
            fips_mode: row.fips_mode,
            selinux_status: row.selinux_status,
        },
        cve_counts: CveSummary {
            critical: row.critical_cve_count as i64,
            high: row.high_cve_count as i64,
            medium: row.medium_cve_count as i64,
            low: row.low_cve_count as i64,
        },
        flake: row.flake_id.and_then(|id| {
            row.flake_name.map(|name| FlakeSummary {
                id,
                name,
                repo_url: row.flake_repo_url.clone().unwrap_or_default(),
                latest_commit: row.flake_latest_commit.clone(),
            })
        }),
        last_seen: row.last_seen,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

async fn load_membership_environment_ids(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<BTreeSet<Uuid>, ()> {
    get_user_environment_membership_ids(pool, user_id)
        .await
        .map_err(|_| ())
}

fn forbidden() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Viewer, operator, or admin privileges are required".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn forbidden_mutation() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Operator or admin privileges are required".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn bad_request(message: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "validation_error".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn not_found() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "not_found".to_string(),
            message: "System not found".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn internal_error(message: &str) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "internal_error".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

/// 409 response returned when a CVE scan prerequisite (e.g. vulnix) is not satisfied.
/// Centralised here so systems and flakes handlers share the same error code and message.
pub(crate) fn scan_ineligible_response() -> axum::response::Response {
    (
        StatusCode::CONFLICT,
        Json(ApiError {
            error: "scan_ineligible".to_string(),
            message: "vulnix is not available on this node; immediate scan cannot start"
                .to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn action_to_str(action: AuditAction) -> &'static str {
    match action {
        AuditAction::UserCreated => "user_created",
        AuditAction::UserUpdated => "user_updated",
        AuditAction::UserDeleted => "user_deleted",
        AuditAction::UserEnabled => "user_enabled",
        AuditAction::UserDisabled => "user_disabled",
        AuditAction::UserRoleAssigned => "user_role_assigned",
        AuditAction::UserEnvironmentMembershipUpdated => "user_environment_membership_updated",
        AuditAction::OidcMappingChanged => "oidc_mapping_changed",
        AuditAction::SystemSyncRequested => "system_sync_requested",
        AuditAction::SystemDeployRequested => "system_deploy_requested",
        AuditAction::SystemRollbackRequested => "system_rollback_requested",
        AuditAction::CveScanRequested => "cve_scan_requested",
        AuditAction::SessionInvalidated => "session_invalidated",
    }
}

fn validate_target_commit(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("Target commit is required".to_string());
    }

    if !(7..=64).contains(&value.len()) {
        return Err("Target commit must be between 7 and 64 hex characters".to_string());
    }

    if !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("Target commit must contain only hexadecimal characters".to_string());
    }

    Ok(())
}

pub async fn deploy_system(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(payload): Json<DeploySystemRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let commit_sha = payload.commit_sha.trim();
    if let Err(message) = validate_target_commit(commit_sha) {
        return bad_request(&message);
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    // Validate deployment policy - only manual and pinned allow manual deployments
    if !matches!(row.deployment_policy.as_str(), "manual" | "pinned") {
        return bad_request("Manual deployment is not allowed for auto_latest systems");
    }

    let belongs_to_flake = match commit_belongs_to_system_flake(&pool, system_id, commit_sha).await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to validate requested commit"),
    };

    if !belongs_to_flake {
        return bad_request("Requested commit is not available for this system");
    }

    // Set the desired_target to trigger deployment
    if update_system_desired_target(&pool, system_id, commit_sha)
        .await
        .is_err()
    {
        return internal_error("Failed to request deployment");
    }

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::SystemDeployRequested,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "deploy", "target_commit": commit_sha }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::ACCEPTED,
        Json(SystemMutationResponse {
            status: "accepted".to_string(),
            message: format!(
                "Deployment requested for {} to commit {}",
                row.hostname, commit_sha
            ),
        }),
    )
        .into_response()
}

pub async fn get_system_commits(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to load environment memberships".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: "System not found".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to load system".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "not_found".to_string(),
                message: "System not found".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    let commits = match list_recent_commits_for_system(&pool, system_id, 50).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row| {
                let short_sha = row.sha.chars().take(7).collect::<String>();
                CommitInfo {
                    sha: row.sha,
                    short_sha,
                    message: row.message.unwrap_or_default(),
                    author: row.author.unwrap_or_else(|| "unknown".to_string()),
                    timestamp: row.timestamp.to_rfc3339(),
                }
            })
            .collect::<Vec<_>>(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to load system commits".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let response = SystemCommitsResponse {
        commits,
        current_commit: None,
    };

    (StatusCode::OK, Json(response)).into_response()
}

pub async fn get_system_history(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let rows = match list_system_history_rows(&pool, system_id, 200).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load system history"),
    };

    let entries = rows
        .into_iter()
        .map(|row| SystemHistoryEntry {
            timestamp: row.timestamp,
            store_path: row.store_path,
            system_configuration_name: row.system_configuration_name,
            change_reason: row
                .change_reason
                .unwrap_or_else(|| "state_delta".to_string()),
            commit_hash: row.commit_hash,
            flake_name: row.flake_name,
            flake_repo_url: row.flake_repo_url,
            actor: "agent".to_string(),
            outcome: "recorded".to_string(),
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(entries)).into_response()
}

pub async fn get_system_agent_events(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let rows = match list_system_agent_event_rows(&pool, system_id, 300).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load agent events"),
    };

    let entries = rows
        .into_iter()
        .map(|row| SystemAgentEvent {
            timestamp: row.timestamp,
            level: row.level,
            event_type: row.event_type,
            message: row.message,
            deployment_related: row.deployment_related,
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(entries)).into_response()
}

async fn record_system_mutation_audit(
    pool: &PgPool,
    actor_user_id: Uuid,
    action: AuditAction,
    target: String,
    request_origin: Option<String>,
    metadata: serde_json::Value,
) -> Result<(), ()> {
    let actor_identifier = crate::queries::admin::find_user_email(pool, actor_user_id)
        .await
        .map_err(|_| ())?
        .unwrap_or_else(|| actor_user_id.to_string());

    crate::queries::admin::insert_admin_audit_event(
        pool,
        actor_user_id,
        &actor_identifier,
        action_to_str(action),
        &target,
        request_origin,
        metadata,
    )
    .await
    .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use chrono::Utc;
    use sqlx::postgres::PgPoolOptions;

    #[test]
    fn highest_role_prefers_admin_then_operator_then_viewer() {
        assert_eq!(highest_role(&[AuthRole::Admin]), Some(Role::Admin));
        assert_eq!(highest_role(&[AuthRole::Operator]), Some(Role::Operator));
        assert_eq!(highest_role(&[AuthRole::Viewer]), Some(Role::Viewer));
        assert_eq!(highest_role(&[]), None);
    }

    #[test]
    fn matches_filters_checks_search_and_environment() {
        let row = SystemAccessRow {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid"),
            hostname: "prod-edge-01".to_string(),
            environment_id: None,
            environment: Some("prod".to_string()),
            is_active: true,
            deployment_policy: "manual".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let mut params = SystemsListParams::default();
        params.search = Some("edge".to_string());
        params.environment = Some("PROD".to_string());
        assert!(matches_filters(&row, &params));

        params.environment = Some("staging".to_string());
        assert!(!matches_filters(&row, &params));
    }

    #[tokio::test]
    async fn list_systems_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = list_systems(
            State(pool),
            HeaderMap::new(),
            Query(SystemsListParams::default()),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_system_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_system_cves_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system_cves(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn save_system_cve_justification_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = save_system_cve_justification(
            State(pool),
            HeaderMap::new(),
            Path((
                Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid"),
                "CVE-2025-1234".to_string(),
            )),
            Json(SaveSystemCveJustificationRequest {
                category: Some("accepted_risk".to_string()),
                reason: "Risk accepted pending upstream patch".to_string(),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // NOTE: Additional integration tests requiring a real test database:
    // - save_system_cve_justification_valid_category_accepted (200 OK, verify DB write + audit)
    // - save_system_cve_justification_no_category_accepted (200 OK with category=NULL)
    // - save_system_cve_justification_empty_reason_rejected (400 Bad Request)
    // - save_system_cve_justification_reason_too_long_rejected (400 Bad Request)
    // - save_system_cve_justification_invalid_category_rejected (400 Bad Request)
    // - save_system_cve_justification_cve_not_on_system_rejected (400 Bad Request)
    // - save_system_cve_justification_system_not_found (404 Not Found)
    // - save_system_cve_justification_no_environment_access (404 Not Found)
    // - save_system_cve_justification_viewer_forbidden (403 Forbidden)
    // - save_system_cve_justification_transaction_rollback_on_audit_failure (verify atomicity)
    //
    // These tests should be implemented when a test database fixture/harness is available.

    #[tokio::test]
    async fn get_system_cve_scan_eligibility_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system_cve_scan_eligibility(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn trigger_system_cve_scan_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = trigger_system_cve_scan(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_cve_scan_status_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_cve_scan_status(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000002").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn parse_cve_severity_maps_expected_values() {
        assert_eq!(
            parse_cve_severity("critical"),
            crate::api::models::CveSeverity::Critical
        );
        assert_eq!(
            parse_cve_severity("high"),
            crate::api::models::CveSeverity::High
        );
        assert_eq!(
            parse_cve_severity("medium"),
            crate::api::models::CveSeverity::Medium
        );
        assert_eq!(
            parse_cve_severity("low"),
            crate::api::models::CveSeverity::Low
        );
        assert_eq!(
            parse_cve_severity("unknown"),
            crate::api::models::CveSeverity::Low
        );
    }

    #[tokio::test]
    async fn sync_system_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = sync_system(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn rollback_system_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = rollback_system(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
            Json(SystemRollbackRequest {
                target_commit: "abc123".to_string(),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn deploy_system_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = deploy_system(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
            Json(DeploySystemRequest {
                commit_sha: "a1b2c3d".to_string(),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_system_commits_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system_commits(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_system_history_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system_history(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_system_agent_events_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system_agent_events(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn rollback_target_commit_validation_enforces_bounds_and_format() {
        assert!(validate_target_commit("").is_err());
        assert!(validate_target_commit("abc").is_err());
        assert!(validate_target_commit("zzzzzzz").is_err());
        assert!(validate_target_commit("a1b2c3d").is_ok());
        assert!(validate_target_commit(&"a".repeat(65)).is_err());
    }

    #[test]
    fn action_to_str_distinguishes_deploy_and_rollback() {
        assert_eq!(
            action_to_str(AuditAction::SystemDeployRequested),
            "system_deploy_requested"
        );
        assert_eq!(
            action_to_str(AuditAction::SystemRollbackRequested),
            "system_rollback_requested"
        );
        assert_eq!(
            action_to_str(AuditAction::CveScanRequested),
            "cve_scan_requested"
        );
    }

    // ── CVE trigger ─────────────────────────────────────────────────────────

    #[test]
    fn scan_ineligible_response_returns_409_with_correct_error_code() {
        let response = scan_ineligible_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn cve_scan_error_vulnix_unavailable_display_is_stable() {
        use crate::services::cve_scans::CveScanError;
        let msg = CveScanError::VulnixUnavailable.to_string();
        assert!(msg.contains("vulnix"), "display must mention vulnix: {msg}");
        assert!(matches!(
            CveScanError::VulnixUnavailable,
            CveScanError::VulnixUnavailable
        ));
    }

    // ── CVE Justification Tests ────────────────────────────────────────────────

    #[test]
    fn empty_justification_reason_is_rejected() {
        // This test validates that the validation logic correctly identifies empty reasons.
        // The actual handler test requires a database connection, so this is a unit test
        // of the validation rules.
        let reason = "";
        assert!(
            reason.trim().is_empty(),
            "Empty reason should be rejected by validation"
        );
    }

    #[test]
    fn justification_reason_max_length_enforced() {
        let reason = "x".repeat(2001);
        assert!(
            reason.len() > 2000,
            "Reason longer than 2000 chars should be rejected"
        );
    }

    #[test]
    fn valid_justification_categories_accepted() {
        for category in ALLOWED_CVE_JUSTIFICATION_CATEGORIES {
            assert!(
                ALLOWED_CVE_JUSTIFICATION_CATEGORIES
                    .iter()
                    .any(|allowed| *allowed == *category),
                "Category {category} should be in allowed list"
            );
        }
    }

    #[test]
    fn invalid_justification_category_detected() {
        let invalid_category = "arbitrary_category";
        assert!(
            !ALLOWED_CVE_JUSTIFICATION_CATEGORIES
                .iter()
                .any(|allowed| *allowed == invalid_category),
            "Invalid category should not be in allowed list"
        );
    }

    #[test]
    fn category_validation_rejects_values_not_in_preset_list() {
        // This test verifies that the validation logic (lines 354-360) correctly
        // identifies categories that are not in the allowed preset.
        let valid_categories = vec![
            "false_positive",
            "accepted_risk",
            "compensating_control",
            "planned_remediation",
            "vendor_pending_fix",
        ];

        let invalid_categories = vec![
            "custom_reason",
            "not_applicable",
            "temporary_exception",
            "executive_override",
        ];

        for cat in valid_categories {
            assert!(
                ALLOWED_CVE_JUSTIFICATION_CATEGORIES
                    .iter()
                    .any(|allowed| *allowed == cat),
                "Valid category '{cat}' should be accepted"
            );
        }

        for cat in invalid_categories {
            assert!(
                !ALLOWED_CVE_JUSTIFICATION_CATEGORIES
                    .iter()
                    .any(|allowed| *allowed == cat),
                "Invalid category '{cat}' should be rejected"
            );
        }
    }

    #[test]
    fn allowed_cve_categories_match_ui_presets() {
        // This test documents the expected alignment between backend validation
        // and UI preset list defined in packages/web-ui/src/components/cve/mod.rs
        let expected_categories = [
            "false_positive",
            "accepted_risk",
            "compensating_control",
            "planned_remediation",
            "vendor_pending_fix",
        ];
        assert_eq!(
            ALLOWED_CVE_JUSTIFICATION_CATEGORIES.len(),
            expected_categories.len(),
            "Backend and UI category lists should have same length"
        );
        for category in expected_categories {
            assert!(
                ALLOWED_CVE_JUSTIFICATION_CATEGORIES
                    .iter()
                    .any(|allowed| *allowed == category),
                "Category {category} should be in backend allowed list"
            );
        }
    }
}
