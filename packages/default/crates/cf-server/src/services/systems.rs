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
    /// Matches one exact operator label before pagination.
    pub tag: Option<String>,
    /// Restricts authority before pagination and totals.
    pub management_mode: Option<crate::models::systems::ManagementMode>,
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
                tag: params.tag.clone(),
                management_mode: params.management_mode,
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
    server_default_interval: u64,
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
        tag: ctx.filter.tag.clone(),
        management_mode: ctx.filter.management_mode,
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

    let items = items
        .into_iter()
        .map(|row| list_row_to_summary(row, server_default_interval))
        .collect();

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
    server_default_interval: u64,
) -> Result<Option<SystemDetail>> {
    // Get system detail from query layer
    let row = match queries::get_system_detail_by_id(pool, system_id).await? {
        Some(row) => row,
        None => return Ok(None),
    };

    // Unassigned resources are administrator-only, including on detail reads.
    if !ctx.is_admin() {
        if row.environment.is_none() {
            return Ok(None);
        }
        if let Some(env_name) = &row.environment {
            let env_ids = queries::get_environment_ids_by_names(pool, &[env_name.clone()]).await?;
            let has_access = env_ids.iter().any(|id| ctx.environment_ids.contains(id));
            if !has_access {
                return Ok(None);
            }
        }
    }

    Ok(Some(detail_row_to_api_model(row, server_default_interval)))
}

/// Registers an insert-only resource and its audit in one transaction.
///
/// Revalidates actor scope and the optional parent under environment locks.
/// Registration never overwrites a resource with the same name.
/// # Errors
/// Returns validation, authorization, name-conflict, or persistence errors.
pub async fn create_system(
    pool: &PgPool,
    user_id: Uuid,
    payload: CreateSystemRequest,
    server_default_interval: u64,
) -> Result<SystemDetail> {
    validate_registration(&payload)?;
    // Look up environment ID from name
    let environment_id = if let Some(env_name) = payload.environment.as_ref() {
        let env_name_trimmed = env_name.trim();
        if !env_name_trimmed.is_empty() {
            Some(
                queries::get_environment_id_by_name(pool, env_name_trimmed)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("Environment not found"))?,
            )
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
            Some(
                queries::get_flake_id_by_name(pool, flake_name_trimmed)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("Flake not found"))?,
            )
        } else {
            None
        }
    } else {
        None
    };

    let hostname = payload.hostname.trim();

    let mut tx = pool.begin().await?;
    if let Some(environment_id) = environment_id {
        sqlx::query("SELECT id FROM environments WHERE id=$1 FOR UPDATE")
            .bind(environment_id)
            .execute(&mut *tx)
            .await?;
    }
    queries::authorize_system_write_tx(&mut tx, user_id, &[environment_id]).await?;
    queries::validate_parent_host_tx(
        &mut tx,
        Uuid::nil(),
        payload.management_mode,
        payload.parent_host_id,
        environment_id,
    )
    .await?;
    // PERSISTENCE: Registration is insert-only. A name conflict must never
    // overwrite another resource's credentials, mode, or configuration.
    let id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO systems (hostname, environment_id, is_active, public_key,
             flake_id, system_configuration_name, deployment_policy,
             management_mode, parent_host_id, derivation, tags, description, fqdn, reachability)
         VALUES ($1,$2,true,$3,$4,$5,$6,$7,$8,'',$9,$10,$11,$12) RETURNING id",
    )
    .bind(hostname)
    .bind(environment_id)
    .bind(payload.public_key.as_deref().map(str::trim))
    .bind(flake_id)
    .bind(payload.system_configuration_name.as_deref().map(str::trim))
    .bind(&payload.deployment_policy)
    .bind(payload.management_mode)
    .bind(payload.parent_host_id)
    .bind(normalize_tags(&payload.tags))
    .bind(normalize_optional_text(payload.description.as_deref()))
    .bind(if payload.management_mode.permits_deployment() {
        normalize_optional_text(payload.fqdn.as_deref())
    } else {
        None
    })
    .bind(if payload.management_mode.permits_deployment() {
        payload.reachability.trim()
    } else {
        "direct"
    })
    .fetch_one(&mut *tx)
    .await?;
    queries::audit_system_metadata_tx(
        &mut tx,
        user_id,
        id,
        "system_created",
        serde_json::json!({"management_mode": payload.management_mode,
            "parent_host_id": payload.parent_host_id}),
    )
    .await?;

    // Fetch the created system from view to return complete data
    let detail = queries::get_system_detail_by_id(&mut *tx, id)
        .await?
        .map(|row| detail_row_to_api_model(row, server_default_interval))
        .ok_or_else(|| anyhow::anyhow!("System created but not found in view"))?;

    tx.commit().await?;
    Ok(detail)
}

/// Validates registration authority without accessing persistence.
///
/// Reference names are descriptive resource identities, not DNS hostnames.
/// # Errors
/// Returns an error for missing identity, invalid policy or enrollment, or a
/// configuration-only registration without an explicit flake/configuration.
pub fn validate_registration(payload: &CreateSystemRequest) -> Result<()> {
    use crate::models::{public_key::PublicKey, systems::ManagementMode};
    validate_reachability(&payload.reachability)?;
    anyhow::ensure!(
        !payload.hostname.trim().is_empty(),
        "Resource name is required"
    );
    payload
        .deployment_policy
        .parse::<crate::models::systems::DeploymentPolicy>()?;
    if payload.management_mode == ManagementMode::Managed {
        PublicKey::from_base64(
            payload.public_key.as_deref().unwrap_or("").trim(),
            &payload.hostname,
        )?;
    } else {
        anyhow::ensure!(
            payload.public_key.is_none(),
            "Nonmanaged resources cannot have an agent key"
        );
        anyhow::ensure!(
            payload
                .flake_name
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty()),
            "Nonmanaged resources require a flake"
        );
        anyhow::ensure!(
            payload
                .system_configuration_name
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty()),
            "Nonmanaged resources require an explicit system_configuration_name"
        );
    }
    anyhow::ensure!(
        payload.parent_host_id.is_none() || payload.management_mode == ManagementMode::External,
        "Only external resources can have a parent host"
    );
    Ok(())
}

// === Helper functions ===

/// Returns trimmed, nonempty, case-sensitive labels in first-occurrence order.
pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for tag in tags
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
    {
        if !result.iter().any(|existing| existing == tag) {
            result.push(tag.to_string());
        }
    }
    result
}

/// Returns trimmed operator text, treating blank input as absent.
pub fn normalize_optional_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Validates the existing reachability vocabulary without changing transport.
/// # Errors
/// Returns an error unless the trimmed value is `direct` or `pull`.
pub fn validate_reachability(value: &str) -> Result<()> {
    anyhow::ensure!(
        matches!(value.trim(), "direct" | "pull"),
        "Reachability must be direct or pull"
    );
    Ok(())
}

/// Validates an explicit authority change and returns canonical new credentials.
///
/// Omitted mode preserves authority. Initial managed conversion uses manual
/// policy so enrollment cannot itself trigger an automatic deployment.
/// # Errors
/// Returns an error for implicit credential replacement, invalid enrollment, or
/// a managed conversion that did not explicitly select manual policy.
pub fn validate_management_update(
    old: crate::models::systems::ManagementMode,
    payload: &crate::api::models::UpdateSystemRequest,
) -> Result<Option<String>> {
    use crate::models::{public_key::PublicKey, systems::ManagementMode};
    let mode = payload.management_mode.unwrap_or(old);
    if mode != ManagementMode::Managed {
        anyhow::ensure!(
            payload.public_key.is_none(),
            "Nonmanaged resources cannot have an agent key"
        );
        return Ok(None);
    }
    if old != ManagementMode::Managed {
        anyhow::ensure!(
            payload.management_mode == Some(ManagementMode::Managed)
                && payload.deployment_policy == "manual",
            "Conversion into managed requires explicit managed mode and manual policy; enable auto_latest or pinned in a later edit"
        );
        anyhow::ensure!(
            payload.public_key.is_some(),
            "Conversion into managed requires a new enrollment key"
        );
    }
    if let Some(key) = payload.public_key.as_deref() {
        anyhow::ensure!(
            payload.management_mode == Some(ManagementMode::Managed),
            "Enrollment key requires an explicit managed-mode request"
        );
        return PublicKey::from_base64(key.trim(), &payload.hostname)
            .map(|key| Some(key.to_base64()));
    }
    Ok(None)
}

fn list_row_to_summary(row: queries::SystemListRow, server_default_interval: u64) -> SystemSummary {
    use crate::api::models::{CveSummary, DeploymentStatus, HealthStatus, PipelineStage};

    let effective_heartbeat_interval_secs = row
        .heartbeat_interval_secs
        .map(|v| v as i32)
        .unwrap_or(server_default_interval as i32);

    SystemSummary {
        tags: row.tags,
        description: row.description,
        management_mode: row.management_mode,
        parent_host_id: row.parent_host_id,
        id: row.id,
        hostname: row.hostname,
        system_configuration_name: row.system_configuration_name,
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
        fqdn: row.fqdn,
        heartbeat_interval_secs: row.heartbeat_interval_secs,
        effective_heartbeat_interval_secs,
        // A retained boot identity is history after authority is revoked.
        boot_id: if row.management_mode == crate::models::systems::ManagementMode::Managed {
            row.boot_id
        } else {
            None
        },
    }
}

/// Clears current-runtime projections for assurance-only registrations.
///
/// Applies equally to registration and read/update responses. This changes only
/// the response row; retained agent and scan history stays in the database.
pub(crate) fn mask_nonmanaged_runtime(row: &mut queries::SystemDetailRow) {
    use crate::models::systems::ManagementMode;
    if row.management_mode == ManagementMode::Managed {
        return;
    }
    row.health_status = "unknown".into();
    row.deployment_status = "unknown".into();
    row.pipeline_stage = "unknown".into();
    row.nixos_version = None;
    row.kernel = None;
    row.agent_version = None;
    row.current_store_path = None;
    row.generation = None;
    row.generation_matches_current_store_path = None;
    row.cpu_brand = None;
    row.cpu_cores = None;
    row.memory_gb = None;
    row.uptime_secs = None;
    row.board_serial = None;
    row.bios_version = None;
    row.primary_ip_address = None;
    row.primary_mac_address = None;
    row.gateway_ip = None;
    row.tpm_present = None;
    row.secure_boot_enabled = None;
    row.fips_mode = None;
    row.selinux_status = None;
    row.last_seen = None;
    row.boot_id = None;
    row.last_restart_type = None;
    row.last_restart_at = None;
    row.critical_cve_count = 0;
    row.high_cve_count = 0;
    row.medium_cve_count = 0;
    row.low_cve_count = 0;
}

fn detail_row_to_api_model(
    mut row: queries::SystemDetailRow,
    server_default_interval: u64,
) -> SystemDetail {
    use crate::api::models::{
        CveSummary, FlakeSummary, SystemHardwareInfo, SystemNetworkInfo, SystemSecurityInfo,
    };

    mask_nonmanaged_runtime(&mut row);

    let effective_heartbeat_interval_secs = row
        .heartbeat_interval_secs
        .map(|v| v as i32)
        .unwrap_or(server_default_interval as i32);

    let public_key_fingerprint = crate::handlers::api::systems::public_key_fingerprint(
        row.public_key.as_deref(),
        &row.hostname,
    );

    SystemDetail {
        tags: row.tags,
        description: row.description,
        management_transition: None,
        management_mode: row.management_mode,
        parent_host_id: row.parent_host_id,
        id: row.id,
        hostname: row.hostname,
        fqdn: row.fqdn,
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
        generation: row.generation,
        generation_matches_current_store_path: row.generation_matches_current_store_path,
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
            reachability: row.reachability,
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
        heartbeat_interval_secs: row.heartbeat_interval_secs,
        effective_heartbeat_interval_secs,
        boot_id: row.boot_id,
        restart_type: row.last_restart_type,
        last_restart_at: row.last_restart_at,
        public_key_fingerprint,
    }
}

// Nonmanaged view rows use `unknown`; no missing health can prove an outage.
fn parse_health_status(status: &str) -> crate::api::models::HealthStatus {
    match status {
        "healthy" => crate::api::models::HealthStatus::Healthy,
        "warning" => crate::api::models::HealthStatus::Warning,
        "critical" => crate::api::models::HealthStatus::Critical,
        "offline" => crate::api::models::HealthStatus::Offline,
        _ => crate::api::models::HealthStatus::Unknown,
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

#[cfg(test)]
mod management_mode_tests {
    use super::*;
    use crate::api::models::{FieldUpdate, UpdateSystemRequest};
    use crate::models::{public_key::PublicKey, systems::ManagementMode};

    #[test]
    fn management_mode_metadata_normalization_and_reachability_validation() {
        assert_eq!(
            normalize_tags(&[
                " web ".into(),
                "".into(),
                "web".into(),
                "Web".into(),
                " edge ".into()
            ]),
            ["web", "Web", "edge"]
        );
        assert_eq!(normalize_optional_text(Some(" notes \n")), Some("notes"));
        assert_eq!(normalize_optional_text(Some(" \n")), None);
        for valid in ["direct", "pull", " pull "] {
            assert!(validate_reachability(valid).is_ok());
        }
        for invalid in ["", "ssh", "Direct", "inherit"] {
            assert!(validate_reachability(invalid).is_err());
        }
    }

    #[test]
    fn management_mode_metadata_wire_defaults_and_tristate() {
        let legacy = serde_json::json!({"hostname":"host", "deployment_policy":"manual"});
        let create: CreateSystemRequest = serde_json::from_value(legacy.clone()).unwrap();
        assert!(create.tags.is_empty());
        assert!(create.description.is_none());
        assert!(create.fqdn.is_none());
        assert_eq!(create.reachability, "direct");
        let update: UpdateSystemRequest = serde_json::from_value(legacy.clone()).unwrap();
        let wire = serde_json::to_value(update).unwrap();
        for field in ["tags", "description", "fqdn", "reachability"] {
            assert!(wire.get(field).is_none(), "{field}");
        }
        for field in ["tags", "description", "fqdn", "reachability"] {
            let mut clear = legacy.clone();
            clear[field] = serde_json::Value::Null;
            let update: UpdateSystemRequest = serde_json::from_value(clear).unwrap();
            assert_eq!(
                serde_json::to_value(update).unwrap()[field],
                serde_json::Value::Null
            );
        }
        let update: UpdateSystemRequest = serde_json::from_value(serde_json::json!({
            "hostname":"host", "deployment_policy":"manual", "tags":[], "description":"", "reachability":"pull", "fqdn":"192.0.2.1"
        })).unwrap();
        assert_eq!(update.tags, FieldUpdate::Set(Vec::new()));
        assert_eq!(update.description, FieldUpdate::Set(String::new()));
        assert_eq!(update.reachability, FieldUpdate::Set("pull".into()));
        assert_eq!(update.fqdn, FieldUpdate::Set("192.0.2.1".into()));
    }

    #[test]
    fn health_status_mapping_preserves_unknown_and_managed_categories() {
        use crate::api::models::HealthStatus;
        for (wire, expected) in [
            ("healthy", HealthStatus::Healthy),
            ("warning", HealthStatus::Warning),
            ("critical", HealthStatus::Critical),
            ("offline", HealthStatus::Offline),
            ("unknown", HealthStatus::Unknown),
            ("", HealthStatus::Unknown),
        ] {
            assert_eq!(parse_health_status(wire), expected);
        }
    }

    fn enrollment_key() -> String {
        PublicKey::from_verifying_key(
            ed25519_dalek::SigningKey::from_bytes(&[17; 32]).verifying_key(),
        )
        .to_base64()
    }

    #[test]
    fn management_mode_legacy_create_requires_real_enrollment() {
        let mut request: CreateSystemRequest = serde_json::from_value(serde_json::json!({
            "hostname": "managed-host", "public_key": enrollment_key(),
            "deployment_policy": "manual",
        }))
        .unwrap();
        assert_eq!(request.management_mode, ManagementMode::Managed);
        assert!(validate_registration(&request).is_ok());
        request.public_key = None;
        assert!(validate_registration(&request).is_err());
        request.public_key = Some("invalid".into());
        assert!(validate_registration(&request).is_err());
    }

    #[test]
    fn management_mode_keyless_registration_requires_explicit_configuration() {
        for mode in [ManagementMode::External, ManagementMode::Reference] {
            let mut request: CreateSystemRequest = serde_json::from_value(serde_json::json!({
                "hostname": "Golden workstation / ISO baseline", "management_mode": mode,
                "flake_name": "fleet", "system_configuration_name": "workstation",
                "deployment_policy": "manual",
            }))
            .unwrap();
            assert!(validate_registration(&request).is_ok());
            request.public_key = Some(enrollment_key());
            assert!(validate_registration(&request).is_err());
            request.public_key = Some(String::new());
            assert!(validate_registration(&request).is_err());
            request.public_key = None;
            request.system_configuration_name = Some("  ".into());
            assert!(validate_registration(&request).is_err());
            request.system_configuration_name = Some("workstation".into());
            request.flake_name = None;
            assert!(validate_registration(&request).is_err());
        }
    }

    #[test]
    fn management_mode_edit_omission_never_promotes_or_reenrolls() {
        let mut request: UpdateSystemRequest = serde_json::from_value(serde_json::json!({
            "hostname": "reference", "deployment_policy": "auto_latest",
        }))
        .unwrap();
        assert_eq!(request.management_mode, None);
        assert_eq!(request.parent_host_id, FieldUpdate::Unset);
        assert_eq!(
            validate_management_update(ManagementMode::Reference, &request).unwrap(),
            None
        );
        request.public_key = Some(enrollment_key());
        assert!(validate_management_update(ManagementMode::Reference, &request).is_err());
        request.management_mode = Some(ManagementMode::Managed);
        assert!(validate_management_update(ManagementMode::Reference, &request).is_err());
        request.deployment_policy = "manual".into();
        assert_eq!(
            validate_management_update(ManagementMode::Reference, &request).unwrap(),
            Some(enrollment_key())
        );
        request.public_key = None;
        assert!(validate_management_update(ManagementMode::External, &request).is_err());
        assert!(validate_management_update(ManagementMode::Managed, &request).is_ok());
    }

    #[test]
    fn management_mode_parent_patch_preserves_clear_and_set() {
        for (value, expected) in [
            (serde_json::Value::Null, FieldUpdate::Clear),
            (
                serde_json::json!(Uuid::nil()),
                FieldUpdate::Set(Uuid::nil()),
            ),
        ] {
            let request: UpdateSystemRequest = serde_json::from_value(serde_json::json!({
                "hostname": "external", "deployment_policy": "manual", "parent_host_id": value,
            }))
            .unwrap();
            assert_eq!(request.parent_host_id, expected);
        }
        assert!(
            serde_json::from_value::<CreateSystemRequest>(serde_json::json!({
                "hostname": "bad", "management_mode": "unknown", "deployment_policy": "manual",
            }))
            .is_err()
        );
    }
}
