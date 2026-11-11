use crate::models::systems::{ManagementMode, System};
use anyhow::Result;
use sqlx::PgPool;

const AUTO_LATEST_SYSTEMS_QUERY: &str = r#"
        SELECT
            id,
            hostname,
            environment_id,
            is_active,
            management_mode,
            parent_host_id,
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
        AND management_mode = 'managed'
        ORDER BY hostname
        "#;

/// Returns active managed systems with the `auto_latest` deployment policy.
///
/// # Errors
/// Returns database or system decoding errors.
pub async fn get_systems_with_auto_latest_policy(pool: &PgPool) -> Result<Vec<System>> {
    let systems = sqlx::query_as::<_, System>(AUTO_LATEST_SYSTEMS_QUERY)
        .fetch_all(pool)
        .await?;

    Ok(systems)
}

/// Updates a system's desired target through locked deployment authorization.
///
/// Clearing a target remains permitted for all modes.
///
/// # Errors
/// Returns an error if deployment authority or target evidence is insufficient,
/// or if the database operation fails.
pub async fn update_desired_target(
    pool: &PgPool,
    hostname: &str,
    desired_target: Option<&str>,
) -> Result<()> {
    update_desired_target_with_source(pool, hostname, desired_target, "auto_desired_target").await
}

/// Updates a desired target and records the supplied deployment source.
///
/// Nonmanaged and inactive systems cannot receive targets. Clearing a target
/// remains permitted to revoke deployment intent.
///
/// # Errors
/// Returns authorization, evidence, or database errors without setting a target.
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

/// Updates the deployment policy of a managed system by hostname.
///
/// This operation does not change management mode or queue deployment work.
///
/// # Errors
/// Returns an error for a nonmanaged system or a database failure.
pub async fn update_deployment_policy(pool: &PgPool, hostname: &str, policy: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    // CONCURRENCY: Serialize with mode transitions before changing policy.
    let mode = sqlx::query_scalar::<_, ManagementMode>(
        "SELECT management_mode FROM systems WHERE hostname = $1 FOR UPDATE",
    )
    .bind(hostname)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(mode) = mode else {
        return Ok(());
    };
    anyhow::ensure!(
        mode.permits_deployment(),
        "Management mode {mode} does not permit deployment policy changes"
    );
    sqlx::query(
        r#"
        UPDATE systems 
        SET deployment_policy = $1, updated_at = NOW() 
        WHERE hostname = $2
        "#,
    )
    .bind(policy)
    .bind(hostname)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(())
}

/// Returns active managed systems with the requested deployment policy.
///
/// # Errors
/// Returns database or system decoding errors.
pub async fn get_systems_by_deployment_policy(pool: &PgPool, policy: &str) -> Result<Vec<System>> {
    let systems = sqlx::query_as::<_, System>(
        r#"
        SELECT 
            id,
            hostname,
            environment_id,
            is_active,
            management_mode,
            parent_host_id,
            public_key,
            flake_id,
            derivation,
            system_configuration_name,
            created_at,
            updated_at,
            desired_target,
            deployment_policy
        FROM systems 
        WHERE deployment_policy = $1 
        AND is_active = true
        AND management_mode = 'managed'
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
