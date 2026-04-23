use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{
    ApiError, HardeningFleetSummaryResponse, HardeningJustificationResponse,
    HardeningScanEligibilityResponse, HardeningScanStatusResponse, HardeningScanTriggerResponse,
    HardeningServiceResultResponse, HardeningSystemPostureResponse, HardeningTopServiceResponse,
    SaveHardeningJustificationRequest, SystemMutationResponse,
};
use crate::auth::models::Role;
use crate::handlers::api::rbac::{authenticated_user_roles, require_admin};
use crate::models::auth_identity::AuthRole;
use crate::queries::hardening_scans::{
    get_fleet_summary, get_justifications_for_system, get_scan_by_id, get_service_results,
    get_system_posture, get_top_vulnerable_services, list_scan_environment_ids,
    list_system_postures,
    resolve_system_hardening_scan_target, upsert_justification,
};
use crate::queries::systems::{find_system_access_row, get_user_environment_membership_ids};
use crate::services::hardening_scans::{HardeningScanError, trigger_system_hardening_scan};

#[derive(Debug, Default, serde::Deserialize)]
pub struct TopServicesParams {
    pub limit: Option<i64>,
}

pub async fn hardening_fleet_summary(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    match get_fleet_summary(&pool).await {
        Ok(summary) => (
            StatusCode::OK,
            Json(HardeningFleetSummaryResponse {
                total_systems_scanned: summary.total_systems_scanned,
                avg_fleet_score: summary.avg_fleet_score,
                total_well_hardened_services: summary.total_well_hardened_services,
                total_moderately_hardened_services: summary.total_moderately_hardened_services,
                total_poorly_hardened_services: summary.total_poorly_hardened_services,
                total_vulnerable_services: summary.total_vulnerable_services,
                total_services_scanned: summary.total_services_scanned,
                last_scan_completed: summary.last_scan_completed,
            }),
        )
            .into_response(),
        Err(_) => internal_error("Failed to load hardening summary"),
    }
}

pub async fn hardening_top_services(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<TopServicesParams>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    let limit = params.limit.unwrap_or(10).clamp(1, 50);
    match get_top_vulnerable_services(&pool, limit).await {
        Ok(rows) => {
            let payload = rows
                .into_iter()
                .map(|item| HardeningTopServiceResponse {
                    service_name: item.service_name,
                    affected_systems_count: item.affected_systems_count,
                    avg_score: item.avg_score,
                    min_score: item.min_score,
                    max_score: item.max_score,
                })
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(payload)).into_response()
        }
        Err(_) => internal_error("Failed to load hardening top services"),
    }
}

pub async fn hardening_system_postures(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    match list_system_postures(&pool).await {
        Ok(rows) => {
            let payload = rows
                .into_iter()
                .map(map_posture)
                .collect::<Vec<HardeningSystemPostureResponse>>();
            (StatusCode::OK, Json(payload)).into_response()
        }
        Err(_) => internal_error("Failed to load system hardening posture"),
    }
}

pub async fn get_system_hardening(
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

    let environment_memberships = match get_user_environment_membership_ids(&pool, user_id).await {
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

    let posture = match get_system_posture(&pool, system_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load hardening posture"),
    };

    let Some(posture) = posture else {
        return (
            StatusCode::OK,
            Json(Vec::<HardeningServiceResultResponse>::new()),
        )
            .into_response();
    };

    let Some(scan_id) = posture.latest_scan_id else {
        return (
            StatusCode::OK,
            Json(Vec::<HardeningServiceResultResponse>::new()),
        )
            .into_response();
    };

    match get_service_results(&pool, scan_id).await {
        Ok(results) => {
            let payload = results
                .into_iter()
                .map(|item| HardeningServiceResultResponse {
                    id: item.id,
                    scan_id: item.scan_id,
                    service_name: item.service_name,
                    service_type: item.service_type,
                    hardening_score: item.hardening_score,
                    risk_level: risk_level_string(item.risk_level),
                    directives_detail: item.directives_detail,
                    enabled_directives_count: item.enabled_directives_count,
                    disabled_directives_count: item.disabled_directives_count,
                    missing_directives_count: item.missing_directives_count,
                })
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(payload)).into_response()
        }
        Err(_) => internal_error("Failed to load service hardening results"),
    }
}

pub async fn get_system_hardening_justifications(
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

    let environment_memberships = match get_user_environment_membership_ids(&pool, user_id).await {
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

    match get_justifications_for_system(&pool, system_id).await {
        Ok(items) => {
            let payload = items
                .into_iter()
                .map(|item| HardeningJustificationResponse {
                    id: item.id,
                    system_id: item.system_id,
                    service_name: item.service_name,
                    directive_name: item.directive_name,
                    category: item.category,
                    reason: item.reason,
                    created_at: item.created_at,
                    updated_at: item.updated_at,
                    expires_at: item.expires_at,
                })
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(payload)).into_response()
        }
        Err(_) => internal_error("Failed to load hardening justifications"),
    }
}

pub async fn save_hardening_justification(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((system_id, service_name)): Path<(Uuid, String)>,
    Json(payload): Json<SaveHardeningJustificationRequest>,
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

    let environment_memberships = match get_user_environment_membership_ids(&pool, user_id).await {
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

    let reason = payload.reason.trim();
    if reason.is_empty() {
        return bad_request("Justification reason is required");
    }

    if reason.len() > 2000 {
        return bad_request("Justification reason must be 2000 characters or less");
    }

    let service_name = service_name.trim();
    if service_name.is_empty() {
        return bad_request("Service name is required");
    }

    let directive_name = payload
        .directive_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let category = payload
        .category
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());

    match upsert_justification(
        &pool,
        system_id,
        service_name,
        directive_name,
        category,
        reason,
        Some(user_id),
    )
    .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(SystemMutationResponse {
                status: "ok".to_string(),
                message: "Hardening justification saved".to_string(),
            }),
        )
            .into_response(),
        Err(_) => internal_error("Failed to save hardening justification"),
    }
}

pub async fn get_system_hardening_scan_eligibility(
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

    let environment_memberships = match get_user_environment_membership_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(e) => {
            tracing::error!("Hardening scan eligibility: Failed to load environment memberships for user {}: {:?}", user_id, e);
            return internal_error("Failed to load environment memberships");
        }
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(e) => {
            tracing::error!("Hardening scan eligibility: Failed to load system {}: {:?}", system_id, e);
            return internal_error("Failed to load system");
        }
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let payload = match resolve_system_hardening_scan_target(&pool, system_id).await {
        Ok(Some(target)) => HardeningScanEligibilityResponse {
            eligible: target.blocked_reason.is_none(),
            reason: target.blocked_reason,
            derivation_id: Some(target.derivation_id),
            config_name: Some(target.config_name),
            hostname: Some(target.hostname),
        },
        Ok(None) => HardeningScanEligibilityResponse {
            eligible: false,
            reason: Some(
                "No eligible derivation was found for this system configuration.".to_string(),
            ),
            derivation_id: None,
            config_name: None,
            hostname: None,
        },
        Err(e) => {
            tracing::error!("Hardening scan eligibility: Failed to resolve scan target for system {}: {:?}", system_id, e);
            return internal_error("Failed to evaluate hardening scan eligibility");
        }
    };

    (StatusCode::OK, Json(payload)).into_response()
}

pub async fn trigger_system_hardening_scan_handler(
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

    let environment_memberships = match get_user_environment_membership_ids(&pool, user_id).await {
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

    let scan_id = match trigger_system_hardening_scan(pool.clone(), system_id).await {
        Ok(value) => value,
        Err(HardeningScanError::DerivationNotEligible(reason)) => {
            return (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "scan_ineligible".to_string(),
                    message: reason,
                    details: None,
                }),
            )
                .into_response();
        }
        Err(HardeningScanError::ScanAlreadyActive(scan_id)) => scan_id,
        Err(HardeningScanError::NixEvalFailed(message)) => {
            return internal_error(&format!("Failed to queue hardening scan: {message}"));
        }
        Err(HardeningScanError::Internal(err)) => {
            return internal_error(&format!("Failed to queue hardening scan: {err}"));
        }
    };

    (
        StatusCode::ACCEPTED,
        Json(HardeningScanTriggerResponse {
            scan_id,
            status: "accepted".to_string(),
            message: "Hardening scan queued".to_string(),
        }),
    )
        .into_response()
}

pub async fn get_hardening_scan_status(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(scan_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let scan = match get_scan_by_id(&pool, scan_id).await {
        Ok(Some(scan)) => scan,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: "Hardening scan not found".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
        Err(_) => return internal_error("Failed to load hardening scan status"),
    };

    if !matches!(caller_role, Role::Admin) {
        let environment_memberships = match get_user_environment_membership_ids(&pool, user_id).await {
            Ok(value) => value,
            Err(_) => return internal_error("Failed to load environment memberships"),
        };

        let environment_ids = match list_scan_environment_ids(&pool, scan_id).await {
            Ok(value) => value,
            Err(_) => return internal_error("Failed to evaluate scan access"),
        };

        let can_access = environment_ids
            .into_iter()
            .any(|environment_id| {
                caller_role.can_access_system_environment(environment_id, &environment_memberships)
            });

        if !can_access {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: "Hardening scan not found".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    }

    (
        StatusCode::OK,
        Json(HardeningScanStatusResponse {
            scan_id: scan.id,
            derivation_id: scan.derivation_id,
            status: scan.status.to_string(),
            error_message: scan
                .scan_metadata
                .as_ref()
                .and_then(|meta| meta.get("error"))
                .and_then(|value| value.as_str())
                .map(|value| value.to_string()),
            scheduled_at: scan.scheduled_at,
            started_at: scan.started_at,
            completed_at: scan.completed_at,
            attempts: scan.attempts,
            total_services: scan.total_services,
            overall_score: scan.overall_score,
        }),
    )
        .into_response()
}

fn map_posture(
    item: crate::hardening::types::SystemHardeningPosture,
) -> HardeningSystemPostureResponse {
    HardeningSystemPostureResponse {
        system_id: item.system_id,
        derivation_id: item.derivation_id,
        config_name: item.config_name,
        hostname: item.hostname,
        environment_name: item.environment_name,
        latest_scan_id: item.latest_scan_id,
        overall_score: item.overall_score,
        risk_level: item.risk_level.map(risk_level_string),
        total_services: item.total_services,
        well_hardened_count: item.well_hardened_count,
        moderately_hardened_count: item.moderately_hardened_count,
        poorly_hardened_count: item.poorly_hardened_count,
        vulnerable_count: item.vulnerable_count,
        last_scan_at: item.last_scan_at,
    }
}

fn risk_level_string(level: crate::hardening::types::RiskLevel) -> String {
    match level {
        crate::hardening::types::RiskLevel::WellHardened => "well_hardened".to_string(),
        crate::hardening::types::RiskLevel::ModeratelyHardened => "moderately_hardened".to_string(),
        crate::hardening::types::RiskLevel::PoorlyHardened => "poorly_hardened".to_string(),
        crate::hardening::types::RiskLevel::Vulnerable => "vulnerable".to_string(),
    }
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

fn forbidden_admin() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Admin privileges are required".to_string(),
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

fn not_found() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "not_found".to_string(),
            message: "Resource not found".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn bad_request(message: impl Into<String>) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "bad_request".to_string(),
            message: message.into(),
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
