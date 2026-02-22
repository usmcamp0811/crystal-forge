use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::api::models::{
    ApiError, CveSummary, DeploymentStatus, PaginatedResponse, PipelineStage, SortOrder,
    SystemDetail, SystemHardwareInfo, SystemNetworkInfo, SystemSecurityInfo, SystemSummary,
    SystemsListParams,
};
use crate::auth::models::Role;
use crate::handlers::api::rbac::authenticated_user_roles;
use crate::models::auth_identity::AuthRole;

#[derive(Debug, sqlx::FromRow)]
struct SystemListRow {
    id: Uuid,
    hostname: String,
    environment_id: Option<Uuid>,
    environment: Option<String>,
    is_active: bool,
    deployment_policy: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub async fn list_systems(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<SystemsListParams>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let caller_role = highest_role(&roles);
    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let rows = match sqlx::query_as::<_, SystemListRow>(
        "SELECT s.id,
                s.hostname,
                s.environment_id,
                e.name AS environment,
                s.is_active,
                s.deployment_policy,
                s.created_at,
                s.updated_at
         FROM systems s
         LEFT JOIN environments e ON e.id = s.environment_id",
    )
    .fetch_all(&pool)
    .await
    {
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

    let caller_role = highest_role(&roles);
    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match sqlx::query_as::<_, SystemListRow>(
        "SELECT s.id,
                s.hostname,
                s.environment_id,
                e.name AS environment,
                s.is_active,
                s.deployment_policy,
                s.created_at,
                s.updated_at
         FROM systems s
         LEFT JOIN environments e ON e.id = s.environment_id
         WHERE s.id = $1",
    )
    .bind(system_id)
    .fetch_optional(&pool)
    .await
    {
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

fn highest_role(roles: &[AuthRole]) -> Role {
    if roles.contains(&AuthRole::Admin) {
        Role::Admin
    } else if roles.contains(&AuthRole::Operator) {
        Role::Operator
    } else {
        Role::Viewer
    }
}

fn matches_filters(row: &SystemListRow, params: &SystemsListParams) -> bool {
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

fn row_to_summary(row: SystemListRow) -> SystemSummary {
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
    let values = sqlx::query_scalar::<_, Uuid>(
        "SELECT environment_id FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ())?;

    Ok(values.into_iter().collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use sqlx::postgres::PgPoolOptions;

    #[test]
    fn highest_role_prefers_admin_then_operator_then_viewer() {
        assert_eq!(highest_role(&[AuthRole::Admin]), Role::Admin);
        assert_eq!(highest_role(&[AuthRole::Operator]), Role::Operator);
        assert_eq!(highest_role(&[AuthRole::Viewer]), Role::Viewer);
    }

    #[test]
    fn matches_filters_checks_search_and_environment() {
        let row = SystemListRow {
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
}
