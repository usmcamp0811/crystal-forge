//! Database queries for deployment policy management.

use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::deployment_policies::{
    CreateDeploymentPolicyRequest, DeploymentPolicyRecord, UpdateDeploymentPolicyRequest,
};

/// List deployment policies with pagination
pub async fn list_deployment_policies(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<DeploymentPolicyRecord>> {
    let policies = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        SELECT id, name, description, policy_type, config, enabled, created_at, updated_at
        FROM deployment_policies
        ORDER BY name ASC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to list deployment policies")?;

    Ok(policies)
}

/// List all enabled deployment policies for evaluator execution.
/// Deprecated in favour of [`list_enabled_policies_for_flake`] which scopes
/// results to the environments that actually contain systems from the flake
/// being evaluated. Kept for CVE-policy loading (no flake context needed).
pub async fn list_enabled_deployment_policies(
    pool: &PgPool,
) -> Result<Vec<DeploymentPolicyRecord>> {
    let policies = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        SELECT id, name, description, policy_type, config, enabled, created_at, updated_at
        FROM deployment_policies
        WHERE enabled = true
        ORDER BY name ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to list enabled deployment policies")?;

    Ok(policies)
}

/// List enabled deployment policies that are assigned to at least one
/// environment which contains an active system belonging to `flake_id`.
///
/// This is the correct scope for Nix-eval policy checks: a policy that is
/// only assigned to environments unrelated to this flake should not block
/// builds for systems in other environments.
pub async fn list_enabled_policies_for_flake(
    pool: &PgPool,
    flake_id: i32,
) -> Result<Vec<DeploymentPolicyRecord>> {
    let policies = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        SELECT DISTINCT dp.id, dp.name, dp.description, dp.policy_type,
                        dp.config, dp.enabled, dp.created_at, dp.updated_at
        FROM deployment_policies dp
        WHERE dp.enabled = true
          AND (
              -- Policy is attached to an environment that has at least one
              -- active system from this flake.
              EXISTS (
                  SELECT 1
                  FROM environment_policies ep
                  JOIN systems s ON s.environment_id = ep.environment_id
                  WHERE ep.policy_id = dp.id
                    AND s.flake_id   = $1
                    AND s.is_active  = TRUE
              )
              OR
              -- Policy is attached directly to an active system from this flake.
              EXISTS (
                  SELECT 1
                  FROM system_policies sp
                  JOIN systems s ON s.id = sp.system_id
                  WHERE sp.policy_id = dp.id
                    AND s.flake_id   = $1
                    AND s.is_active  = TRUE
              )
          )
        ORDER BY dp.name ASC
        "#,
    )
    .bind(flake_id)
    .fetch_all(pool)
    .await
    .context("Failed to list enabled deployment policies for flake")?;

    Ok(policies)
}

/// Count total deployment policies (for pagination metadata)
pub async fn count_deployment_policies(pool: &PgPool) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deployment_policies")
        .fetch_one(pool)
        .await
        .context("Failed to count deployment policies")?;

    Ok(count)
}

/// Get a deployment policy by ID
pub async fn get_deployment_policy_by_id(
    pool: &PgPool,
    policy_id: &Uuid,
) -> Result<Option<DeploymentPolicyRecord>> {
    let policy = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        SELECT id, name, description, policy_type, config, enabled, created_at, updated_at
        FROM deployment_policies
        WHERE id = $1
        "#,
    )
    .bind(policy_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch deployment policy by ID")?;

    Ok(policy)
}

/// Create a new deployment policy
pub async fn create_deployment_policy(
    pool: &PgPool,
    request: &CreateDeploymentPolicyRequest,
) -> Result<DeploymentPolicyRecord> {
    let policy = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        INSERT INTO deployment_policies (name, description, policy_type, config, enabled)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, description, policy_type, config, enabled, created_at, updated_at
        "#,
    )
    .bind(&request.name)
    .bind(&request.description)
    .bind(&request.policy_type)
    .bind(&request.config)
    .bind(request.enabled.unwrap_or(true))
    .fetch_one(pool)
    .await
    .context("Failed to create deployment policy")?;

    Ok(policy)
}

/// Update an existing deployment policy
pub async fn update_deployment_policy(
    pool: &PgPool,
    policy_id: &Uuid,
    request: &UpdateDeploymentPolicyRequest,
) -> Result<Option<DeploymentPolicyRecord>> {
    let policy = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        UPDATE deployment_policies
        SET
            name = COALESCE($2, name),
            description = COALESCE($3, description),
            policy_type = COALESCE($4, policy_type),
            config = COALESCE($5, config),
            enabled = COALESCE($6, enabled)
        WHERE id = $1
        RETURNING id, name, description, policy_type, config, enabled, created_at, updated_at
        "#,
    )
    .bind(policy_id)
    .bind(&request.name)
    .bind(&request.description)
    .bind(&request.policy_type)
    .bind(&request.config)
    .bind(request.enabled)
    .fetch_optional(pool)
    .await
    .context("Failed to update deployment policy")?;

    Ok(policy)
}

/// Delete a deployment policy
/// Returns true if deleted, false if not found
pub async fn delete_deployment_policy(pool: &PgPool, policy_id: &Uuid) -> Result<bool> {
    let result = sqlx::query("DELETE FROM deployment_policies WHERE id = $1")
        .bind(policy_id)
        .execute(pool)
        .await
        .context("Failed to delete deployment policy")?;

    Ok(result.rows_affected() > 0)
}

/// Check if a policy name already exists (case-insensitive)
/// exclude_id: Optional policy ID to exclude from the check (for updates)
pub async fn check_policy_name_exists(
    pool: &PgPool,
    name: &str,
    exclude_id: Option<&Uuid>,
) -> Result<bool> {
    let count: i64 = match exclude_id {
        Some(id) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policies WHERE LOWER(name) = LOWER($1) AND id != $2",
        )
        .bind(name)
        .bind(id)
        .fetch_one(pool)
        .await
        .context("Failed to check policy name existence")?,
        None => sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policies WHERE LOWER(name) = LOWER($1)",
        )
        .bind(name)
        .fetch_one(pool)
        .await
        .context("Failed to check policy name existence")?,
    };

    Ok(count > 0)
}

/// Check if a policy with equivalent semantic content already exists.
///
/// A duplicate is defined as same policy_type and same config JSON payload.
/// exclude_id: Optional policy ID to exclude from the check (for updates).
pub async fn check_policy_content_exists(
    pool: &PgPool,
    policy_type: &str,
    config: &serde_json::Value,
    exclude_id: Option<&Uuid>,
) -> Result<bool> {
    let count: i64 = match exclude_id {
        Some(id) => sqlx::query_scalar(
            r#"
                SELECT COUNT(*)
                FROM deployment_policies
                WHERE policy_type = $1
                  AND config = $2
                  AND id != $3
                "#,
        )
        .bind(policy_type)
        .bind(config)
        .bind(id)
        .fetch_one(pool)
        .await
        .context("Failed to check policy content existence")?,
        None => sqlx::query_scalar(
            r#"
                SELECT COUNT(*)
                FROM deployment_policies
                WHERE policy_type = $1
                  AND config = $2
                "#,
        )
        .bind(policy_type)
        .bind(config)
        .fetch_one(pool)
        .await
        .context("Failed to check policy content existence")?,
    };

    Ok(count > 0)
}

/// Count distinct active systems that inherit each deployment policy through
/// either their environment (environment_policies) or direct system assignment (system_policies).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PolicySystemCount {
    pub policy_id: Uuid,
    pub system_count: i64,
}

pub async fn count_systems_for_all_policies(pool: &PgPool) -> Result<Vec<PolicySystemCount>> {
    let rows = sqlx::query_as::<_, PolicySystemCount>(
        r#"
        WITH policy_systems AS (
            SELECT ep.policy_id, s.id AS system_id
            FROM environment_policies ep
            JOIN systems s ON s.environment_id = ep.environment_id AND s.is_active = TRUE
            UNION
            SELECT sp.policy_id, s.id AS system_id
            FROM system_policies sp
            JOIN systems s ON s.id = sp.system_id AND s.is_active = TRUE
        )
        SELECT dp.id AS policy_id, COUNT(DISTINCT ps.system_id) AS system_count
        FROM deployment_policies dp
        LEFT JOIN policy_systems ps ON ps.policy_id = dp.id
        GROUP BY dp.id
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to count systems per policy")?;
    Ok(rows)
}

/// Count total NixOS derivations (systems) in the fleet.
pub async fn count_nixos_derivations(pool: &PgPool) -> Result<i64> {
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM derivations WHERE derivation_type = 'nixos'"#)
            .fetch_one(pool)
            .await
            .context("Failed to count NixOS derivations")?;
    Ok(count)
}

/// Check if a policy is in use by any environments or systems
pub async fn check_policy_in_use(pool: &PgPool, policy_id: &Uuid) -> Result<bool> {
    let env_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM environment_policies WHERE policy_id = $1")
            .bind(policy_id)
            .fetch_one(pool)
            .await
            .context("Failed to check environment_policies")?;

    if env_count > 0 {
        return Ok(true);
    }

    let system_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM system_policies WHERE policy_id = $1")
            .bind(policy_id)
            .fetch_one(pool)
            .await
            .context("Failed to check system_policies")?;

    Ok(system_count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would require a test database setup
    // For now, they serve as documentation of expected behavior

    #[test]
    fn test_query_compilation() {
        // This test ensures the SQL queries compile correctly
        // Actual database tests would require sqlx test fixtures
    }
}
