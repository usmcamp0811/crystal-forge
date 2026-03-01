//! Database test utilities.

use sqlx::{PgPool, postgres::PgPoolOptions};

/// Create a test database pool (lazy connection).
///
/// This does not immediately connect, but constructs a pool
/// that will connect when first queried.
pub async fn test_pool() -> PgPool {
    PgPoolOptions::new()
        .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
        .expect("lazy pool should construct")
}
