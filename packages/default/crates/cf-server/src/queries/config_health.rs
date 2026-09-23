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
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM builders WHERE status = 'active'")
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

/// Counts flakes whose authoritative latest commit has an evaluation error.
///
/// Commit timestamp ties are resolved by descending commit ID, matching the
/// ordering used by evaluation history. An older failed commit MUST NOT keep a
/// flake unhealthy after a newer successful commit.
pub async fn count_flakes_with_eval_errors(pool: &PgPool) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM flakes f
        WHERE (
            SELECT c.evaluation_error_message IS NOT NULL
            FROM commits c
            WHERE c.flake_id = f.id
            ORDER BY c.commit_timestamp DESC, c.id DESC
            LIMIT 1
        ) IS TRUE
        "#,
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn eval_error_count_uses_the_newest_commit_when_timestamps_tie(pool: PgPool) {
        let flake_id: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("config-health-{}", uuid::Uuid::new_v4()))
        .bind("https://example.invalid/config-health.git")
        .fetch_one(&pool)
        .await
        .unwrap();
        let commit_time = chrono::Utc::now();

        sqlx::query(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, evaluation_enqueued_at, evaluation_status, evaluation_error_message) \
             VALUES ($1, $2, $3, $3, 'failed', 'historical failure')",
        )
        .bind(flake_id)
        .bind(format!("failed-{}", uuid::Uuid::new_v4()))
        .bind(commit_time)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, evaluation_enqueued_at, evaluation_status, evaluation_error_message) \
             VALUES ($1, $2, $3, $3, 'complete', NULL)",
        )
        .bind(flake_id)
        .bind(format!("complete-{}", uuid::Uuid::new_v4()))
        .bind(commit_time)
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            super::count_flakes_with_eval_errors(&pool).await.unwrap(),
            0
        );
    }
}
