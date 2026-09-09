use crate::models::systems::System;
use anyhow::Result;
use sqlx::PgPool;

const AUTO_LATEST_SYSTEMS_QUERY: &str = r#"
        SELECT
            id,
            hostname,
            environment_id,
            is_active,
            public_key,
            flake_id,
            derivation,
            system_configuration_name,
            created_at,
            updated_at,
            desired_target,
            deployment_policy
        FROM systems
        WHERE deployment_policy = 'auto_latest'
        AND is_active = true
        ORDER BY hostname
        "#;

/// Get all systems that have deployment_policy set to 'auto_latest'
pub async fn get_systems_with_auto_latest_policy(pool: &PgPool) -> Result<Vec<System>> {
    let systems = sqlx::query_as::<_, System>(AUTO_LATEST_SYSTEMS_QUERY)
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
    update_desired_target_with_source(pool, hostname, desired_target, "auto_desired_target").await
}

pub async fn update_desired_target_with_source(
    pool: &PgPool,
    hostname: &str,
    desired_target: Option<&str>,
    source: &str,
) -> Result<()> {
    if let Some(target) = desired_target {
        let system_id =
            sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM systems WHERE hostname = $1")
                .bind(hostname)
                .fetch_optional(pool)
                .await?;
        let Some(system_id) = system_id else {
            return Ok(());
        };
        let authorization =
            crate::services::composite_enforcement::authorize_and_set_system_target(
                pool, system_id, target, source,
            )
            .await?;
        if !authorization.allowed() {
            anyhow::bail!(authorization.detail);
        }
        return Ok(());
    }

    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        UPDATE systems
        SET desired_target = $1,
            desired_target_set_at = CASE WHEN $1::text IS NULL THEN NULL ELSE NOW() END,
            updated_at = NOW()
        WHERE hostname = $2
        "#,
    )
    .bind(desired_target)
    .bind(hostname)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

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

#[cfg(test)]
mod tests {
    use super::AUTO_LATEST_SYSTEMS_QUERY;

    #[test]
    fn auto_latest_query_selects_system_configuration_name() {
        assert!(AUTO_LATEST_SYSTEMS_QUERY.contains("system_configuration_name"));
    }
}
