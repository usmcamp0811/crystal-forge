use crate::models::System;
use crate::models::system_states::SystemState;
use anyhow::{Context, Result};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub async fn update_hostname(&self, pool: &PgPool, new_hostname: &str) -> Result<()> {
    sqlx::query("UPDATE tbl_systems SET hostname = $1, updated_at = NOW() WHERE id = $2")
        .bind(new_hostname)
        .bind(self.id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_by_hostname(pool: &PgPool, hostname: &str) -> Result<Option<System>> {
    let system = sqlx::query_as::<_, System>("SELECT * FROM tbl_systems WHERE hostname = $1")
        .bind(hostname)
        .fetch_optional(pool)
        .await?;
    Ok(system)
}

pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<System>> {
    let system = sqlx::query_as::<_, System>("SELECT * FROM tbl_systems WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(system)
}
