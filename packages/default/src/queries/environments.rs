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
