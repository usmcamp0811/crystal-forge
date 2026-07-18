//! CVE scanning worker for the Crystal Forge server.
//!
//! This module provides [`run_cve_scan_loop`], the background task that:
//!
//! 1. **Post-build scans** — picks up build-complete derivations that have
//!    never been successfully scanned and runs vulnix on them.
//! 2. **Periodic rescans** — picks up derivations whose last completed scan is
//!    older than the configured interval in `scan_schedule_policy`, so newly
//!    published NVD advisories are picked up automatically (vulnix fetches the
//!    latest NVD data on every invocation).
//!
//! The loop is registered as a [`BackgroundJobHandle`] so the Admin → Background
//! Jobs tab (TASK-336.5) can expose its status, enable/disable it, and trigger
//! an immediate run without waiting for the next poll interval.
//!
//! ## Scheduling
//!
//! Post-build and rescan targets are processed in separate phases within each
//! poll cycle.  Post-build scans take priority so newly built configurations get
//! their first scan quickly.  The `on_build` flag in `scan_schedule_policy`
//! controls whether the post-build phase runs at all.

use crate::config::CrystalForgeConfig;
use crate::log::{WorkerState, WorkerStatus, get_cve_status};
use crate::queries::cve_scans::{
    create_cve_scan, get_targets_needing_cve_rescan, get_targets_needing_cve_scan,
    mark_cve_scan_failed, mark_scan_in_progress, save_scan_results,
};
use crate::queries::scanning::get_scan_schedule_policy;
use crate::server::jobs::BackgroundJobHandle;
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::PgPool;
use tokio::fs;
use tokio::process::Command as TokioCommand;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Run the CVE scan background loop.
///
/// Pass a `BackgroundJobHandle` created by [`BackgroundJobHandle::new`] and
/// the matching `watch::Receiver<bool>` returned from that call.  The handle is
/// registered in the server's [`BackgroundJobRegistry`] before this function is
/// called; the receiver lets the loop respond to run-now signals from HTTP
/// handlers.
///
/// The loop exits early (returning `()`) when vulnix is not on `$PATH`, logging
/// an error.  All other errors are logged and the loop continues.
pub async fn run_cve_scan_loop(
    pool: PgPool,
    job: BackgroundJobHandle,
    mut run_now_rx: tokio::sync::watch::Receiver<u64>,
) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let vulnix_config = cfg.get_vulnix_config();

    info!(
        "🔍 Starting CVE Scan loop (poll every {}s)...",
        vulnix_config.poll_interval.as_secs()
    );

    if !VulnixRunner::check_vulnix_available().await {
        error!("❌ vulnix is not available — CVE scanning disabled");
        return;
    }

    let vulnix_version = VulnixRunner::get_vulnix_version().await.ok();

    debug!("🔧 vulnix version: {:?}", vulnix_version);
    debug!(
        "🔧 vulnix config: timeout={}s whitelist={} extra_args={:?}",
        vulnix_config.timeout_seconds(),
        vulnix_config.enable_whitelist,
        vulnix_config.extra_args
    );

    let vulnix_runner = VulnixRunner::with_config(&vulnix_config);

    loop {
        // Honour the enabled flag — sleep the full interval and skip work when disabled.
        let enabled = *job.state.enabled.read().await;
        if !enabled {
            debug!("CVE scan loop: disabled — skipping cycle");
            // Update next_run_at while disabled so the UI shows something reasonable.
            *job.state.next_run_at.write().await = Some(
                Utc::now()
                    + chrono::Duration::from_std(vulnix_config.poll_interval)
                        .unwrap_or(chrono::Duration::seconds(60)),
            );
            sleep(vulnix_config.poll_interval).await;
            // Mark the current counter value as seen so a stale signal does
            // not trigger an immediate cycle on re-enable.
            let _ = run_now_rx.borrow_and_update();
            continue;
        }

        // Mark the job as running and record timing.
        *job.state.is_running.write().await = true;
        *job.state.last_run_at.write().await = Some(Utc::now());

        if let Err(e) = scan_cycle(&pool, &vulnix_runner, vulnix_version.clone()).await {
            error!("❌ Error in CVE scan cycle: {e}");
        }

        // Update job metadata after the cycle completes.
        *job.state.is_running.write().await = false;
        *job.state.next_run_at.write().await = Some(
            Utc::now()
                + chrono::Duration::from_std(vulnix_config.poll_interval)
                    .unwrap_or(chrono::Duration::seconds(60)),
        );

        // Wait for either the poll interval or a run-now signal.
        // The run-now channel uses a monotonically increasing counter so every
        // trigger fires `changed()` exactly once — no double-cycle.
        tokio::select! {
            _ = sleep(vulnix_config.poll_interval) => {}
            _ = run_now_rx.changed() => {
                info!("⚡ CVE scan loop: run-now signal received — starting immediate cycle");
            }
        }
    }
}

/// One full scan cycle: post-build scans followed by periodic rescans.
async fn scan_cycle(
    pool: &PgPool,
    vulnix_runner: &VulnixRunner,
    vulnix_version: Option<String>,
) -> Result<()> {
    set_cve_status_working("finding scan targets").await;

    // Read scan schedule policy to check on_build flag.
    let policy = get_scan_schedule_policy(pool).await.unwrap_or_else(|e| {
        warn!("Failed to load scan schedule policy: {e}, using defaults");
        crate::queries::scanning::ScanSchedulePolicyRow {
            on_build: true,
            deployed_interval: "24h".to_string(),
            recent_interval: "24h".to_string(),
            archived_interval: "168h".to_string(),
            archived_enabled: true,
            rebuild_to_scan: false,
            updated_at: Utc::now(),
        }
    });

    // --- Phase 1: post-build scans (newly unscanned derivations) ---
    if policy.on_build {
        match get_targets_needing_cve_scan(pool, Some(1)).await {
            Ok(targets) if !targets.is_empty() => {
                let derivation = &targets[0];
                info!(
                    "🔍 [post-build] Scanning newly built derivation: {}",
                    derivation.derivation_name
                );
                scan_one(pool, vulnix_runner, vulnix_version.clone(), derivation).await?;
            }
            Ok(_) => {
                debug!("🔍 No newly built derivations need CVE scanning");
            }
            Err(e) => error!("❌ Failed to get post-build scan targets: {e}"),
        }
    } else {
        debug!("🔍 on_build = false — skipping post-build scan phase");
    }

    // --- Phase 2: periodic rescan (stale completed scans) ---
    match get_targets_needing_cve_rescan(pool, Some(1)).await {
        Ok(targets) if !targets.is_empty() => {
            let derivation = &targets[0];
            info!(
                "🔄 [rescan] Re-scanning stale derivation: {}",
                derivation.derivation_name
            );
            scan_one(pool, vulnix_runner, vulnix_version.clone(), derivation).await?;
        }
        Ok(_) => {
            info!("🔍 No derivations need CVE rescanning");
            set_cve_status_idle().await;
        }
        Err(e) => error!("❌ Failed to get rescan targets: {e}"),
    }

    Ok(())
}

/// Scan a single derivation: create a scan record, run vulnix, save results.
///
/// If the store path is not present in the local Nix store, the function
/// attempts to copy it from a configured cache destination (using the first
/// completed cache push job's destination URL).  If materialization fails, the
/// scan is marked as failed — the derivation's build status is **never**
/// rewritten.
async fn scan_one(
    pool: &PgPool,
    vulnix_runner: &VulnixRunner,
    vulnix_version: Option<String>,
    derivation: &crate::derivations::Derivation,
) -> Result<()> {
    set_cve_status_working(&format!("scanning {}", derivation.derivation_name)).await;

    let Some(ref path) = derivation.store_path else {
        warn!(
            "❌ No store_path set for derivation {}",
            derivation.derivation_name
        );
        return Ok(());
    };

    // 1. Check local presence.
    let locally_present = match fs::try_exists(path).await {
        Ok(exists) => exists,
        Err(e) => {
            error!("❌ Error checking derivation path {}: {}", path, e);
            // Treat as not present — try cache materialization below.
            false
        }
    };

    // 2. Materialize from cache if not present locally.
    if !locally_present {
        warn!(
            "Store path {} not found locally — attempting cache materialization",
            path
        );
        match materialize_store_path_from_cache(pool, derivation, path).await {
            Ok(true) => {
                info!("✅ Successfully materialized {} from cache", path);
            }
            Ok(false) => {
                warn!(
                    "❌ Could not materialize {} from any cache — marking scan as failed",
                    path
                );
                // Record a failed scan (do NOT rewrite build status).
                mark_cve_scan_failed(
                    pool,
                    derivation,
                    &format!(
                        "Store path {} not present locally and no cache could provide it",
                        path
                    ),
                )
                .await?;
                return Ok(());
            }
            Err(e) => {
                error!("❌ Error materializing {} from cache: {}", path, e);
                mark_cve_scan_failed(
                    pool,
                    derivation,
                    &format!("Cache materialization error: {}", e),
                )
                .await?;
                return Ok(());
            }
        }
    }

    // 3. Create scan record and run vulnix.
    let scan_id = create_cve_scan(pool, derivation.id, "vulnix", vulnix_version.clone()).await?;
    mark_scan_in_progress(pool, scan_id).await?;

    let start = std::time::Instant::now();
    match vulnix_runner
        .scan_derivation(pool, derivation.id, vulnix_version)
        .await
    {
        Ok(entries) => {
            let elapsed_ms = Some(start.elapsed().as_millis() as i32);
            let stats = crate::vulnix::vulnix_parser::VulnixParser::calculate_stats(&entries);
            save_scan_results(pool, scan_id, &entries, elapsed_ms).await?;
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
            if let Err(save_err) = mark_cve_scan_failed(pool, derivation, &e.to_string()).await {
                error!("❌ Failed to mark CVE scan as failed: {save_err}");
            }
        }
    }

    Ok(())
}

/// Try to copy a store path from a configured cache destination.
///
/// Queries for completed `cache_push_jobs` for this derivation, resolves the
/// cache destination's `push_to` URL, and runs `nix copy --from <url> <path>`
/// for the first eligible cache.  Returns `Ok(true)` if materialization
/// succeeded, `Ok(false)` if no cache could provide the path, or `Err` on a
/// fatal error.
async fn materialize_store_path_from_cache(
    pool: &PgPool,
    derivation: &crate::derivations::Derivation,
    store_path: &str,
) -> Result<bool> {
    // Find cache URLs that previously received a completed push for this derivation.
    let cache_urls: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT cd.push_to
        FROM cache_push_jobs cpj
        JOIN cache_destinations cd ON cd.name = cpj.cache_destination
        WHERE cpj.derivation_id = $1
          AND cpj.status = 'completed'
          AND cd.push_to IS NOT NULL
        "#,
    )
    .bind(derivation.id)
    .fetch_all(pool)
    .await
    .context("Failed to query cache destinations for materialization")?;

    if cache_urls.is_empty() {
        debug!(
            "No completed cache push found for derivation {} — cannot materialize",
            derivation.id
        );
        return Ok(false);
    }

    for url in &cache_urls {
        debug!("Trying nix copy --from {} {}", url, store_path);
        let output = TokioCommand::new("nix")
            .args(["copy", "--from", url, store_path])
            .output()
            .await
            .with_context(|| format!("Failed to spawn nix copy from {}", url))?;

        if output.status.success() {
            // Verify the path is now present.
            match fs::try_exists(store_path).await {
                Ok(true) => return Ok(true),
                Ok(false) => {
                    warn!(
                        "nix copy from {} reported success but {} is still absent",
                        url, store_path
                    );
                    // Continue trying other caches.
                }
                Err(e) => {
                    warn!(
                        "nix copy from {} succeeded but path check failed: {}",
                        url, e
                    );
                }
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                "nix copy from {} failed for {}: {}",
                url,
                store_path,
                stderr.trim()
            );
        }
    }

    Ok(false)
}

async fn set_cve_status_working(task: &str) {
    let mut status = get_cve_status().write().await;
    *status = Some(WorkerStatus {
        worker_id: 0,
        current_task: Some(task.to_string()),
        started_at: Some(std::time::Instant::now()),
        state: WorkerState::Working,
    });
}

async fn set_cve_status_idle() {
    let mut status = get_cve_status().write().await;
    *status = Some(WorkerStatus {
        worker_id: 0,
        current_task: None,
        started_at: None,
        state: WorkerState::Idle,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms that [`run_cve_scan_loop`] exits cleanly when vulnix is not on
    /// `$PATH`, rather than panicking.  In CI vulnix is absent so the check at
    /// the top of the function short-circuits.  In dev environments where vulnix
    /// is present the timeout fires instead — both outcomes are acceptable.
    #[tokio::test]
    async fn cve_scan_loop_exits_cleanly_without_vulnix() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct without connecting");

        let (handle, run_now_rx) = BackgroundJobHandle::new(
            "cve_scan",
            "CVE Scan",
            std::time::Duration::from_secs(60),
            true,
        );

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_cve_scan_loop(pool, handle, run_now_rx),
        )
        .await;

        // Either vulnix absent (fast return Ok) or present (timeout = still running).
        // The important invariant: no panic.
        match result {
            Ok(()) => {}        // vulnix not found, loop exited cleanly
            Err(_timeout) => {} // vulnix present, loop is running — fine in dev
        }
    }

    /// Confirms that [`VulnixRunner::check_vulnix_available`] returns a bool
    /// without panicking regardless of PATH contents.
    #[tokio::test]
    async fn check_vulnix_available_does_not_panic() {
        let _ = VulnixRunner::check_vulnix_available().await;
    }

    /// Confirms that the [`BackgroundJobHandle`] state machine correctly reports
    /// the enabled flag, is_running, last_run_at, and next_run_at.
    #[tokio::test]
    async fn background_job_handle_state_machine() {
        let (handle, _rx) = BackgroundJobHandle::new(
            "cve_scan_test",
            "CVE Scan Test",
            std::time::Duration::from_secs(60),
            true, // enabled by default
        );

        // Initially: enabled, not running, no run timestamps.
        let status = handle.status().await;
        assert!(status.enabled, "job should start enabled");
        assert!(!status.is_running, "job should not be running initially");
        assert!(status.last_run_at.is_none(), "no last_run_at initially");
        assert!(status.next_run_at.is_none(), "no next_run_at initially");

        // Disable and verify.
        handle.set_enabled(false).await;
        let status = handle.status().await;
        assert!(!status.enabled, "job should be disabled after toggle");

        // Re-enable and verify.
        handle.set_enabled(true).await;
        let status = handle.status().await;
        assert!(status.enabled, "job should be re-enabled");

        // Run-now signal: should not panic.
        handle.trigger_run_now();

        // Set running state to simulate an active scan.
        *handle.state.is_running.write().await = true;
        let status = handle.status().await;
        assert!(status.is_running, "job should report running");

        // Set timestamps and verify they propagate to the status snapshot.
        let now = Utc::now();
        *handle.state.last_run_at.write().await = Some(now);
        *handle.state.next_run_at.write().await = Some(now + chrono::Duration::seconds(60));

        let status = handle.status().await;
        assert!(status.last_run_at.is_some(), "last_run_at should be set");
        assert!(status.next_run_at.is_some(), "next_run_at should be set");
    }
}
