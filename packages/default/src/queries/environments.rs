use crate::api::models::{
    DeploymentPolicySummary, EnvironmentPolicyMapEntry, EnvironmentRollup, EnvironmentSummary,
    EnvironmentWithPolicies,
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
                COALESCE(er.flake_names, ARRAY[]::text[]) AS flake_names
            FROM environments e
            LEFT JOIN view_environment_rollups er ON er.environment_id = e.id
            LEFT JOIN systems s ON s.environment_id = e.id
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active, er.active_system_count, er.healthy_count, er.warning_count, er.critical_count, er.offline_count, er.cve_critical_high_count, er.flake_names
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
                COALESCE(er.flake_names, ARRAY[]::text[]) AS flake_names
            FROM environments e
            JOIN user_environment_memberships uem
              ON uem.environment_id = e.id
             AND uem.user_id = $1
            LEFT JOIN view_environment_rollups er ON er.environment_id = e.id
            LEFT JOIN systems s ON s.environment_id = e.id
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active, er.active_system_count, er.healthy_count, er.warning_count, er.critical_count, er.offline_count, er.cve_critical_high_count, er.flake_names
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
                COALESCE(er.flake_names, ARRAY[]::text[]) AS flake_names
            FROM environments e
            LEFT JOIN view_environment_rollups er ON er.environment_id = e.id
            LEFT JOIN systems s ON s.environment_id = e.id
            WHERE e.id = $1
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active, er.active_system_count, er.healthy_count, er.warning_count, er.critical_count, er.offline_count, er.cve_critical_high_count, er.flake_names
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
                COALESCE(er.flake_names, ARRAY[]::text[]) AS flake_names
            FROM environments e
            JOIN user_environment_memberships uem
              ON uem.environment_id = e.id
             AND uem.user_id = $1
            LEFT JOIN view_environment_rollups er ON er.environment_id = e.id
            LEFT JOIN systems s ON s.environment_id = e.id
            WHERE e.id = $2
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active, er.active_system_count, er.healthy_count, er.warning_count, er.critical_count, er.offline_count, er.cve_critical_high_count, er.flake_names
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
) -> Result<EnvironmentSummary> {
    let environment_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO environments (name, description, color_hex, is_active)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(description)
    .bind(color_hex)
    .bind(is_active)
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

/// Update an environment name/description and return summary shape.
pub async fn update_environment_metadata(
    pool: &PgPool,
    environment_id: Uuid,
    name: &str,
    description: Option<&str>,
    color_hex: &str,
) -> Result<Option<EnvironmentSummary>> {
    let environment_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE environments e
        SET name = $2,
            description = $3,
            color_hex = $4,
            updated_at = NOW()
        WHERE e.id = $1
        RETURNING e.id
        "#,
    )
    .bind(environment_id)
    .bind(name)
    .bind(description)
    .bind(color_hex)
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
            COALESCE(er.flake_names, ARRAY[]::text[]) AS flake_names
        FROM environments e
        LEFT JOIN view_environment_rollups er ON er.environment_id = e.id
        LEFT JOIN systems s ON s.environment_id = e.id
        WHERE e.id = $1
        GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active, er.active_system_count, er.healthy_count, er.warning_count, er.critical_count, er.offline_count, er.cve_critical_high_count, er.flake_names
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
    // Delete existing environment policies
    sqlx::query!(
        "DELETE FROM environment_policies WHERE environment_id = $1",
        environment_id
    )
    .execute(pool)
    .await?;

    // Insert new policies
    for policy_id in policy_ids {
        sqlx::query!(
            "INSERT INTO environment_policies (environment_id, policy_id, created_by) VALUES ($1, $2, $3)",
            environment_id,
            policy_id,
            user_id
        )
        .execute(pool)
        .await?;
    }

    Ok(policy_ids.to_vec())
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
}
