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

pub async fn insert_system(pool: &PgPool, system: &System) -> Result<System> {
    let inserted = sqlx::query_as::<_, System>(
        r#"
    INSERT INTO systems (
        hostname,
        environment_id,
        is_active,
        public_key,
        flake_id,
        derivation,
        created_at,
        updated_at
    )
    VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
    ON CONFLICT (hostname) DO UPDATE SET
        environment_id = EXCLUDED.environment_id,
        is_active = EXCLUDED.is_active,
        public_key = EXCLUDED.public_key,
        flake_id = EXCLUDED.flake_id,
        derivation = EXCLUDED.derivation,
        updated_at = NOW()
    RETURNING *
    "#,
    )
    .bind(&system.hostname)
    .bind(system.environment_id)
    .bind(system.is_active)
    .bind(&system.public_key.to_base64())
    .bind(system.flake_id)
    .bind(&system.derivation)
    .fetch_one(pool)
    .await?;
    Ok(inserted)
}
