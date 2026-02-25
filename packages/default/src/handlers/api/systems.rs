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
    ApiError, AuditAction, CreateSystemRequest, CveSummary, DeploymentStatus, PipelineStage,
    SortOrder, SystemDetail, SystemHardwareInfo, SystemMutationResponse, SystemNetworkInfo,
    SystemRollbackRequest, SystemSecurityInfo, SystemSummary, SystemsListParams,
    UpdateSystemPublicKeyRequest,
};
use crate::auth::models::Role;
use crate::handlers::api::rbac::{authenticated_user_roles, extract_request_origin};
use crate::models::auth_identity::AuthRole;
use crate::queries::systems::{
    SystemAccessRow, SystemDetailRow, SystemListRow, deactivate_system, find_system_access_row,
    get_system_detail_by_id, get_user_environment_membership_ids, list_system_access_rows,
    touch_system_updated_at, update_public_key, update_system_desired_target,
};
use crate::services::systems::SystemsListContext;

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

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };
    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
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

fn list_row_to_summary(row: SystemListRow) -> SystemSummary {
    SystemSummary {
        id: row.id,
        hostname: row.hostname,
        environment: row.environment,
        health_status: parse_health_status(&row.health_status),
        deployment_status: parse_deployment_status(&row.deployment_status),
        pipeline_stage: Some(parse_pipeline_stage(&row.pipeline_stage)),
        cve_counts: CveSummary {
            critical: row.critical_cve_count as i64,
            high: row.high_cve_count as i64,
            medium: row.medium_cve_count as i64,
            low: row.low_cve_count as i64,
        },
        nixos_version: row.nixos_version,
        last_seen: row.last_seen,
        deployment_policy: row.deployment_policy,
    }
}

fn matches_filters_on_list_row(row: &SystemListRow, params: &SystemsListParams) -> bool {
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
