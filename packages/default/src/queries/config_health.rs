//! Config health count queries for `GET /api/v1/admin/config-health`.
//!
//! All queries use simple `COUNT(*)` patterns and are intended to run
//! concurrently via [`tokio::try_join!`].

use anyhow::Result;
use sqlx::PgPool;

/// Count of configured flakes (any row in the `flakes` table).
pub async fn count_flakes(pool: &PgPool) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM flakes")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Count of active environments.
pub async fn count_environments(pool: &PgPool) -> Result<i64> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM environments WHERE is_active = true")
            .fetch_one(pool)
            .await?;
    Ok(count)
}

/// Count of registered builders (not deactivated).
pub async fn count_builders(pool: &PgPool) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM builders WHERE is_active = true")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Count of configured cache destinations.
pub async fn count_cache_destinations(pool: &PgPool) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cache_destinations")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Count of flakes whose latest commit has a non-null `evaluation_error_message`.
///
/// Uses a lateral join to select only the most recent commit per flake.
pub async fn count_flakes_with_eval_errors(pool: &PgPool) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM flakes f
        WHERE EXISTS (
            SELECT 1
            FROM commits c
            WHERE c.flake_id = f.id
              AND c.evaluation_error_message IS NOT NULL
              AND c.created_at = (
                  SELECT MAX(c2.created_at) FROM commits c2 WHERE c2.flake_id = f.id
              )
        )
        "#,
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}
