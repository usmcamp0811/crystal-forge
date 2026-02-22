use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::api::models::{
    ApiError, AuditAction, CveSummary, DeploymentStatus, PaginatedResponse, PipelineStage, SortOrder,
    SystemMutationResponse, SystemRollbackRequest,
    SystemDetail, SystemHardwareInfo, SystemNetworkInfo, SystemSecurityInfo, SystemSummary,
    SystemsListParams,
};
use crate::auth::models::Role;
use crate::handlers::api::rbac::{authenticated_user_roles, extract_request_origin};
use crate::models::auth_identity::AuthRole;
use crate::queries::systems::{
    find_system_access_row, get_user_environment_membership_ids, list_system_access_rows,
    touch_system_updated_at, update_system_desired_target, SystemAccessRow,
};

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
    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let rows = match list_system_access_rows(&pool).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load systems"),
    };

    let mut items = rows
        .into_iter()
        .filter(|row| caller_role.can_access_system_environment(row.environment_id, &environment_memberships))
        .filter(|row| matches_filters(row, &params))
        .map(row_to_summary)
        .collect::<Vec<_>>();

    sort_items(&mut items, params.sort_order);

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(50).clamp(1, 200);
    let total = items.len() as i64;
    let start = ((page - 1) * per_page) as usize;
    let paged_items = if start >= items.len() {
        vec![]
    } else {
        items
            .into_iter()
            .skip(start)
            .take(per_page as usize)
            .collect::<Vec<_>>()
    };

    (
        StatusCode::OK,
        Json(PaginatedResponse {
            items: paged_items,
            total,
            page,
            per_page,
        }),
    )
        .into_response()
}

pub async fn get_system(
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

    let detail = SystemDetail {
        id: row.id,
        hostname: row.hostname,
        environment: row.environment,
        is_active: row.is_active,
        deployment_policy: row.deployment_policy,
        health_status: crate::api::models::HealthStatus::Offline,
        deployment_status: DeploymentStatus::Unknown,
        pipeline_stage: Some(PipelineStage::Unknown),
        nixos_version: None,
        kernel: None,
        agent_version: None,
        current_store_path: None,
        hardware: SystemHardwareInfo {
            cpu_brand: None,
            cpu_cores: None,
            memory_gb: None,
            uptime_secs: None,
            board_serial: None,
            bios_version: None,
        },
        network: SystemNetworkInfo {
            primary_ip: None,
            primary_mac: None,
            gateway_ip: None,
        },
        security: SystemSecurityInfo {
            tpm_present: None,
            secure_boot_enabled: None,
            fips_mode: None,
            selinux_status: None,
        },
        cve_counts: CveSummary {
            critical: 0,
            high: 0,
            medium: 0,
            low: 0,
        },
        flake: None,
        last_seen: None,
        created_at: row.created_at,
        updated_at: row.updated_at,
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
            let env_name = row.environment.clone().unwrap_or_default().to_ascii_lowercase();
            if env_name != needle {
                return false;
            }
        }
    }

    true
}

fn sort_items(items: &mut [SystemSummary], sort_order: Option<SortOrder>) {
    let descending = !matches!(sort_order, Some(SortOrder::Asc));
    items.sort_by(|left, right| left.hostname.cmp(&right.hostname));
    if descending {
        items.reverse();
    }
}

fn row_to_summary(row: SystemAccessRow) -> SystemSummary {
    SystemSummary {
        id: row.id,
        hostname: row.hostname,
        environment: row.environment,
        health_status: crate::api::models::HealthStatus::Offline,
        deployment_status: DeploymentStatus::Unknown,
        pipeline_stage: Some(PipelineStage::Unknown),
        cve_counts: CveSummary {
            critical: 0,
            high: 0,
            medium: 0,
            low: 0,
        },
        nixos_version: None,
        last_seen: None,
        deployment_policy: row.deployment_policy,
    }
}

async fn load_membership_environment_ids(pool: &PgPool, user_id: Uuid) -> Result<BTreeSet<Uuid>, ()> {
    get_user_environment_membership_ids(pool, user_id).await.map_err(|_| ())
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

fn action_to_str(action: AuditAction) -> &'static str {
    match action {
        AuditAction::UserCreated => "user_created",
        AuditAction::UserUpdated => "user_updated",
        AuditAction::UserEnabled => "user_enabled",
        AuditAction::UserDisabled => "user_disabled",
        AuditAction::UserRoleAssigned => "user_role_assigned",
        AuditAction::UserEnvironmentMembershipUpdated => "user_environment_membership_updated",
        AuditAction::OidcMappingChanged => "oidc_mapping_changed",
        AuditAction::SystemSyncRequested => "system_sync_requested",
        AuditAction::SystemRollbackRequested => "system_rollback_requested",
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

    #[test]
    fn rollback_target_commit_validation_enforces_bounds_and_format() {
        assert!(validate_target_commit("").is_err());
        assert!(validate_target_commit("abc").is_err());
        assert!(validate_target_commit("zzzzzzz").is_err());
        assert!(validate_target_commit("a1b2c3d").is_ok());
        assert!(validate_target_commit(&"a".repeat(65)).is_err());
    }
}
