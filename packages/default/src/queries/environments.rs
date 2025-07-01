use crate::models::config::EnvironmentConfig;
use crate::models::environments::Environment;
use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

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
