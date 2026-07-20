//! Status / health-check database queries.
//!
//! Used by the `/status` endpoint to verify database connectivity and
//! report basic fleet statistics.

use sqlx::PgPool;

/// Run a trivial query to verify the database connection is alive.
/// Returns `true` if healthy, `false` on any error.
pub async fn check_database_health(pool: &PgPool) -> bool {
    sqlx::query("SELECT 1 as health_check")
        .fetch_one(pool)
        .await
        .is_ok()
}

/// Fetch basic fleet statistics: (total_systems, total_derivations, pending_evaluations).
pub async fn get_basic_stats(pool: &PgPool) -> (i64, i64, i64) {
    let systems_count = sqlx::query_scalar!("SELECT COUNT(*) FROM systems")
        .fetch_one(pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

    let derivations_count = sqlx::query_scalar!("SELECT COUNT(*) FROM derivations")
        .fetch_one(pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

    let pending_count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM derivations d 
         JOIN derivation_statuses ds ON d.status_id = ds.id 
         WHERE ds.is_terminal = false"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    (systems_count, derivations_count, pending_count)
}
