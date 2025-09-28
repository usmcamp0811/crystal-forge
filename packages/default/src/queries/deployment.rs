use crate::models::systems::System;
use anyhow::Result;
use sqlx::PgPool;

/// Get all systems that have deployment_policy set to 'auto_latest'
pub async fn get_systems_with_auto_latest_policy(pool: &PgPool) -> Result<Vec<System>> {
    let systems = sqlx::query_as::<_, System>(
        r#"
        SELECT 
            id,
            hostname,
            environment_id,
            is_active,
            public_key,
            flake_id,
            derivation,
            created_at,
            updated_at,
            desired_target,
            deployment_policy
        FROM systems 
        WHERE deployment_policy = 'auto_latest' 
        AND is_active = true
        ORDER BY hostname
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(systems)
}

/// Update the desired_target for a system by hostname
pub async fn update_desired_target(
    pool: &PgPool,
    hostname: &str,
    desired_target: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE systems 
        SET desired_target = $1, updated_at = NOW() 
        WHERE hostname = $2
        "#,
    )
    .bind(desired_target)
    .bind(hostname)
    .execute(pool)
    .await?;

    Ok(())
}

/// Update the deployment policy for a system by hostname
pub async fn update_deployment_policy(pool: &PgPool, hostname: &str, policy: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE systems 
        SET deployment_policy = $1, updated_at = NOW() 
        WHERE hostname = $2
        "#,
    )
    .bind(policy)
    .bind(hostname)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get systems by deployment policy
pub async fn get_systems_by_deployment_policy(pool: &PgPool, policy: &str) -> Result<Vec<System>> {
    let systems = sqlx::query_as::<_, System>(
        r#"
        SELECT 
            id,
            hostname,
            environment_id,
            is_active,
            public_key,
            flake_id,
            derivation,
            created_at,
            updated_at,
            desired_target,
            deployment_policy
        FROM systems 
        WHERE deployment_policy = $1 
        AND is_active = true
        ORDER BY hostname
        "#,
    )
    .bind(policy)
    .fetch_all(pool)
    .await?;

    Ok(systems)
}
