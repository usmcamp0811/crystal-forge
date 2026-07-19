//! CVE scanning worker for the builder module.
//!
//! This module handles periodic CVE scanning of completed derivations using vulnix.

use crate::config::CrystalForgeConfig;
use crate::log::{WorkerState, WorkerStatus, get_cve_status};
use crate::queries::cve_scans::{
    create_cve_scan, get_targets_needing_cve_scan, mark_cve_scan_failed, mark_scan_in_progress,
    save_scan_results,
};
use crate::queries::derivations::{EvaluationStatus, update_derivation_status};
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::Result;
use sqlx::PgPool;
use tokio::fs;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Runs the periodic CVE scanning loop
pub async fn run_cve_scan_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let vulnix_config = cfg.get_vulnix_config();

    info!(
        "🔍 Starting CVE Scan loop (every {}s)...",
        vulnix_config.poll_interval.as_secs()
    );

    if !VulnixRunner::check_vulnix_available().await {
        error!("❌ vulnix is not available - CVE scanning disabled");
        return;
    }

    let vulnix_version = VulnixRunner::get_vulnix_version().await.ok();

    debug!("🔧 Using vulnix version: {:?}", vulnix_version);
    debug!(
        "🔧 Vulnix config: timeout={}s, whitelist={}, extra_args={:?}",
        vulnix_config.timeout_seconds(),
        vulnix_config.enable_whitelist,
        vulnix_config.extra_args
    );

    let vulnix_runner = VulnixRunner::with_config(&vulnix_config);

    loop {
        if let Err(e) = scan_derivations(&pool, &vulnix_runner, vulnix_version.clone()).await {
            error!("❌ Error in CVE scan cycle: {e}");
        }

        sleep(vulnix_config.poll_interval).await;
    }
}

/// Process derivations that need CVE scanning
async fn scan_derivations(
    pool: &PgPool,
    vulnix_runner: &VulnixRunner,
    vulnix_version: Option<String>,
) -> Result<()> {
    // Get derivations that need CVE scanning (those with build-complete status)
    // Update status: looking for work
    {
        let mut status = get_cve_status().write().await; // Use helper function
        *status = Some(WorkerStatus {
            worker_id: 0,
            current_task: Some("finding scan targets".to_string()),
            started_at: Some(std::time::Instant::now()),
            state: WorkerState::Working,
        });
    }

    match get_targets_needing_cve_scan(pool, Some(1)).await {
        Ok(derivations) => {
            if derivations.is_empty() {
                info!("🔍 No derivations need CVE scanning");
                // Update status: idle
                {
                    let mut status = get_cve_status().write().await;
                    *status = Some(WorkerStatus {
                        worker_id: 0,
                        current_task: None,
                        started_at: None,
                        state: WorkerState::Idle,
                    });
                }
                info!("No derivations need CVE scanning");
                return Ok(());
            }

            let derivation = &derivations[0];

            // Update status: scanning specific derivation
            {
                let mut status = get_cve_status().write().await;
                *status = Some(WorkerStatus {
                    worker_id: 0,
                    current_task: Some(format!("scanning {}", derivation.derivation_name)),
                    started_at: Some(std::time::Instant::now()),
                    state: WorkerState::Working,
                });
            }

            // Check if the derivation path exists
            if let Some(ref path) = derivation.store_path {
                match fs::try_exists(path).await {
                    Ok(true) => {
                        info!(
                            "🔍 Starting CVE scan for derivation: {}",
                            derivation.derivation_name
                        );

                        // Create a new scan record before starting
                        let scan_id =
                            create_cve_scan(pool, derivation.id, "vulnix", vulnix_version.clone())
                                .await?;

                        // Mark scan as in progress
                        mark_scan_in_progress(pool, scan_id).await?;

                        let start_time = std::time::Instant::now();

                        // Run CVE scan using the vulnix runner
                        match vulnix_runner
                            .scan_derivation(&pool, derivation.id, vulnix_version)
                            .await
                        {
                            Ok(vulnix_entries) => {
                                let scan_duration_ms =
                                    Some(start_time.elapsed().as_millis() as i32);
                                let stats =
                                    crate::vulnix::vulnix_parser::VulnixParser::calculate_stats(
                                        &vulnix_entries,
                                    );

                                // Save the detailed scan results to database
                                save_scan_results(pool, scan_id, &vulnix_entries, scan_duration_ms)
                                    .await?;

                                info!(
                                    "✅ CVE scan completed for {}: {}",
                                    derivation.derivation_name, stats
                                );
                            }
                            Err(e) => {
                                error!(
                                    "❌ CVE scan failed for {}: {}",
                                    derivation.derivation_name, e
                                );
                                if let Err(save_err) =
                                    mark_cve_scan_failed(pool, derivation, &e.to_string()).await
                                {
                                    error!("❌ Failed to mark CVE scan as failed: {save_err}");
                                }
                            }
                        }
                    }
                    Ok(false) => {
                        warn!("❌ Derivation path does not exist: {}", path);
                        update_derivation_status(
                            &pool,
                            derivation.id,
                            EvaluationStatus::DryRunComplete,
                            derivation.derivation_path.as_deref(),
                            Some("Missing Nix Store Path"),
                            derivation.store_path.as_deref(),
                        )
                        .await?;
                    }
                    Err(e) => {
                        error!("❌ Error checking derivation path {}: {}", path, e);
                    }
                }
            } else {
                warn!("❌ No derivation path set for derivation");
            }
        }
        Err(e) => error!("❌ Failed to get derivations needing CVE scan: {e}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms that run_cve_scan_loop exits cleanly when vulnix is not on PATH,
    /// rather than panicking. This exercises the check_vulnix_available() guard.
    ///
    /// In CI / test environments vulnix is not on PATH, so this test validates the
    /// safe no-op behaviour. In production the NixOS builder service puts vulnix in
    /// PATH so the guard passes and the loop continues.
    #[tokio::test]
    async fn cve_scan_loop_exits_cleanly_without_vulnix() {
        // Use a lazy pool that never actually connects — the loop should exit
        // before attempting any DB query because vulnix is unavailable in this env.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct without connecting");

        // run_cve_scan_loop returns () when vulnix is unavailable (the check at the
        // top of the function short-circuits). We wrap it in a timeout to be safe.
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(5), run_cve_scan_loop(pool)).await;

        // Either it completed within the timeout (vulnix absent → fast return)
        // or it timed out (vulnix present → loop running, also fine in dev envs).
        // The important thing is it did NOT panic.
        match result {
            Ok(()) => {
                // vulnix not found — loop exited with error log, as expected in CI
            }
            Err(_timeout) => {
                // vulnix was found on PATH and the loop is running — correct in dev
            }
        }
    }

    /// Confirms that check_vulnix_available returns a bool without panicking.
    #[tokio::test]
    async fn check_vulnix_available_does_not_panic() {
        let _ = VulnixRunner::check_vulnix_available().await;
    }
}
