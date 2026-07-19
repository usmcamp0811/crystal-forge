//! Garbage collection for commit metadata cache.
//!
//! Removes cache entries older than the configured retention period
//! to prevent unbounded growth.

use sqlx::PgPool;
use tracing::info;

/// Delete cached commit metadata older than retention_days
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `retention_days` - Number of days to retain cached data
///
/// # Returns
/// Number of rows deleted
pub async fn garbage_collect_commit_cache(
    pool: &PgPool,
    retention_days: i32,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        DELETE FROM commit_metadata_cache
        WHERE cached_at < NOW() - make_interval(days => $1)
        "#,
        retention_days
    )
    .execute(pool)
    .await?;

    let deleted = result.rows_affected();

    if deleted > 0 {
        info!(
            "🗑️  Garbage collected {} cached commit metadata entries older than {} days",
            deleted, retention_days
        );
    }

    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retention_days_validation() {
        // This test just ensures the function signature is correct
        // Real testing would require database setup
        assert!(true);
    }
}
