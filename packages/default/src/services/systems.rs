//! Systems service layer.
//!
//! Provides use-case oriented functions for systems-related operations.

use crate::api::models::{
    CreateSystemRequest, DeploymentStatus, PaginatedResponse, SystemDetail, SystemSummary,
    SystemsListParams,
};
use crate::auth::models::Role;
use crate::models::auth_identity::AuthRole;
use crate::queries::systems as queries;
use anyhow::Result;
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

/// Filter options for systems list.
#[derive(Debug, Clone, Default)]
pub struct SystemsListFilter {
    /// Search filter (matches hostname)
    pub search: Option<String>,
    /// Environment name filter
    pub environment: Option<String>,
}

/// Sort options for systems list.
#[derive(Debug, Clone, Default)]
pub struct SystemsSort {
    pub field: SystemsSortField,
    pub descending: bool,
}

#[derive(Debug, Clone, Default)]
pub enum SystemsSortField {
    #[default]
    Hostname,
}

/// Pagination options.
#[derive(Debug, Clone)]
pub struct Pagination {
    pub page: u32,
    pub per_page: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 50,
        }
    }
}

impl Pagination {
    pub fn new(page: Option<u32>, per_page: Option<u32>) -> Self {
        Self {
            page: page.unwrap_or(1).max(1),
            per_page: per_page.unwrap_or(50).clamp(1, 200),
        }
    }

    pub fn offset(&self) -> u32 {
        (self.page.saturating_sub(1)) * self.per_page
    }
}

/// Context for a system list request, including auth and filtering.
pub struct SystemsListContext {
    pub user_id: Uuid,
    pub roles: Vec<AuthRole>,
    pub environment_ids: BTreeSet<Uuid>,
    pub filter: SystemsListFilter,
    pub sort: SystemsSort,
    pub pagination: Pagination,
}

impl SystemsListContext {
    /// Extract from request params and auth.
    pub fn new(
        user_id: Uuid,
        roles: Vec<AuthRole>,
        environment_ids: BTreeSet<Uuid>,
        params: &SystemsListParams,
    ) -> Self {
        Self {
            user_id,
            roles,
            environment_ids,
            filter: SystemsListFilter {
                search: params.search.clone(),
                environment: params.environment.clone(),
            },
            sort: SystemsSort {
                field: SystemsSortField::Hostname,
                descending: !matches!(params.sort_order, Some(crate::api::models::SortOrder::Asc)),
            },
            pagination: Pagination::new(
                params.page.map(|p| p as u32),
                params.per_page.map(|p| p as u32),
            ),
        }
    }

    /// Get the caller's highest role.
    pub fn caller_role(&self) -> Option<Role> {
        if self.roles.contains(&AuthRole::Admin) {
            Some(Role::Admin)
        } else if self.roles.contains(&AuthRole::Operator) {
            Some(Role::Operator)
        } else if self.roles.contains(&AuthRole::Viewer) {
            Some(Role::Viewer)
        } else {
            None
        }
    }

    /// Check if caller is admin (sees all systems).
    pub fn is_admin(&self) -> bool {
        matches!(self.caller_role(), Some(Role::Admin))
    }
}

/// List systems for a user with RBAC scoping and server-side filtering/sorting/pagination.
///
/// Returns a paginated list of system summaries.
pub async fn list_systems_for_user(
    pool: &PgPool,
    ctx: &SystemsListContext,
) -> Result<PaginatedResponse<SystemSummary>> {
    // Use the query layer for server-side filtering/sorting/pagination
    let is_admin = ctx.is_admin();
    let environment_ids: Vec<Uuid> = if is_admin {
        vec![]
    } else {
        ctx.environment_ids.iter().cloned().collect()
    };

    // Convert service pagination to query pagination
    let query_pagination = queries::Pagination {
        offset: ctx.pagination.offset(),
        limit: ctx.pagination.per_page,
    };

    // Convert filter types
    let query_filter = queries::SystemsListFilter {
        search: ctx.filter.search.clone(),
        environment: ctx.filter.environment.clone(),
    };

    let query_sort = queries::SystemsSort {
        field: match ctx.sort.field {
            SystemsSortField::Hostname => queries::SystemsSortField::Hostname,
        },
        descending: ctx.sort.descending,
    };

    let (items, total) = queries::list_systems_scoped(
        pool,
        is_admin,
        &environment_ids,
        &query_filter,
        &query_sort,
        &query_pagination,
    )
    .await?;

    let items = items.into_iter().map(list_row_to_summary).collect();

    Ok(PaginatedResponse {
        items,
        total: total as i64,
        page: ctx.pagination.page as i64,
        per_page: ctx.pagination.per_page as i64,
    })
}

/// Get a single system detail by ID, with RBAC scoping.
pub async fn get_system_detail(
    pool: &PgPool,
    system_id: Uuid,
    ctx: &SystemsListContext,
) -> Result<Option<SystemDetail>> {
    // Get system detail from query layer
    let row = match queries::get_system_detail_by_id(pool, system_id).await? {
        Some(row) => row,
        None => return Ok(None),
    };

    // For non-admins, check environment membership
    if !ctx.is_admin() {
        if let Some(env_name) = &row.environment {
            let env_ids = queries::get_environment_ids_by_names(pool, &[env_name.clone()]).await?;
            let has_access = env_ids.iter().any(|id| ctx.environment_ids.contains(id));
            if !has_access {
                return Ok(None);
            }
        }
    }

    Ok(Some(detail_row_to_api_model(row)))
}

/// Create a new system.
pub async fn create_system(
    pool: &PgPool,
    _user_id: Uuid,
    payload: CreateSystemRequest,
) -> Result<SystemDetail> {
    // Look up environment ID from name
    let environment_id = if let Some(env_name) = payload.environment.as_ref() {
        let env_name_trimmed = env_name.trim();
        if !env_name_trimmed.is_empty() {
            queries::get_environment_id_by_name(pool, env_name_trimmed).await?
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
            queries::get_flake_id_by_name(pool, flake_name_trimmed).await?
        } else {
            None
        }
    } else {
        None
    };

    // Validate required fields
    let hostname = payload.hostname.trim();
    if hostname.is_empty() {
        anyhow::bail!("Hostname is required");
    }

    let public_key = payload.public_key.trim();
    if public_key.is_empty() {
        anyhow::bail!("Public key is required");
    }

    // Validate deployment policy
    if !matches!(
        payload.deployment_policy.as_str(),
        "manual" | "auto_latest" | "pinned"
    ) {
        anyhow::bail!("Invalid deployment policy (must be: manual, auto_latest, or pinned)");
    }

    // Use the System model to create and validate the system
    use crate::models::systems::System;
    let system = System::new(
        pool,
        hostname.to_string(),
        environment_id,
        true, // is_active
        public_key.to_string(),
        flake_id,
        None, // desired_target
        payload.deployment_policy.clone(),
    )
    .await?;

    // Fetch the created system from view to return complete data
    let detail = queries::get_system_detail_by_id(pool, system.id)
        .await?
        .map(detail_row_to_api_model)
        .ok_or_else(|| anyhow::anyhow!("System created but not found in view"))?;

    Ok(detail)
}

// === Helper functions ===

fn list_row_to_summary(row: queries::SystemListRow) -> SystemSummary {
    use crate::api::models::{CveSummary, DeploymentStatus, HealthStatus, PipelineStage};

    SystemSummary {
        id: row.id,
        hostname: row.hostname,
        environment: row.environment,
        flake_id: row.flake_id,
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

fn detail_row_to_api_model(row: queries::SystemDetailRow) -> SystemDetail {
    use crate::api::models::{
        CveSummary, FlakeSummary, SystemHardwareInfo, SystemNetworkInfo, SystemSecurityInfo,
    };

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

fn parse_pipeline_stage(stage: &str) -> crate::api::models::PipelineStage {
    match stage {
        "dry_run" => crate::api::models::PipelineStage::DryRun,
        "ready_for_build" => crate::api::models::PipelineStage::ReadyForBuild,
        "building" => crate::api::models::PipelineStage::Building,
        "build_complete" => crate::api::models::PipelineStage::BuildComplete,
        "ready_for_deploy" => crate::api::models::PipelineStage::ReadyForDeploy,
        _ => crate::api::models::PipelineStage::Unknown,
    }
}
