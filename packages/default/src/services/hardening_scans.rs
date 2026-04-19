//! Service for triggering and managing hardening scans.

use anyhow::Result;
use sqlx::PgPool;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::derivations::utils::build_flake_reference;
use crate::hardening::scanner::HardeningScanner;
use crate::queries::hardening_scans::{
    complete_hardening_scan, create_hardening_scan, get_active_scan_for_derivation,
    insert_service_result, list_commit_hardening_targets, mark_scan_failed, mark_scan_in_progress,
};

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
    flake_ref: &str,
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

    // Spawn background task to run the scan
    let spawn_pool = pool.clone();
    let flake_ref = flake_ref.to_string();
    let config_name = config_name.to_string();

    tokio::spawn(async move {
        let result = run_hardening_scan(&spawn_pool, scan_id, &flake_ref, &config_name).await;
        if let Err(err) = result {
            error!(
                "Hardening scan {} failed for {}: {:#}",
                scan_id, config_name, err
            );
        }
    });

    Ok(scan_id)
}

/// Run a hardening scan (called from background task).
async fn run_hardening_scan(
    pool: &PgPool,
    scan_id: Uuid,
    flake_ref: &str,
    config_name: &str,
) -> Result<()> {
    debug!(
        "Starting hardening scan {} for {} at {}",
        scan_id, config_name, flake_ref
    );

    // Mark scan as in progress
    mark_scan_in_progress(pool, scan_id).await?;

    let start_time = std::time::Instant::now();
    let scanner = HardeningScanner::new();

    // Run the scan
    match scanner.scan_config(flake_ref, config_name).await {
        Ok(scan_result) => {
            let scan_duration_ms = start_time.elapsed().as_millis() as i32;

            // Insert service results
            for service in &scan_result.services {
                let directives_json = serde_json::to_value(&service.score_result.directives)?;

                insert_service_result(
                    pool,
                    scan_id,
                    &service.name,
                    service.service_type.as_deref(),
                    service.score_result.score,
                    service.score_result.risk_level,
                    directives_json,
                    service.score_result.enabled_count,
                    service.score_result.disabled_count,
                    service.score_result.missing_count,
                )
                .await?;
            }

            // Complete the scan
            complete_hardening_scan(
                pool,
                scan_id,
                scan_result.total_services,
                scan_result.well_hardened_count,
                scan_result.moderately_hardened_count,
                scan_result.poorly_hardened_count,
                scan_result.vulnerable_count,
                scan_result.overall_score,
                Some(scan_duration_ms),
            )
            .await?;

            info!(
                "Completed hardening scan {} for {}: {} services, overall score {}",
                scan_id,
                config_name,
                scan_result.total_services,
                scan_result
                    .overall_score
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            );
        }
        Err(err) => {
            let error_message = format!("{err:#}");
            mark_scan_failed(pool, scan_id, &error_message).await?;
            return Err(err);
        }
    }

    Ok(())
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
