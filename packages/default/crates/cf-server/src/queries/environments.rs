use crate::api::models::{
    DeploymentPolicySummary, EnvironmentCacheSummary, EnvironmentComplianceSummary,
    EnvironmentPolicyMapEntry, EnvironmentRollup, EnvironmentSummary, EnvironmentWithPolicies,
};
use crate::config::EnvironmentConfig;
use crate::models::environments::Environment;
use anyhow::Result;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Row type for list queries
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, FromRow)]
pub struct EnvironmentRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
    pub is_active: bool,
    pub system_count: i64,
    pub active_system_count: i64,
    pub healthy_count: i64,
    pub warning_count: i64,
    pub critical_count: i64,
    pub offline_count: i64,
    pub cve_critical_high_count: i64,
    pub flake_names: Vec<String>,
    pub default_policy: String,
    pub auto_sync: bool,
    pub requires_approval: bool,
    pub is_production: bool,
    pub role_assignment_count: i64,
    pub cache_name: Option<String>,
    pub cache_url: Option<String>,
    pub cache_type: Option<String>,
    pub cache_enabled: Option<bool>,
    pub compliance_bundle_id: Option<Uuid>,
    pub compliance_bundle_name: Option<String>,
    pub compliance_bundle_framework: Option<String>,
}

fn environment_summary_from_row(r: EnvironmentRow) -> EnvironmentSummary {
    EnvironmentSummary {
        id: r.id,
        name: r.name,
        description: r.description,
        color_hex: r.color_hex,
        is_active: r.is_active,
        system_count: r.system_count,
        rollup: EnvironmentRollup {
            active_system_count: r.active_system_count,
            healthy: r.healthy_count,
            warning: r.warning_count,
            critical: r.critical_count,
            offline: r.offline_count,
            cve_critical_high: r.cve_critical_high_count,
            flakes: r.flake_names,
        },
        default_policy: Some(r.default_policy),
        auto_sync: Some(r.auto_sync),
        requires_approval: Some(r.requires_approval),
        is_production: Some(r.is_production),
        role_assignment_count: Some(r.role_assignment_count),
        cache: r.cache_url.map(|url| EnvironmentCacheSummary {
            name: r.cache_name.unwrap_or_else(|| "cache".to_string()),
            url,
            cache_type: r.cache_type.unwrap_or_else(|| "unknown".to_string()),
            // Reflects the cache destination's administrative enabled/disabled
            // state (`cache_destinations.enabled`). This is not a live
            // reachability/health probe — no such check exists yet — so we
            // report the one truthful signal we have rather than fabricating
            // "healthy".
            status: if r.cache_enabled.unwrap_or(false) {
                "enabled".to_string()
            } else {
                "disabled".to_string()
            },
        }),
        compliance_bundle: r
            .compliance_bundle_id
            .map(|id| EnvironmentComplianceSummary {
                id,
                name: r
                    .compliance_bundle_name
                    .unwrap_or_else(|| "Compliance bundle".to_string()),
                framework: r
                    .compliance_bundle_framework
                    .unwrap_or_else(|| "unknown".to_string()),
            }),
    }
}

/// Fetch the environment record associated with this system
pub async fn get_environment(pool: &PgPool, id: Uuid) -> Result<Option<Environment>> {
    let env = sqlx::query_as::<_, Environment>("SELECT * FROM environments WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(env)
}

pub async fn get_environment_id_by_name(pool: &PgPool, name: &str) -> Result<Option<Uuid>> {
    let env_id = sqlx::query_scalar!("SELECT id FROM environments WHERE name = $1", name)
        .fetch_optional(pool)
        .await?;
    Ok(env_id)
}

pub async fn get_or_insert_environment_id_by_config(
    pool: &PgPool,
    env_config: &EnvironmentConfig,
) -> Result<Uuid> {
    // First try to get existing by name
    if let Some(id) = get_environment_id_by_name(pool, &env_config.name).await? {
        return Ok(id);
    }

    // Look up the compliance level ID
    let compliance_level_id = sqlx::query_scalar!(
        "SELECT id FROM compliance_levels WHERE name = $1",
        env_config.compliance_level
    )
    .fetch_optional(pool)
    .await?;

    // Look up the risk profile ID
    let risk_profile_id = sqlx::query_scalar!(
        "SELECT id FROM risk_profiles WHERE name = $1",
        env_config.risk_profile
    )
    .fetch_optional(pool)
    .await?;

    // Insert with the foreign key IDs
    let id = sqlx::query_scalar!(
        "INSERT INTO environments (name, description, is_active, compliance_level_id, risk_profile_id) 
         VALUES ($1, $2, $3, $4, $5) 
         RETURNING id",
        env_config.name,
        env_config.description,
        env_config.is_active,
        compliance_level_id,
        risk_profile_id
    )
    .fetch_one(pool)
    .await?;

    Ok(id)
}

// ─────────────────────────────────────────────────────────────────────────────
// API-facing list/get queries (RBAC scoped)
// ─────────────────────────────────────────────────────────────────────────────

/// List all environments visible to a given user.
///
/// Admins see every environment.
/// Non-admins see only environments they are a member of via
/// `user_environment_memberships`.
///
/// Returns rows ordered by name.
pub async fn list_environments_for_user(
    pool: &PgPool,
    user_id: Option<Uuid>,
) -> Result<Vec<EnvironmentSummary>> {
    let rows = if user_id.is_none() {
        // Admin path: no membership filter
        sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT
                e.id,
                e.name,
                e.description,
                COALESCE(e.color_hex, '#6B7280') AS color_hex,
                COALESCE(e.is_active, TRUE) AS is_active,
                COUNT(s.id)::bigint AS system_count,
                COALESCE(er.active_system_count, 0)::bigint AS active_system_count,
                COALESCE(er.healthy_count, 0)::bigint AS healthy_count,
                COALESCE(er.warning_count, 0)::bigint AS warning_count,
                COALESCE(er.critical_count, 0)::bigint AS critical_count,
                COALESCE(er.offline_count, 0)::bigint AS offline_count,
                COALESCE(er.cve_critical_high_count, 0)::bigint AS cve_critical_high_count,
                COALESCE(er.flake_names, ARRAY[]::text[]) AS flake_names,
                COALESCE(e.default_policy, 'manual') AS default_policy,
                COALESCE(e.auto_sync, TRUE) AS auto_sync,
                COALESCE(e.requires_approval, FALSE) AS requires_approval,
                COALESCE(e.is_production, FALSE) AS is_production,
                COALESCE(rbac.role_assignment_count, 0)::bigint AS role_assignment_count,
                cache.cache_name,
                cache.cache_url,
                cache.cache_type,
                cache.cache_enabled,
                compliance.compliance_bundle_id,
                compliance.compliance_bundle_name,
                compliance.compliance_bundle_framework
            FROM environments e
            LEFT JOIN view_environment_rollups er ON er.environment_id = e.id
            LEFT JOIN systems s ON s.environment_id = e.id
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::bigint AS role_assignment_count
                FROM user_environment_memberships uem_all
                WHERE uem_all.environment_id = e.id
            ) rbac ON TRUE
            LEFT JOIN LATERAL (
                SELECT
                    cd.name AS cache_name,
                    cd.push_to AS cache_url,
                    cd.cache_type AS cache_type,
                    cd.enabled AS cache_enabled
                FROM cache_destination_environments cde
                JOIN cache_destinations cd ON cd.id = cde.cache_destination_id
                WHERE cde.environment_id = e.id
                ORDER BY cd.name
                LIMIT 1
            ) cache ON TRUE
            LEFT JOIN LATERAL (
                SELECT
                    cb.id AS compliance_bundle_id,
                    cb.name AS compliance_bundle_name,
                    cb.framework AS compliance_bundle_framework
                FROM compliance_bundle_environments cbe
                JOIN compliance_bundles cb ON cb.id = cbe.bundle_id
                WHERE cbe.environment_id = e.id
                ORDER BY cb.name
                LIMIT 1
            ) compliance ON TRUE
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active, e.default_policy, e.auto_sync, e.requires_approval, e.is_production, er.active_system_count, er.healthy_count, er.warning_count, er.critical_count, er.offline_count, er.cve_critical_high_count, er.flake_names, rbac.role_assignment_count, cache.cache_name, cache.cache_url, cache.cache_type, cache.cache_enabled, compliance.compliance_bundle_id, compliance.compliance_bundle_name, compliance.compliance_bundle_framework
            ORDER BY e.name ASC
            "#,
        )
        .fetch_all(pool)
        .await?
    } else {
        // Member path: restrict to environments this user belongs to
        sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT
                e.id,
                e.name,
                e.description,
                COALESCE(e.color_hex, '#6B7280') AS color_hex,
                COALESCE(e.is_active, TRUE) AS is_active,
                COUNT(s.id)::bigint AS system_count,
                COALESCE(er.active_system_count, 0)::bigint AS active_system_count,
                COALESCE(er.healthy_count, 0)::bigint AS healthy_count,
                COALESCE(er.warning_count, 0)::bigint AS warning_count,
                COALESCE(er.critical_count, 0)::bigint AS critical_count,
                COALESCE(er.offline_count, 0)::bigint AS offline_count,
                COALESCE(er.cve_critical_high_count, 0)::bigint AS cve_critical_high_count,
                COALESCE(er.flake_names, ARRAY[]::text[]) AS flake_names,
                COALESCE(e.default_policy, 'manual') AS default_policy,
                COALESCE(e.auto_sync, TRUE) AS auto_sync,
                COALESCE(e.requires_approval, FALSE) AS requires_approval,
                COALESCE(e.is_production, FALSE) AS is_production,
                COALESCE(rbac.role_assignment_count, 0)::bigint AS role_assignment_count,
                cache.cache_name,
                cache.cache_url,
                cache.cache_type,
                cache.cache_enabled,
                compliance.compliance_bundle_id,
                compliance.compliance_bundle_name,
                compliance.compliance_bundle_framework
            FROM environments e
            JOIN user_environment_memberships uem
              ON uem.environment_id = e.id
             AND uem.user_id = $1
            LEFT JOIN view_environment_rollups er ON er.environment_id = e.id
            LEFT JOIN systems s ON s.environment_id = e.id
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::bigint AS role_assignment_count
                FROM user_environment_memberships uem_all
                WHERE uem_all.environment_id = e.id
            ) rbac ON TRUE
            LEFT JOIN LATERAL (
                SELECT
                    cd.name AS cache_name,
                    cd.push_to AS cache_url,
                    cd.cache_type AS cache_type,
                    cd.enabled AS cache_enabled
                FROM cache_destination_environments cde
                JOIN cache_destinations cd ON cd.id = cde.cache_destination_id
                WHERE cde.environment_id = e.id
                ORDER BY cd.name
                LIMIT 1
            ) cache ON TRUE
            LEFT JOIN LATERAL (
                SELECT
                    cb.id AS compliance_bundle_id,
                    cb.name AS compliance_bundle_name,
                    cb.framework AS compliance_bundle_framework
                FROM compliance_bundle_environments cbe
                JOIN compliance_bundles cb ON cb.id = cbe.bundle_id
                WHERE cbe.environment_id = e.id
                ORDER BY cb.name
                LIMIT 1
            ) compliance ON TRUE
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active, e.default_policy, e.auto_sync, e.requires_approval, e.is_production, er.active_system_count, er.healthy_count, er.warning_count, er.critical_count, er.offline_count, er.cve_critical_high_count, er.flake_names, rbac.role_assignment_count, cache.cache_name, cache.cache_url, cache.cache_type, cache.cache_enabled, compliance.compliance_bundle_id, compliance.compliance_bundle_name, compliance.compliance_bundle_framework
            ORDER BY e.name ASC
            "#,
        )
        .bind(user_id.unwrap())
        .fetch_all(pool)
        .await?
    };

    Ok(rows.into_iter().map(environment_summary_from_row).collect())
}

/// Fetch a single environment by ID, scoped to a user.
///
/// Admins may fetch any environment (`user_id = None`).
/// Non-admins only see environments they are a member of.
pub async fn find_environment_for_user(
    pool: &PgPool,
    environment_id: Uuid,
    user_id: Option<Uuid>,
) -> Result<Option<EnvironmentSummary>> {
    let row = if user_id.is_none() {
        sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT
                e.id,
                e.name,
                e.description,
                COALESCE(e.color_hex, '#6B7280') AS color_hex,
                COALESCE(e.is_active, TRUE) AS is_active,
                COUNT(s.id)::bigint AS system_count,
                COALESCE(er.active_system_count, 0)::bigint AS active_system_count,
                COALESCE(er.healthy_count, 0)::bigint AS healthy_count,
                COALESCE(er.warning_count, 0)::bigint AS warning_count,
                COALESCE(er.critical_count, 0)::bigint AS critical_count,
                COALESCE(er.offline_count, 0)::bigint AS offline_count,
                COALESCE(er.cve_critical_high_count, 0)::bigint AS cve_critical_high_count,
                COALESCE(er.flake_names, ARRAY[]::text[]) AS flake_names,
                COALESCE(e.default_policy, 'manual') AS default_policy,
                COALESCE(e.auto_sync, TRUE) AS auto_sync,
                COALESCE(e.requires_approval, FALSE) AS requires_approval,
                COALESCE(e.is_production, FALSE) AS is_production,
                COALESCE(rbac.role_assignment_count, 0)::bigint AS role_assignment_count,
                cache.cache_name,
                cache.cache_url,
                cache.cache_type,
                cache.cache_enabled,
                compliance.compliance_bundle_id,
                compliance.compliance_bundle_name,
                compliance.compliance_bundle_framework
            FROM environments e
            LEFT JOIN view_environment_rollups er ON er.environment_id = e.id
            LEFT JOIN systems s ON s.environment_id = e.id
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::bigint AS role_assignment_count
                FROM user_environment_memberships uem_all
                WHERE uem_all.environment_id = e.id
            ) rbac ON TRUE
            LEFT JOIN LATERAL (
                SELECT
                    cd.name AS cache_name,
                    cd.push_to AS cache_url,
                    cd.cache_type AS cache_type,
                    cd.enabled AS cache_enabled
                FROM cache_destination_environments cde
                JOIN cache_destinations cd ON cd.id = cde.cache_destination_id
                WHERE cde.environment_id = e.id
                ORDER BY cd.name
                LIMIT 1
            ) cache ON TRUE
            LEFT JOIN LATERAL (
                SELECT
                    cb.id AS compliance_bundle_id,
                    cb.name AS compliance_bundle_name,
                    cb.framework AS compliance_bundle_framework
                FROM compliance_bundle_environments cbe
                JOIN compliance_bundles cb ON cb.id = cbe.bundle_id
                WHERE cbe.environment_id = e.id
                ORDER BY cb.name
                LIMIT 1
            ) compliance ON TRUE
            WHERE e.id = $1
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active, e.default_policy, e.auto_sync, e.requires_approval, e.is_production, er.active_system_count, er.healthy_count, er.warning_count, er.critical_count, er.offline_count, er.cve_critical_high_count, er.flake_names, rbac.role_assignment_count, cache.cache_name, cache.cache_url, cache.cache_type, cache.cache_enabled, compliance.compliance_bundle_id, compliance.compliance_bundle_name, compliance.compliance_bundle_framework
            "#,
        )
        .bind(environment_id)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT
                e.id,
                e.name,
                e.description,
                COALESCE(e.color_hex, '#6B7280') AS color_hex,
                COALESCE(e.is_active, TRUE) AS is_active,
                COUNT(s.id)::bigint AS system_count,
                COALESCE(er.active_system_count, 0)::bigint AS active_system_count,
                COALESCE(er.healthy_count, 0)::bigint AS healthy_count,
                COALESCE(er.warning_count, 0)::bigint AS warning_count,
                COALESCE(er.critical_count, 0)::bigint AS critical_count,
                COALESCE(er.offline_count, 0)::bigint AS offline_count,
                COALESCE(er.cve_critical_high_count, 0)::bigint AS cve_critical_high_count,
                COALESCE(er.flake_names, ARRAY[]::text[]) AS flake_names,
                COALESCE(e.default_policy, 'manual') AS default_policy,
                COALESCE(e.auto_sync, TRUE) AS auto_sync,
                COALESCE(e.requires_approval, FALSE) AS requires_approval,
                COALESCE(e.is_production, FALSE) AS is_production,
                COALESCE(rbac.role_assignment_count, 0)::bigint AS role_assignment_count,
                cache.cache_name,
                cache.cache_url,
                cache.cache_type,
                cache.cache_enabled,
                compliance.compliance_bundle_id,
                compliance.compliance_bundle_name,
                compliance.compliance_bundle_framework
            FROM environments e
            JOIN user_environment_memberships uem
              ON uem.environment_id = e.id
             AND uem.user_id = $1
            LEFT JOIN view_environment_rollups er ON er.environment_id = e.id
            LEFT JOIN systems s ON s.environment_id = e.id
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::bigint AS role_assignment_count
                FROM user_environment_memberships uem_all
                WHERE uem_all.environment_id = e.id
            ) rbac ON TRUE
            LEFT JOIN LATERAL (
                SELECT
                    cd.name AS cache_name,
                    cd.push_to AS cache_url,
                    cd.cache_type AS cache_type,
                    cd.enabled AS cache_enabled
                FROM cache_destination_environments cde
                JOIN cache_destinations cd ON cd.id = cde.cache_destination_id
                WHERE cde.environment_id = e.id
                ORDER BY cd.name
                LIMIT 1
            ) cache ON TRUE
            LEFT JOIN LATERAL (
                SELECT
                    cb.id AS compliance_bundle_id,
                    cb.name AS compliance_bundle_name,
                    cb.framework AS compliance_bundle_framework
                FROM compliance_bundle_environments cbe
                JOIN compliance_bundles cb ON cb.id = cbe.bundle_id
                WHERE cbe.environment_id = e.id
                ORDER BY cb.name
                LIMIT 1
            ) compliance ON TRUE
            WHERE e.id = $2
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active, e.default_policy, e.auto_sync, e.requires_approval, e.is_production, er.active_system_count, er.healthy_count, er.warning_count, er.critical_count, er.offline_count, er.cve_critical_high_count, er.flake_names, rbac.role_assignment_count, cache.cache_name, cache.cache_url, cache.cache_type, cache.cache_enabled, compliance.compliance_bundle_id, compliance.compliance_bundle_name, compliance.compliance_bundle_framework
            "#,
        )
        .bind(user_id.unwrap())
        .bind(environment_id)
        .fetch_optional(pool)
        .await?
    };

    Ok(row.map(environment_summary_from_row))
}

/// Create a new environment and return API summary shape.
pub async fn create_environment(
    pool: &PgPool,
    name: &str,
    description: Option<&str>,
    color_hex: &str,
    is_active: bool,
    default_policy: &str,
    auto_sync: bool,
    requires_approval: bool,
    is_production: bool,
) -> Result<EnvironmentSummary> {
    let environment_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO environments (name, description, color_hex, is_active, default_policy, auto_sync, requires_approval, is_production)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(description)
    .bind(color_hex)
    .bind(is_active)
    .bind(default_policy)
    .bind(auto_sync)
    .bind(requires_approval)
    .bind(is_production)
    .fetch_one(pool)
    .await?;

    match find_environment_for_user(pool, environment_id, None).await? {
        Some(summary) => Ok(summary),
        None => anyhow::bail!("created environment was not visible after insert"),
    }
}

/// Delete an environment by id.
pub async fn delete_environment(pool: &PgPool, environment_id: Uuid) -> Result<u64> {
    let result = sqlx::query("DELETE FROM environments WHERE id = $1")
        .bind(environment_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Count systems assigned to an environment.
pub async fn count_systems_in_environment(pool: &PgPool, environment_id: Uuid) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM systems WHERE environment_id = $1",
    )
    .bind(environment_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Update an environment's metadata and return summary shape.
///
/// `default_policy`, `auto_sync`, `requires_approval`, and `is_production`
/// are `Option`: when `None`, the SQL `COALESCE` preserves the environment's
/// existing stored value instead of overwriting it. This lets callers (and
/// older clients that don't send these fields) update `name`/`description`/
/// `color_hex` without silently resetting the newer UI-metadata fields.
pub async fn update_environment_metadata(
    pool: &PgPool,
    environment_id: Uuid,
    name: &str,
    description: Option<&str>,
    color_hex: &str,
    default_policy: Option<&str>,
    auto_sync: Option<bool>,
    requires_approval: Option<bool>,
    is_production: Option<bool>,
) -> Result<Option<EnvironmentSummary>> {
    let environment_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE environments e
        SET name = $2,
            description = $3,
            color_hex = $4,
            default_policy = COALESCE($5, e.default_policy),
            auto_sync = COALESCE($6, e.auto_sync),
            requires_approval = COALESCE($7, e.requires_approval),
            is_production = COALESCE($8, e.is_production),
            updated_at = NOW()
        WHERE e.id = $1
        RETURNING e.id
        "#,
    )
    .bind(environment_id)
    .bind(name)
    .bind(description)
    .bind(color_hex)
    .bind(default_policy)
    .bind(auto_sync)
    .bind(requires_approval)
    .bind(is_production)
    .fetch_optional(pool)
    .await?;

    match environment_id {
        Some(environment_id) => find_environment_for_user(pool, environment_id, None).await,
        None => Ok(None),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Policy queries
// ─────────────────────────────────────────────────────────────────────────────

/// Row type for policy queries
#[derive(Debug, FromRow)]
pub struct PolicyRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
}

/// Get all available deployment policies.
pub async fn list_deployment_policies(pool: &PgPool) -> Result<Vec<DeploymentPolicySummary>> {
    let rows = sqlx::query_as::<_, PolicyRow>(
        "SELECT id, name, description, policy_type, config, enabled FROM deployment_policies ORDER BY name",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| DeploymentPolicySummary {
            id: r.id,
            name: r.name,
            description: r.description,
            policy_type: r.policy_type,
            config: r.config,
            enabled: r.enabled,
        })
        .collect())
}

/// Get required policy IDs for an environment (the baseline).
pub async fn get_environment_required_policy_ids(
    pool: &PgPool,
    environment_id: Uuid,
) -> Result<Vec<Uuid>> {
    let rows = sqlx::query_scalar!(
        "SELECT policy_id FROM environment_policies WHERE environment_id = $1",
        environment_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Get required policy IDs for all environments visible to the caller.
pub async fn list_environment_policy_map_for_user(
    pool: &PgPool,
    user_id: Option<Uuid>,
) -> Result<Vec<EnvironmentPolicyMapEntry>> {
    let rows: Vec<(Uuid, Vec<Uuid>)> = if user_id.is_none() {
        sqlx::query_as::<_, (Uuid, Vec<Uuid>)>(
            r#"
            SELECT
                e.id AS environment_id,
                COALESCE(
                    array_agg(DISTINCT ep.policy_id) FILTER (WHERE ep.policy_id IS NOT NULL),
                    ARRAY[]::uuid[]
                ) AS required_policy_ids
            FROM environments e
            LEFT JOIN environment_policies ep ON ep.environment_id = e.id
            GROUP BY e.id
            ORDER BY e.id
            "#,
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, (Uuid, Vec<Uuid>)>(
            r#"
            SELECT
                e.id AS environment_id,
                COALESCE(
                    array_agg(DISTINCT ep.policy_id) FILTER (WHERE ep.policy_id IS NOT NULL),
                    ARRAY[]::uuid[]
                ) AS required_policy_ids
            FROM environments e
            JOIN user_environment_memberships uem
              ON uem.environment_id = e.id
             AND uem.user_id = $1
            LEFT JOIN environment_policies ep ON ep.environment_id = e.id
            GROUP BY e.id
            ORDER BY e.id
            "#,
        )
        .bind(user_id.unwrap())
        .fetch_all(pool)
        .await?
    };

    Ok(rows
        .into_iter()
        .map(
            |(environment_id, required_policy_ids)| EnvironmentPolicyMapEntry {
                environment_id,
                required_policy_ids,
            },
        )
        .collect())
}

/// Get an environment with its required policies (baseline).
pub async fn get_environment_with_policies(
    pool: &PgPool,
    environment_id: Uuid,
) -> Result<Option<EnvironmentWithPolicies>> {
    // First get the environment
    let env_row = sqlx::query_as::<_, EnvironmentRow>(
        r#"
        SELECT
            e.id,
            e.name,
            e.description,
            COALESCE(e.color_hex, '#6B7280') AS color_hex,
            COALESCE(e.is_active, TRUE) AS is_active,
            COUNT(s.id)::bigint AS system_count,
            COALESCE(er.active_system_count, 0)::bigint AS active_system_count,
            COALESCE(er.healthy_count, 0)::bigint AS healthy_count,
            COALESCE(er.warning_count, 0)::bigint AS warning_count,
            COALESCE(er.critical_count, 0)::bigint AS critical_count,
            COALESCE(er.offline_count, 0)::bigint AS offline_count,
            COALESCE(er.cve_critical_high_count, 0)::bigint AS cve_critical_high_count,
            COALESCE(er.flake_names, ARRAY[]::text[]) AS flake_names,
            COALESCE(e.default_policy, 'manual') AS default_policy,
            COALESCE(e.auto_sync, TRUE) AS auto_sync,
            COALESCE(e.requires_approval, FALSE) AS requires_approval,
            COALESCE(e.is_production, FALSE) AS is_production,
            COALESCE(rbac.role_assignment_count, 0)::bigint AS role_assignment_count,
                cache.cache_name,
                cache.cache_url,
                cache.cache_type,
                cache.cache_enabled,
                compliance.compliance_bundle_id,
            compliance.compliance_bundle_name,
            compliance.compliance_bundle_framework
        FROM environments e
        LEFT JOIN view_environment_rollups er ON er.environment_id = e.id
        LEFT JOIN systems s ON s.environment_id = e.id
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS role_assignment_count
            FROM user_environment_memberships uem_all
            WHERE uem_all.environment_id = e.id
        ) rbac ON TRUE
        LEFT JOIN LATERAL (
            SELECT
                cd.name AS cache_name,
                cd.push_to AS cache_url,
                cd.cache_type AS cache_type,
                cd.enabled AS cache_enabled
            FROM cache_destination_environments cde
            JOIN cache_destinations cd ON cd.id = cde.cache_destination_id
            WHERE cde.environment_id = e.id
            ORDER BY cd.name
            LIMIT 1
        ) cache ON TRUE
        LEFT JOIN LATERAL (
            SELECT
                cb.id AS compliance_bundle_id,
                cb.name AS compliance_bundle_name,
                cb.framework AS compliance_bundle_framework
            FROM compliance_bundle_environments cbe
            JOIN compliance_bundles cb ON cb.id = cbe.bundle_id
            WHERE cbe.environment_id = e.id
            ORDER BY cb.name
            LIMIT 1
        ) compliance ON TRUE
        WHERE e.id = $1
        GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active, e.default_policy, e.auto_sync, e.requires_approval, e.is_production, er.active_system_count, er.healthy_count, er.warning_count, er.critical_count, er.offline_count, er.cve_critical_high_count, er.flake_names, rbac.role_assignment_count, cache.cache_name, cache.cache_url, cache.cache_type, cache.cache_enabled, compliance.compliance_bundle_id, compliance.compliance_bundle_name, compliance.compliance_bundle_framework
        "#,
    )
    .bind(environment_id)
    .fetch_optional(pool)
    .await?;

    match env_row {
        Some(env) => {
            // Get required policy IDs
            let policy_ids = get_environment_required_policy_ids(pool, environment_id).await?;

            Ok(Some(EnvironmentWithPolicies {
                id: env.id,
                name: env.name,
                description: env.description,
                color_hex: env.color_hex,
                is_active: env.is_active,
                system_count: env.system_count,
                required_policy_ids: policy_ids,
            }))
        }
        None => Ok(None),
    }
}

/// Set the required policies for an environment (the baseline).
/// This replaces all existing environment policies with the new set.
pub async fn set_environment_required_policies(
    pool: &PgPool,
    environment_id: Uuid,
    policy_ids: &[Uuid],
    user_id: Option<Uuid>,
) -> Result<Vec<Uuid>> {
    // Deduplicate before validation.
    let unique_ids: Vec<Uuid> = policy_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    // Validate the entire requested set inside a single transaction
    // BEFORE any mutation.  This ensures that a partial failure does
    // not leave the environment without its previous assignments.
    let mut tx = pool.begin().await?;

    // Count how many of the requested IDs are valid: they exist,
    // are enabled, and are NOT require_cf_agent (built-in invariant).
    let valid_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM deployment_policies
        WHERE id = ANY($1)
          AND enabled IS TRUE
          AND policy_type <> 'require_cf_agent'
        "#,
    )
    .bind(&unique_ids)
    .fetch_one(&mut *tx)
    .await?;

    if valid_count != unique_ids.len() as i64 {
        tx.rollback().await?;
        anyhow::bail!(
            "One or more policies do not exist, are disabled, \
             or cannot be assigned (require_cf_agent is a built-in invariant)"
        );
    }

    // Atomically replace: delete existing, then insert the validated set.
    // All operations share one transaction so a failure at any point
    // rolls back the entire replacement, preserving the previous
    // assignment set.
    sqlx::query("DELETE FROM environment_policies WHERE environment_id = $1")
        .bind(environment_id)
        .execute(&mut *tx)
        .await?;

    for policy_id in &unique_ids {
        sqlx::query(
            "INSERT INTO environment_policies (environment_id, policy_id, created_by) \
             VALUES ($1, $2, $3)",
        )
        .bind(environment_id)
        .bind(policy_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(unique_ids)
}

/// Get system policies: includes both environment baseline AND system-specific additional policies.
/// This represents the complete set of policies a system must satisfy.
pub async fn get_system_effective_policy_ids(pool: &PgPool, system_id: Uuid) -> Result<Vec<Uuid>> {
    // First get the environment for this system
    let environment_id: Option<Option<Uuid>> = sqlx::query_scalar!(
        "SELECT environment_id FROM systems WHERE id = $1",
        system_id
    )
    .fetch_optional(pool)
    .await?;

    let mut all_policy_ids: Vec<Uuid> = Vec::new();

    // Add environment baseline policies
    if let Some(Some(env_id)) = environment_id {
        let env_policies = get_environment_required_policy_ids(pool, env_id).await?;
        all_policy_ids.extend(env_policies);
    }

    // Add system-specific policies
    let system_policies: Vec<Uuid> = sqlx::query_scalar!(
        "SELECT policy_id FROM system_policies WHERE system_id = $1",
        system_id
    )
    .fetch_all(pool)
    .await?;

    all_policy_ids.extend(system_policies);

    // Remove duplicates (system policies can overlap with environment)
    all_policy_ids.sort();
    all_policy_ids.dedup();

    Ok(all_policy_ids)
}

/// Add an additional policy to a system.
/// Returns the policy_id if successful, or error if it already exists or is already required by environment.
pub async fn add_system_policy(
    pool: &PgPool,
    system_id: Uuid,
    policy_id: Uuid,
    user_id: Option<Uuid>,
) -> Result<Uuid> {
    // Check if policy is already in environment baseline (can't add duplicate)
    let environment_id: Option<Option<Uuid>> = sqlx::query_scalar!(
        "SELECT environment_id FROM systems WHERE id = $1",
        system_id
    )
    .fetch_optional(pool)
    .await?;

    if let Some(Some(env_id)) = environment_id {
        let is_required: Option<Uuid> = sqlx::query_scalar!(
            "SELECT policy_id FROM environment_policies WHERE environment_id = $1 AND policy_id = $2",
            env_id,
            policy_id
        )
        .fetch_optional(pool)
        .await?;

        if is_required.is_some() {
            // Policy is already required by environment baseline - no need to add
            return Ok(policy_id);
        }
    }

    // Check if already added to system
    let existing: Option<Uuid> = sqlx::query_scalar!(
        "SELECT policy_id FROM system_policies WHERE system_id = $1 AND policy_id = $2",
        system_id,
        policy_id
    )
    .fetch_optional(pool)
    .await?;

    if existing.is_some() {
        return Ok(policy_id);
    }

    // Add the policy
    sqlx::query!(
        "INSERT INTO system_policies (system_id, policy_id, created_by) VALUES ($1, $2, $3)",
        system_id,
        policy_id,
        user_id
    )
    .execute(pool)
    .await?;

    Ok(policy_id)
}

/// Remove a policy from a system.
/// Only removes system-specific additions, NOT environment baseline policies.
pub async fn remove_system_policy(pool: &PgPool, system_id: Uuid, policy_id: Uuid) -> Result<bool> {
    // First check if this policy is required by the environment baseline
    let environment_id: Option<Option<Uuid>> = sqlx::query_scalar!(
        "SELECT environment_id FROM systems WHERE id = $1",
        system_id
    )
    .fetch_optional(pool)
    .await?;

    if let Some(Some(env_id)) = environment_id {
        let is_required: Option<Uuid> = sqlx::query_scalar!(
            "SELECT policy_id FROM environment_policies WHERE environment_id = $1 AND policy_id = $2",
            env_id,
            policy_id
        )
        .fetch_optional(pool)
        .await?;

        if is_required.is_some() {
            // Cannot remove environment baseline policy - return error
            anyhow::bail!("Cannot remove policy that is required by environment baseline");
        }
    }

    // Remove the system-specific policy
    let result = sqlx::query!(
        "DELETE FROM system_policies WHERE system_id = $1 AND policy_id = $2",
        system_id,
        policy_id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_summary_from_row_maps_rollup_fields() {
        let summary = environment_summary_from_row(EnvironmentRow {
            id: Uuid::from_u128(42),
            name: "production".to_string(),
            description: Some("Live fleet".to_string()),
            color_hex: "#0F766E".to_string(),
            is_active: true,
            system_count: 6,
            active_system_count: 5,
            healthy_count: 4,
            warning_count: 1,
            critical_count: 1,
            offline_count: 0,
            cve_critical_high_count: 9,
            flake_names: vec!["infra".to_string(), "edge".to_string()],
            default_policy: "manual".to_string(),
            auto_sync: false,
            requires_approval: false,
            is_production: true,
            role_assignment_count: 0,
            cache_name: None,
            cache_url: None,
            cache_type: None,
            cache_enabled: None,
            compliance_bundle_id: None,
            compliance_bundle_name: None,
            compliance_bundle_framework: None,
        });

        assert_eq!(summary.system_count, 6);
        assert_eq!(summary.rollup.active_system_count, 5);
        assert_eq!(summary.rollup.healthy, 4);
        assert_eq!(summary.rollup.warning, 1);
        assert_eq!(summary.rollup.critical, 1);
        assert_eq!(summary.rollup.offline, 0);
        assert_eq!(summary.rollup.cve_critical_high, 9);
        assert_eq!(summary.rollup.flakes, vec!["infra", "edge"]);
    }

    #[test]
    fn environment_summary_from_row_reports_cache_disabled_when_not_enabled() {
        let summary = environment_summary_from_row(EnvironmentRow {
            id: Uuid::from_u128(7),
            name: "staging".to_string(),
            description: None,
            color_hex: "#B45309".to_string(),
            is_active: true,
            system_count: 1,
            active_system_count: 1,
            healthy_count: 1,
            warning_count: 0,
            critical_count: 0,
            offline_count: 0,
            cve_critical_high_count: 0,
            flake_names: vec![],
            default_policy: "manual".to_string(),
            auto_sync: true,
            requires_approval: false,
            is_production: false,
            role_assignment_count: 0,
            cache_name: Some("staging-cache".to_string()),
            cache_url: Some("s3://staging-cache".to_string()),
            cache_type: Some("S3".to_string()),
            cache_enabled: Some(false),
            compliance_bundle_id: None,
            compliance_bundle_name: None,
            compliance_bundle_framework: None,
        });

        let cache = summary
            .cache
            .expect("cache should be present when a cache is assigned");
        assert_eq!(cache.status, "disabled");
    }

    #[test]
    fn environment_summary_from_row_reports_cache_enabled_when_flagged() {
        let summary = environment_summary_from_row(EnvironmentRow {
            id: Uuid::from_u128(8),
            name: "production".to_string(),
            description: None,
            color_hex: "#0F766E".to_string(),
            is_active: true,
            system_count: 1,
            active_system_count: 1,
            healthy_count: 1,
            warning_count: 0,
            critical_count: 0,
            offline_count: 0,
            cve_critical_high_count: 0,
            flake_names: vec![],
            default_policy: "manual".to_string(),
            auto_sync: true,
            requires_approval: true,
            is_production: true,
            role_assignment_count: 0,
            cache_name: Some("prod-cache".to_string()),
            cache_url: Some("s3://prod-cache".to_string()),
            cache_type: Some("S3".to_string()),
            cache_enabled: Some(true),
            compliance_bundle_id: None,
            compliance_bundle_name: None,
            compliance_bundle_framework: None,
        });

        let cache = summary
            .cache
            .expect("cache should be present when a cache is assigned");
        assert_eq!(cache.status, "enabled");
    }

    #[test]
    fn environment_summary_from_row_reports_cache_disabled_when_enabled_flag_missing() {
        // A NULL enabled flag (should not normally happen given the NOT NULL
        // column default, but the LATERAL join yields NULL when defensively
        // coded) must never be interpreted as "healthy"/"enabled".
        let summary = environment_summary_from_row(EnvironmentRow {
            id: Uuid::from_u128(9),
            name: "lab".to_string(),
            description: None,
            color_hex: "#6B7280".to_string(),
            is_active: true,
            system_count: 1,
            active_system_count: 1,
            healthy_count: 1,
            warning_count: 0,
            critical_count: 0,
            offline_count: 0,
            cve_critical_high_count: 0,
            flake_names: vec![],
            default_policy: "manual".to_string(),
            auto_sync: true,
            requires_approval: false,
            is_production: false,
            role_assignment_count: 0,
            cache_name: Some("lab-cache".to_string()),
            cache_url: Some("s3://lab-cache".to_string()),
            cache_type: Some("S3".to_string()),
            cache_enabled: None,
            compliance_bundle_id: None,
            compliance_bundle_name: None,
            compliance_bundle_framework: None,
        });

        let cache = summary
            .cache
            .expect("cache should be present when a cache is assigned");
        assert_eq!(cache.status, "disabled");
    }

    // ─────────────────────────────────────────────────────────────────────
    // DB-backed regression tests. These require a live Postgres instance
    // (sqlx::test spins up an isolated database per test using the crate's
    // migrations) and are `#[ignore]`d by default. Run explicitly against a
    // local, isolated development database with:
    //
    //   cargo test -p default --lib queries::environments::tests::db_tests -- --ignored
    // ─────────────────────────────────────────────────────────────────────
    #[cfg(test)]
    mod db_tests {
        use super::*;

        async fn insert_environment(
            pool: &PgPool,
            name: &str,
            default_policy: &str,
            auto_sync: bool,
            requires_approval: bool,
            is_production: bool,
        ) -> EnvironmentSummary {
            create_environment(
                pool,
                name,
                None,
                "#2563EB",
                true,
                default_policy,
                auto_sync,
                requires_approval,
                is_production,
            )
            .await
            .expect("environment should be created")
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn update_environment_metadata_preserves_omitted_ui_metadata_fields(pool: PgPool) {
            let created =
                insert_environment(&pool, "regression-preserve", "pinned", false, true, true).await;

            // Simulate an older client that only sends name/description/color —
            // the four newer fields are omitted (None). The stored values must
            // be preserved, not silently reset to defaults.
            let updated = update_environment_metadata(
                &pool,
                created.id,
                "regression-preserve-renamed",
                Some("renamed description"),
                "#16A34A",
                None,
                None,
                None,
                None,
            )
            .await
            .expect("update should succeed")
            .expect("environment should still exist");

            assert_eq!(updated.name, "regression-preserve-renamed");
            assert_eq!(updated.description, Some("renamed description".to_string()));
            assert_eq!(updated.color_hex, "#16A34A");
            assert_eq!(updated.default_policy, Some("pinned".to_string()));
            assert_eq!(updated.auto_sync, Some(false));
            assert_eq!(updated.requires_approval, Some(true));
            assert_eq!(updated.is_production, Some(true));
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn update_environment_metadata_applies_explicitly_provided_fields(pool: PgPool) {
            let created =
                insert_environment(&pool, "regression-explicit", "manual", true, false, false)
                    .await;

            let updated = update_environment_metadata(
                &pool,
                created.id,
                "regression-explicit",
                None,
                "#2563EB",
                Some("auto_latest"),
                Some(false),
                Some(true),
                Some(true),
            )
            .await
            .expect("update should succeed")
            .expect("environment should still exist");

            assert_eq!(updated.default_policy, Some("auto_latest".to_string()));
            assert_eq!(updated.auto_sync, Some(false));
            assert_eq!(updated.requires_approval, Some(true));
            assert_eq!(updated.is_production, Some(true));
        }

        #[sqlx::test]
        #[ignore = "requires live database connection"]
        async fn update_environment_metadata_can_mix_omitted_and_explicit_fields(pool: PgPool) {
            let created =
                insert_environment(&pool, "regression-mixed", "manual", true, true, false).await;

            // Only flip is_production; leave the other three untouched.
            let updated = update_environment_metadata(
                &pool,
                created.id,
                "regression-mixed",
                None,
                "#2563EB",
                None,
                None,
                None,
                Some(true),
            )
            .await
            .expect("update should succeed")
            .expect("environment should still exist");

            assert_eq!(updated.default_policy, Some("manual".to_string()));
            assert_eq!(updated.auto_sync, Some(true));
            assert_eq!(updated.requires_approval, Some(true));
            assert_eq!(updated.is_production, Some(true));
        }
    }
}
