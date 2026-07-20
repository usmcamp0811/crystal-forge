//! Reservation cleanup for the builder module.
//!
//! This module handles periodic cleanup of stale build reservations to prevent
//! deadlocks and resource leaks when workers crash or hang.

use crate::queries::build_reservations;
use sqlx::PgPool;
use tracing::{error, info, warn};

/// Runs periodic cleanup of stale build reservations.
///
/// This background loop reclaims reservations that have been held for too long,
/// preventing deadlocks when workers crash or hang unexpectedly.
pub(super) async fn run_reservation_cleanup_loop(pool: PgPool) {
    info!("🧹 Starting reservation cleanup loop...");

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

        match build_reservations::cleanup_stale_reservations(&pool, 300).await {
            Ok(reclaimed) if !reclaimed.is_empty() => {
                warn!(
                    "🧹 Reclaimed {} stale reservations: {:?}",
                    reclaimed.len(),
                    reclaimed
                );
            }
            Err(e) => {
                error!("❌ Error cleaning up stale reservations: {}", e);
            }
            _ => {}
        }
    }
}
