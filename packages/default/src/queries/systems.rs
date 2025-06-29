use crate::models::systems::System;
use anyhow::Result;
use sqlx::PgPool;

pub async fn update_hostname(pool: &PgPool, system: &System, new_hostname: &str) -> Result<()> {
    sqlx::query("UPDATE systems SET hostname = $1, updated_at = NOW() WHERE id = $2")
        .bind(new_hostname)
        .bind(system.id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_by_hostname(pool: &PgPool, hostname: &str) -> Result<Option<System>> {
    let system = sqlx::query_as::<_, System>("SELECT * FROM systems WHERE hostname = $1")
        .bind(hostname)
        .fetch_optional(pool)
        .await?;
    Ok(system)
}

pub async fn get_by_id(pool: &PgPool, id: i32) -> Result<Option<System>> {
    let system = sqlx::query_as::<_, System>("SELECT * FROM systems WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(system)
}
