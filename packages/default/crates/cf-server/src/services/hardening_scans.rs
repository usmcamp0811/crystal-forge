//! Service for triggering and managing hardening scans.

use anyhow::Result;
use sqlx::PgPool;
use tokio::time::{Duration, MissedTickBehavior};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::derivations::utils::build_flake_reference;
use crate::hardening::scanner::HardeningScanner;
use crate::models::evaluate_with_policies::HEAVY_NIX_ADVISORY_LOCK;
use crate::queries::hardening_scans::{
    ClaimedHardeningScan, claim_next_hardening_scan, create_hardening_scan,
    get_active_scan_for_derivation, hardening_queue_depth, list_commit_hardening_targets,
    mark_scan_failed, persist_completed_hardening_scan, recover_stale_hardening_scans,
};

const HARDENING_QUEUE_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Domain errors for hardening scan operations.
#[derive(Debug)]
pub enum HardeningScanError {
    /// Scan is already in progress for this derivation.
    ScanAlreadyActive(Uuid),
    /// Derivation not found or not eligible for scanning.
    DerivationNotEligible(String),
    /// Nix evaluation failed.
    NixEvalFailed(String),
    /// Any other unexpected failure.
    Internal(anyhow::Error),
}

impl std::fmt::Display for HardeningScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HardeningScanError::ScanAlreadyActive(id) => {
                write!(f, "A hardening scan is already active: {id}")
            }
            HardeningScanError::DerivationNotEligible(reason) => {
                write!(f, "Derivation not eligible for hardening scan: {reason}")
            }
            HardeningScanError::NixEvalFailed(err) => {
                write!(f, "Nix evaluation failed: {err}")
            }
            HardeningScanError::Internal(err) => write!(f, "{err:#}"),
        }
    }
}

impl From<anyhow::Error> for HardeningScanError {
    fn from(err: anyhow::Error) -> Self {
        HardeningScanError::Internal(err)
    }
}

/// Trigger an immediate hardening scan for a derivation.
///
/// Returns the scan ID. If a scan is already active, returns the existing scan ID.
pub async fn trigger_immediate_hardening_scan(
    pool: PgPool,
    derivation_id: i32,
    _flake_ref: &str,
    config_name: &str,
) -> Result<Uuid, HardeningScanError> {
    // Check for existing active scan
    if let Some(existing_scan_id) = get_active_scan_for_derivation(&pool, derivation_id)
        .await
        .map_err(HardeningScanError::Internal)?
    {
        info!(
            "Returning existing active hardening scan {} for derivation {}",
            existing_scan_id, derivation_id
        );
        return Ok(existing_scan_id);
    }

    // Create new scan record
    let scan_id = create_hardening_scan(&pool, derivation_id)
        .await
        .map_err(HardeningScanError::Internal)?;

    info!(
        "Created hardening scan {} for derivation {} ({})",
        scan_id, derivation_id, config_name
    );

    Ok(scan_id)
}

/// Run one scan already atomically claimed by the durable queue worker.
async fn run_hardening_scan(pool: &PgPool, claimed: ClaimedHardeningScan) -> Result<()> {
    let scan_id = claimed.id;
    let config_name = claimed.config_name;
    let flake_ref = build_flake_reference(&claimed.repo_url, &claimed.commit_hash);
    debug!(
        scan_id = %scan_id,
        derivation_id = claimed.derivation_id,
        config_name = %config_name,
        attempt = claimed.attempts,
        "hardening_scan_started"
    );

    let start_time = std::time::Instant::now();
    let scanner = HardeningScanner::new();
    let mut heavy_nix_db_lock = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(HEAVY_NIX_ADVISORY_LOCK)
        .execute(&mut *heavy_nix_db_lock)
        .await?;

    // Run the scan
    let scan_result = match scanner.scan_config(&flake_ref, &config_name).await {
        Ok(result) => result,
        Err(err) => {
            let error_message = format!("{err:#}");
            mark_scan_failed(pool, scan_id, &error_message).await?;
            return Err(err);
        }
    };
    heavy_nix_db_lock.commit().await?;

    let scan_duration_ms = start_time.elapsed().as_millis() as i32;
    let persistence_started = std::time::Instant::now();
    let persist_result =
        persist_completed_hardening_scan(pool, scan_id, &scan_result, scan_duration_ms).await;

    if let Err(err) = persist_result {
        let error_message = format!("{err:#}");
        mark_scan_failed(pool, scan_id, &error_message).await?;
        return Err(err);
    }

    info!(
        scan_id = %scan_id,
        derivation_id = claimed.derivation_id,
        config_name = %config_name,
        attempt = claimed.attempts,
        duration_ms = scan_duration_ms,
        service_count = scan_result.total_services,
        persistence_duration_ms = persistence_started.elapsed().as_millis() as u64,
        "hardening_scan_completed"
    );

    Ok(())
}

/// Run the durable hardening queue with exactly one awaited scan at a time.
/// Correctness depends on PostgreSQL state; polling ensures queued work
/// survives process restarts without an in-memory notification fan-out.
pub async fn run_hardening_scan_queue(pool: PgPool) {
    info!("Starting serial hardening scan queue worker");

    match recover_stale_hardening_scans(&pool).await {
        Ok(recovered) if recovered > 0 => {
            warn!(recovered, "hardening_stale_scans_recovered");
        }
        Ok(_) => {}
        Err(error) => warn!(%error, "hardening_stale_scan_recovery_failed"),
    }

    let mut ticker = tokio::time::interval(HARDENING_QUEUE_POLL_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;

        if let Ok(queue_depth) = hardening_queue_depth(&pool).await {
            debug!(
                queue_depth,
                active_hardening_scans = 0,
                "hardening_queue_state"
            );
        }

        match claim_next_hardening_scan(&pool).await {
            Ok(Some(claimed)) => {
                if let Err(error) = run_hardening_scan(&pool, claimed.clone()).await {
                    error!(
                        scan_id = %claimed.id,
                        derivation_id = claimed.derivation_id,
                        attempt = claimed.attempts,
                        %error,
                        "hardening_scan_failed"
                    );
                }
            }
            Ok(None) => {}
            Err(error) => warn!(%error, "hardening_scan_claim_failed"),
        }

        if let Err(error) = recover_stale_hardening_scans(&pool).await {
            warn!(%error, "hardening_stale_scan_recovery_failed");
        }
    }
}

/// Trigger a hardening scan for a system by system ID.
pub async fn trigger_system_hardening_scan(
    pool: PgPool,
    system_id: Uuid,
) -> Result<Uuid, HardeningScanError> {
    use crate::queries::hardening_scans::resolve_system_hardening_scan_target;

    // Resolve the system to a derivation
    let target = resolve_system_hardening_scan_target(&pool, system_id)
        .await
        .map_err(HardeningScanError::Internal)?
        .ok_or_else(|| {
            HardeningScanError::DerivationNotEligible("System not found or has no builds".into())
        })?;

    if let Some(reason) = target.blocked_reason {
        return Err(HardeningScanError::DerivationNotEligible(reason));
    }

    let flake_ref = build_flake_reference(&target.repo_url, &target.commit_hash);

    trigger_immediate_hardening_scan(pool, target.derivation_id, &flake_ref, &target.config_name)
        .await
}

/// Queue hardening scans for all NixOS derivations in a commit.
pub async fn trigger_commit_hardening_scans(
    pool: PgPool,
    commit_id: i32,
    repo_url: &str,
    commit_hash: &str,
) -> Result<usize, HardeningScanError> {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    let targets = list_commit_hardening_targets(&pool, commit_id)
        .await
        .map_err(HardeningScanError::Internal)?;

    let mut queued = 0usize;
    for target in targets {
        match trigger_immediate_hardening_scan(
            pool.clone(),
            target.derivation_id,
            &flake_ref,
            &target.config_name,
        )
        .await
        {
            Ok(_) => queued += 1,
            Err(HardeningScanError::ScanAlreadyActive(_)) => {}
            Err(err) => {
                error!(
                    "Failed to queue hardening scan for derivation {} ({}): {}",
                    target.derivation_id, target.config_name, err
                );
            }
        }
    }

    Ok(queued)
}
