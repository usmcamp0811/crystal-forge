//! Periodic stale-state and attestation freshness reconciliation.
//!
//! Checks for systems whose latest attestation has aged beyond the evidence
//! freshness threshold and transitions their projected trust state
//! accordingly.

use crate::models::running_state_attestations::DEFAULT_EVIDENCE_FRESHNESS_SECS;
use crate::queries::attention;
use crate::queries::running_state_attestations;
use cf_protocol::attestation::TrustClassification;
use chrono::Utc;
use sqlx::PgPool;
use std::time::Duration;
use tracing::{debug, error, warn};

/// How often to check for stale attestation evidence.
const FRESHNESS_CHECK_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

/// Run the attestation freshness reconciliation loop.
pub async fn run_attestation_freshness_loop(pool: PgPool) {
    tracing::info!(
        "Starting attestation freshness loop (interval={:?})",
        FRESHNESS_CHECK_INTERVAL
    );

    let mut ticker = tokio::time::interval(FRESHNESS_CHECK_INTERVAL);
    loop {
        ticker.tick().await;
        if let Err(e) = reconcile_stale_evidence(&pool).await {
            error!("Attestation freshness sweep failed: {e:#}");
        }
        debug!("Attestation freshness sweep complete");
    }
}

async fn reconcile_stale_evidence(pool: &PgPool) -> anyhow::Result<()> {
    let threshold_secs = DEFAULT_EVIDENCE_FRESHNESS_SECS as i64;

    let stale_systems = running_state_attestations::get_systems_needing_staleness_update(
        pool,
        threshold_secs,
    )
    .await?;

    if stale_systems.is_empty() {
        return Ok(());
    }

    debug!(
        "Found {} systems with stale attestation evidence",
        stale_systems.len()
    );

    for state in &stale_systems {
        let system_id = state.system_id;
        let new_classification = if state.current_classification == "authorized_current" {
            TrustClassification::AuthorizedButEvidenceStale
        } else {
            TrustClassification::AgentAttestationStale
        };

        let mut tx = pool.begin().await?;

        if let Err(e) = running_state_attestations::upsert_system_trust_state(
            &mut tx,
            system_id,
            &new_classification.to_string(),
            "evidence_beyond_freshness_threshold",
            state.latest_attestation_id,
            state.latest_authorization_id,
            state.observed_store_path.as_deref(),
            state.expected_store_path.as_deref(),
            Some(threshold_secs + 1), // it's at least this stale
            state.investigation_id,
        )
        .await
        {
            warn!("Failed to update trust state for system {system_id}: {e:#}");
            continue;
        }

        // If the new classification is flagged, open an attention occurrence.
        if new_classification.is_flagged() {
            // Commit the trust state update first, then open attention.
            tx.commit().await?;

            if let Err(e) = attention::open_or_observe_by_subject(
                pool,
                "attestations",
                "running_state_trust",
                &system_id.to_string(),
                "evidence_stale",
                Utc::now(),
                serde_json::json!({
                    "system_id": system_id.to_string(),
                    "classification": new_classification.to_string(),
                }),
                |_subject_id, episode_id| {
                    format!("running_state_trust:{system_id}:{episode_id}")
                },
            )
            .await
            {
                warn!("Failed to open attention for stale system {system_id}: {e:#}");
            }
            continue; // already committed
        }

        tx.commit().await?;
    }

    Ok(())
}
