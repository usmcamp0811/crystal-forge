use crate::api::models::EnvironmentSummary;
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
}

/// Fetch the environment record associated with this system
pub async fn get_environment(pool: &PgPool, id: Uuid) -> Result<Option<Environment>> {
    let env = sqlx::query_as::<_, Environment>("SELECT * FROM environment WHERE id = $1")
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
                COUNT(s.id) AS system_count
            FROM environments e
            LEFT JOIN systems s ON s.environment_id = e.id
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active
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
                COUNT(s.id) AS system_count
            FROM environments e
            JOIN user_environment_memberships uem
              ON uem.environment_id = e.id
             AND uem.user_id = $1
            LEFT JOIN systems s ON s.environment_id = e.id
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active
            ORDER BY e.name ASC
            "#,
        )
        .bind(user_id.unwrap())
        .fetch_all(pool)
        .await?
    };

    Ok(rows
        .into_iter()
        .map(|r| EnvironmentSummary {
            id: r.id,
            name: r.name,
            description: r.description,
            color_hex: r.color_hex,
            is_active: r.is_active,
            system_count: r.system_count,
        })
        .collect())
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
                COUNT(s.id) AS system_count
            FROM environments e
            LEFT JOIN systems s ON s.environment_id = e.id
            WHERE e.id = $1
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active
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
                COUNT(s.id) AS system_count
            FROM environments e
            JOIN user_environment_memberships uem
              ON uem.environment_id = e.id
             AND uem.user_id = $1
            LEFT JOIN systems s ON s.environment_id = e.id
            WHERE e.id = $2
            GROUP BY e.id, e.name, e.description, e.color_hex, e.is_active
            "#,
        )
        .bind(user_id.unwrap())
        .bind(environment_id)
        .fetch_optional(pool)
        .await?
    };

    Ok(row.map(|r| EnvironmentSummary {
        id: r.id,
        name: r.name,
        description: r.description,
        color_hex: r.color_hex,
        is_active: r.is_active,
        system_count: r.system_count,
    }))
}

/// Create a new environment and return API summary shape.
pub async fn create_environment(
    pool: &PgPool,
    name: &str,
    description: Option<&str>,
    color_hex: &str,
    is_active: bool,
) -> Result<EnvironmentSummary> {
    let row = sqlx::query_as::<_, EnvironmentRow>(
        r#"
        INSERT INTO environments (name, description, color_hex, is_active)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, description, COALESCE(color_hex, '#6B7280') AS color_hex, COALESCE(is_active, TRUE) AS is_active, 0::bigint AS system_count
        "#,
    )
    .bind(name)
    .bind(description)
    .bind(color_hex)
    .bind(is_active)
    .fetch_one(pool)
    .await?;

    Ok(EnvironmentSummary {
        id: row.id,
        name: row.name,
        description: row.description,
        color_hex: row.color_hex,
        is_active: row.is_active,
        system_count: row.system_count,
    })
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
    let row = sqlx::query_as::<_, EnvironmentRow>(
        r#"
        UPDATE environments e
        SET name = $2,
            description = $3,
            color_hex = $4,
            updated_at = NOW()
        WHERE e.id = $1
        RETURNING
            e.id,
            e.name,
            e.description,
            COALESCE(e.color_hex, '#6B7280') AS color_hex,
            COALESCE(e.is_active, TRUE) AS is_active,
            (
                SELECT COUNT(s.id)::bigint
                FROM systems s
                WHERE s.environment_id = e.id
            ) AS system_count
        "#,
    )
    .bind(environment_id)
    .bind(name)
    .bind(description)
    .bind(color_hex)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| EnvironmentSummary {
        id: r.id,
        name: r.name,
        description: r.description,
        color_hex: r.color_hex,
        is_active: r.is_active,
        system_count: r.system_count,
    }))
}
