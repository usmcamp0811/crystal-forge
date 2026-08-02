//! Periodic expiration reconciliation for deployment approval requests.
//!
//! Expires pending requests that have passed their configured expiration
//! timestamp and resolves associated attention occurrences.

use crate::queries::attention;
use crate::queries::deployment_approval_requests;
use sqlx::PgPool;
use std::time::Duration;
use tracing::{debug, error, warn};

/// How often to check for expired approval requests.
const EXPIRATION_CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// Run the approval expiration reconciliation loop.
pub async fn run_approval_expiration_loop(pool: PgPool) {
    tracing::info!(
        "Starting approval expiration loop (interval={:?})",
        EXPIRATION_CHECK_INTERVAL
    );

    let mut ticker = tokio::time::interval(EXPIRATION_CHECK_INTERVAL);
    loop {
        ticker.tick().await;
        if let Err(e) = expire_overdue_requests(&pool).await {
            error!("Approval expiration sweep failed: {e:#}");
        }
        debug!("Approval expiration sweep complete");
    }
}

async fn expire_overdue_requests(pool: &PgPool) -> anyhow::Result<()> {
    let expired_ids = deployment_approval_requests::expire_all_overdue_requests(pool).await?;

    if expired_ids.is_empty() {
        return Ok(());
    }

    debug!(
        "Expired {} approval requests: {:?}",
        expired_ids.len(),
        expired_ids
    );

    // Resolve attention occurrences for expired requests
    for request_id in &expired_ids {
        if let Err(e) = attention::resolve_open_occurrences_for_subject(
            pool,
            "approvals",
            &request_id.to_string(),
        )
        .await
        {
            warn!(
                "Failed to resolve attention for expired request {}: {e:#}",
                request_id
            );
        }
    }

    Ok(())
}
