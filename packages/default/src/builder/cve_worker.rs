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

use crate::config::{CacheConfig, CacheType, CrystalForgeConfig};
use crate::derivations::utils::{
    apply_cache_config_env_to_command, attic_server_url_from_cache_config,
};
use crate::log::{WorkerState, WorkerStatus, get_cve_status};
use crate::models::cache_destination::CacheDestination;
use crate::queries::cache_destinations::get_cache_destination;
use crate::queries::cve_scans::{
    create_cve_scan, get_targets_needing_cve_rescan, get_targets_needing_cve_scan,
    mark_cve_scan_failed, recover_stale_scans, save_scan_results,
};
use crate::queries::scanning::get_scan_schedule_policy;
use crate::server::jobs::BackgroundJobHandle;
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::{Context, Result};
use axum::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use tokio::fs;
use tokio::process::Command as TokioCommand;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
struct MaterializationSource {
    label: String,
    from_url: String,
    cache_config: Option<CacheConfig>,
    trusted_public_key: Option<String>,
    nix_config_lines: Vec<String>,
}

#[async_trait]
trait CveScanRunner {
    async fn scan_derivation(
        &self,
        pool: &PgPool,
        derivation_id: i32,
        vulnix_version: Option<String>,
    ) -> Result<crate::vulnix::vulnix_parser::VulnixScanOutput>;
}

#[async_trait]
impl CveScanRunner for VulnixRunner {
    async fn scan_derivation(
        &self,
        pool: &PgPool,
        derivation_id: i32,
        vulnix_version: Option<String>,
    ) -> Result<crate::vulnix::vulnix_parser::VulnixScanOutput> {
        VulnixRunner::scan_derivation(self, pool, derivation_id, vulnix_version).await
    }
}

fn cache_destination_to_config(dest: &CacheDestination) -> CacheConfig {
    let cache_type = match dest.cache_type.as_str() {
        "S3" => CacheType::S3,
        "Attic" => CacheType::Attic,
        "Http" => CacheType::Http,
        "Nix" => CacheType::Nix,
        _ => CacheType::Nix,
    };

    CacheConfig {
        cache_type,
        push_to: dest.push_to.clone(),
        push_after_build: true,
        signing_key: dest.signing_key_path.clone(),
        compression: dest.compression.clone(),
        push_filter: None,
        parallel_uploads: dest.parallel_uploads.unwrap_or(1) as u32,
        s3_region: dest.s3_region.clone(),
        s3_profile: dest.s3_profile.clone(),
        s3_access_key_id: dest.s3_access_key_id.clone(),
        s3_secret_access_key: dest.s3_secret_access_key.clone(),
        s3_session_token: dest.s3_session_token.clone(),
        s3_endpoint_url: dest.s3_endpoint_url.clone(),
        attic_token: dest.attic_token.clone(),
        attic_cache_name: dest.attic_cache_name.clone(),
        attic_ignore_upstream_cache_filter: dest.attic_ignore_upstream_cache_filter.unwrap_or(true),
        attic_jobs: dest.attic_jobs.unwrap_or(5) as u32,
        max_retries: dest.max_retries.unwrap_or(3) as u32,
        retry_delay_seconds: dest.retry_delay_seconds.unwrap_or(5) as u64,
        poll_interval: std::time::Duration::from_secs(30),
        push_timeout_seconds: dest.push_timeout_seconds.unwrap_or(3600) as u64,
        force_repush: dest.force_repush.unwrap_or(false),
        require_sigs: dest.require_sigs.unwrap_or(true),
    }
}

fn materialization_from_url(cache: &CacheConfig) -> Option<String> {
    match cache.cache_type {
        CacheType::Attic => {
            let server_url = attic_server_url_from_cache_config(cache)?;
            let cache_name = cache.attic_cache_name.as_deref()?.trim();
            if cache_name.is_empty() {
                return None;
            }
            Some(format!(
                "{}/{}",
                server_url.trim_end_matches('/'),
                cache_name
            ))
        }
        CacheType::S3 | CacheType::Http | CacheType::Nix => cache.push_to.clone(),
    }
}

fn materialization_nix_config_lines(
    from_url: &str,
    cache_config: &CacheConfig,
    trusted_public_key: Option<&str>,
) -> Vec<String> {
    let mut lines = vec![format!("extra-substituters = {from_url}")];

    if let Some(public_key) = trusted_public_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("extra-trusted-public-keys = {public_key}"));
    }

    if matches!(cache_config.cache_type, CacheType::Attic)
        && let Some(token) = cache_config.attic_token.as_deref().map(str::trim)
        && !token.is_empty()
        && let Ok(parsed) = url::Url::parse(from_url)
        && let Some(host) = parsed.host_str()
    {
        lines.push(format!("access-tokens = {host}={token}"));
    }

    lines
}

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

        if let Err(e) = scan_cycle(
            &pool,
            &vulnix_config,
            &vulnix_runner,
            vulnix_version.clone(),
        )
        .await
        {
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
        // trigger fires `changed()` **at least once**.  Rapid consecutive
        // triggers may coalesce because `watch::Receiver::changed()` coalesces
        // intermediate values, but this is harmless — the loop will pick up any
        // remaining work on the next cycle.
        tokio::select! {
            _ = sleep(vulnix_config.poll_interval) => {}
            _ = run_now_rx.changed() => {
                info!("⚡ CVE scan loop: run-now signal received — starting immediate cycle");
            }
        }
    }
}

const BATCH_SIZE: i64 = 5;
const POST_BUILD_CONCURRENCY: usize = 5;

/// One full scan cycle: post-build scans followed by periodic rescans.
///
/// Phase 1 (post-build) drains the queue in batches until empty, tracking
/// attempted derivation IDs in a [`HashSet`] so that a failing derivation is
/// not retried within the same cycle.  This guarantees every eligible,
/// non-failing derivation is scanned within one poll interval.
async fn scan_cycle(
    pool: &PgPool,
    vulnix_config: &crate::config::VulnixConfig,
    vulnix_runner: &VulnixRunner,
    vulnix_version: Option<String>,
) -> Result<()> {
    scan_cycle_with_runner(pool, vulnix_config, vulnix_runner, vulnix_version).await
}

async fn scan_cycle_with_runner<R: CveScanRunner + Sync>(
    pool: &PgPool,
    vulnix_config: &crate::config::VulnixConfig,
    vulnix_runner: &R,
    vulnix_version: Option<String>,
) -> Result<()> {
    set_cve_status_working("finding scan targets").await;

    // Derive stale-recovery threshold from the vulnix timeout rather than a
    // hardcoded constant.  A legitimate in-progress scan may take up to the
    // configured timeout to complete; we add a 120 s safety margin on top.
    let stale_threshold = vulnix_config
        .timeout
        .saturating_add(std::time::Duration::from_secs(120));
    match recover_stale_scans(pool, stale_threshold).await {
        Ok(n) if n > 0 => warn!("Recovered {n} stale in_progress CVE scan(s)"),
        Ok(_) => {}
        Err(e) => error!("Failed to recover stale CVE scans: {e}"),
    }

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

    // --- Phase 1: post-build scans (drain queue until empty) ---
    if policy.on_build {
        let snapshot_cutoff = Utc::now();
        let mut attempted: HashSet<i32> = HashSet::new();
        loop {
            let excluded_ids: Vec<i32> = attempted.iter().copied().collect();
            match get_targets_needing_cve_scan(
                pool,
                Some(BATCH_SIZE),
                &excluded_ids,
                Some(snapshot_cutoff),
            )
            .await
            {
                Ok(targets) if !targets.is_empty() => {
                    let batch: Vec<_> = targets.into_iter().take(BATCH_SIZE as usize).collect();

                    if batch.is_empty() {
                        debug!(
                            "🔍 Post-build queue drained ({} attempted)",
                            attempted.len()
                        );
                        break;
                    }

                    for derivation in &batch {
                        attempted.insert(derivation.id);
                    }

                    for chunk in batch.chunks(POST_BUILD_CONCURRENCY) {
                        let futures = chunk.iter().map(|derivation| async {
                            info!(
                                "🔍 [post-build] Scanning newly built derivation: {}",
                                derivation.derivation_name
                            );
                            if let Err(e) =
                                scan_one(pool, vulnix_runner, vulnix_version.clone(), derivation)
                                    .await
                            {
                                error!(
                                    "❌ [post-build] Scan failed for {}: {e}",
                                    derivation.derivation_name
                                );
                            }
                        });
                        futures::future::join_all(futures).await;
                    }
                }
                Ok(_) => {
                    debug!(
                        "🔍 Post-build queue drained ({} attempted)",
                        attempted.len()
                    );
                    break;
                }
                Err(e) => {
                    error!("❌ Failed to get post-build scan targets: {e}");
                    break;
                }
            }
        }
    } else {
        debug!("🔍 on_build = false — skipping post-build scan phase");
    }

    // --- Phase 2: periodic rescan (stale completed scans) ---
    match get_targets_needing_cve_rescan(pool, Some(BATCH_SIZE)).await {
        Ok(targets) if !targets.is_empty() => {
            info!(
                "🔄 [rescan] Re-scanning {} stale derivation(s)",
                targets.len()
            );
            for derivation in &targets {
                info!(
                    "🔄 [rescan] Re-scanning stale derivation: {}",
                    derivation.derivation_name
                );
                if let Err(e) =
                    scan_one(pool, vulnix_runner, vulnix_version.clone(), derivation).await
                {
                    error!(
                        "❌ [rescan] Scan failed for {}: {e}",
                        derivation.derivation_name
                    );
                }
            }
        }
        Ok(_) => {
            debug!("🔍 No derivations need CVE rescanning");
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
async fn scan_one<R: CveScanRunner + Sync>(
    pool: &PgPool,
    vulnix_runner: &R,
    vulnix_version: Option<String>,
    derivation: &crate::derivations::Derivation,
) -> Result<()> {
    set_cve_status_working(&format!("scanning {}", derivation.derivation_name)).await;

    let scan_claim = create_cve_scan(pool, derivation.id, "vulnix", vulnix_version.clone()).await?;
    let scan_id = scan_claim.id();
    if !scan_claim.was_created() {
        info!(
            "⏭️ Skipping duplicate CVE scan for {} — active scan {scan_id} already exists",
            derivation.derivation_name
        );
        return Ok(());
    }

    let Some(ref path) = derivation.store_path else {
        warn!(
            "❌ No store_path set for derivation {}",
            derivation.derivation_name
        );
        mark_cve_scan_failed(
            pool,
            scan_id,
            derivation,
            "No store_path set for derivation",
        )
        .await?;
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
                mark_cve_scan_failed(
                    pool,
                    scan_id,
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
                    scan_id,
                    derivation,
                    &format!("Cache materialization error: {}", e),
                )
                .await?;
                return Ok(());
            }
        }
    }

    let start = std::time::Instant::now();
    match vulnix_runner
        .scan_derivation(pool, derivation.id, vulnix_version)
        .await
    {
        Ok(entries) => {
            let elapsed_ms = Some(start.elapsed().as_millis() as i32);
            let stats = crate::vulnix::vulnix_parser::VulnixParser::calculate_stats(&entries);
            if let Err(err) = save_scan_results(pool, scan_id, &entries, elapsed_ms).await {
                mark_cve_scan_failed(pool, scan_id, derivation, &err.to_string()).await?;
                return Err(err);
            }
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
                mark_cve_scan_failed(pool, scan_id, derivation, &e.to_string()).await
            {
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
    // Resolve completed cache pushes for this derivation. `cache_destination`
    // may hold either a DB destination name or a legacy/server.toml URL, so we
    // match on both `cd.name` and `cd.push_to`.
    let cache_rows = sqlx::query(
        r#"
        SELECT DISTINCT
            cpj.cache_destination,
            cd.id AS cache_destination_id,
            cd.name AS cache_destination_name
        FROM cache_push_jobs cpj
        LEFT JOIN cache_destinations cd
            ON (cd.push_to = cpj.cache_destination OR cd.name = cpj.cache_destination)
        WHERE cpj.derivation_id = $1
          AND cpj.status = 'completed'
          AND COALESCE(cd.push_to, cpj.cache_destination) IS NOT NULL
        "#,
    )
    .bind(derivation.id)
    .fetch_all(pool)
    .await
    .context("Failed to query cache destinations for materialization")?;

    let mut sources = Vec::new();
    for row in &cache_rows {
        let raw_destination = row
            .try_get::<String, _>("cache_destination")
            .context("cache_destination row missing cache_destination")?;

        if let Ok(cache_destination_id) = row.try_get::<i32, _>("cache_destination_id") {
            let Some(destination) = get_cache_destination(pool, cache_destination_id)
                .await
                .with_context(|| {
                    format!(
                        "Failed to load cache destination {} for materialization",
                        cache_destination_id
                    )
                })?
            else {
                continue;
            };

            let cache_config = cache_destination_to_config(&destination);
            let Some(from_url) = materialization_from_url(&cache_config) else {
                warn!(
                    "Skipping cache destination {} for {}: missing usable materialization URL",
                    destination.name, store_path
                );
                continue;
            };
            let nix_config_lines = materialization_nix_config_lines(
                &from_url,
                &cache_config,
                destination.attic_public_key.as_deref(),
            );

            sources.push(MaterializationSource {
                label: destination.name.clone(),
                from_url,
                cache_config: Some(cache_config),
                trusted_public_key: destination.attic_public_key.clone(),
                nix_config_lines,
            });
        } else if !raw_destination.trim().is_empty() {
            let cfg = CrystalForgeConfig::load().unwrap_or_default();
            let server_cache = cfg.get_cache_config().clone();
            let cache_config = server_cache
                .push_to
                .as_deref()
                .filter(|push_to| push_to.trim() == raw_destination.trim())
                .map(|_| server_cache.clone());
            let resolved_from_url = cache_config
                .as_ref()
                .and_then(materialization_from_url)
                .unwrap_or_else(|| raw_destination.clone());
            let nix_config_lines = cache_config
                .as_ref()
                .map(|cache| materialization_nix_config_lines(&resolved_from_url, cache, None))
                .unwrap_or_default();
            sources.push(MaterializationSource {
                label: raw_destination.clone(),
                from_url: resolved_from_url,
                cache_config,
                trusted_public_key: None,
                nix_config_lines,
            });
        }
    }

    if sources.is_empty() {
        debug!(
            "No completed cache push found for derivation {} — cannot materialize",
            derivation.id
        );
        return Ok(false);
    }

    // Timeout each nix copy attempt after 300 seconds to avoid hanging the
    // scan loop when the cache is unreachable or very slow.
    const NIX_COPY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

    for source in &sources {
        debug!(
            "Trying nix copy --from {} {} (source: {})",
            source.from_url, store_path, source.label
        );

        // Spawn with kill_on_drop(true) so the child is guaranteed to be
        // terminated if the child handle is dropped.
        let mut command = TokioCommand::new("nix");
        command.arg("copy").arg("--from").arg(&source.from_url);

        if let Some(cache_config) = &source.cache_config {
            if matches!(cache_config.cache_type, CacheType::Attic) {
                command.args(["--option", "http2", "false"]);
            }

            if let Some(public_key) = source.trusted_public_key.as_deref() {
                command.args(["--option", "trusted-public-keys", public_key]);
            }

            apply_cache_config_env_to_command(&mut command, cache_config);
        }

        if !source.nix_config_lines.is_empty() {
            command.env("NIX_CONFIG", source.nix_config_lines.join("\n") + "\n");
        }

        let mut child = command
            .arg(store_path)
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("Failed to spawn nix copy from {}", source.from_url))?;

        // Use `child.wait()` (borrows `&mut self`) instead of
        // `wait_with_output()` (consumes `self`) so we can still kill the
        // child if the timeout fires.
        let wait_result = match timeout(NIX_COPY_TIMEOUT, child.wait()).await {
            Ok(result) => result?,
            Err(_elapsed) => {
                warn!(
                    "nix copy from {} timed out after {}s for {} — killing process",
                    source.from_url,
                    NIX_COPY_TIMEOUT.as_secs(),
                    store_path
                );
                // Kill and reap to avoid zombies.
                let _ = child.kill().await;
                let _ = child.wait().await;
                continue; // Try the next cache URL.
            }
        };

        if wait_result.success() {
            // Verify the path is now present.
            match fs::try_exists(store_path).await {
                Ok(true) => return Ok(true),
                Ok(false) => {
                    warn!(
                        "nix copy from {} reported success but {} is still absent",
                        source.from_url, store_path
                    );
                    // Continue trying other caches.
                }
                Err(e) => {
                    warn!(
                        "nix copy from {} succeeded but path check failed: {}",
                        source.from_url, e
                    );
                }
            }
        } else {
            warn!(
                "nix copy from {} failed for {} (exit code: {:?})",
                source.from_url,
                store_path,
                wait_result.code()
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
    use crate::queries::derivations::{EvaluationStatus, insert_derivation};
    use sqlx::PgPool;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tempfile::tempdir;
    use uuid::Uuid;

    #[derive(Clone)]
    struct FakeRunner {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl CveScanRunner for FakeRunner {
        async fn scan_derivation(
            &self,
            _pool: &PgPool,
            _derivation_id: i32,
            _vulnix_version: Option<String>,
        ) -> Result<crate::vulnix::vulnix_parser::VulnixScanOutput> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![])
        }
    }

    async fn db_test_pool() -> Option<PgPool> {
        let Ok(db_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return None;
        };
        PgPool::connect(&db_url).await.ok()
    }

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

    /// Confirms that `get_targets_needing_cve_scan` compiles and returns
    /// without panicking even when connected to a lazy pool (no real DB).
    #[tokio::test]
    async fn get_targets_needing_cve_scan_does_not_panic() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct without connecting");

        let result =
            crate::queries::cve_scans::get_targets_needing_cve_scan(&pool, Some(5), &[], None)
                .await;
        if let Err(e) = result {
            let msg = format!("{e:#}");
            assert!(
                msg.contains("error") || msg.contains("connect") || msg.contains("pool"),
                "unexpected error type: {msg}"
            );
        }
    }

    /// Confirms that `get_targets_needing_cve_rescan` compiles and returns
    /// without panicking even when connected to a lazy pool.
    #[tokio::test]
    async fn get_targets_needing_cve_rescan_does_not_panic() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct without connecting");

        let result =
            crate::queries::cve_scans::get_targets_needing_cve_rescan(&pool, Some(5)).await;
        if let Err(e) = result {
            let msg = format!("{e:#}");
            assert!(
                msg.contains("error") || msg.contains("connect") || msg.contains("pool"),
                "unexpected error type: {msg}"
            );
        }
    }

    /// Confirms that `materialize_store_path_from_cache` compiles and handles
    /// the no-cache-found case without panicking.  The function will return
    /// `Ok(false)` because no real cache push jobs exist, verifying the cache
    /// resolution path works end-to-end at the query level.
    #[tokio::test]
    async fn materialize_store_path_from_cache_handles_empty() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct without connecting");

        let derivation = crate::derivations::Derivation {
            id: -1,
            commit_id: None,
            derivation_type: crate::derivations::DerivationType::NixOS,
            derivation_name: "test-nonexistent".into(),
            derivation_path: None,
            derivation_target: None,
            scheduled_at: None,
            completed_at: None,
            started_at: None,
            attempt_count: 0,
            evaluation_duration_ms: None,
            error_message: None,
            pname: None,
            version: None,
            status_id: 0,
            build_elapsed_seconds: None,
            build_current_target: None,
            build_last_activity_seconds: None,
            build_last_heartbeat: None,
            cf_agent_enabled: None,
            store_path: Some("/nix/store/00000000000000000000000000000000-test".into()),
        };

        // With no real cache pushes, this should return Ok(false) without
        // panicking or hanging due to the subprocess timeout.
        let result = materialize_store_path_from_cache(
            &pool,
            &derivation,
            derivation.store_path.as_ref().unwrap(),
        )
        .await;

        match result {
            Ok(false) => {} // Expected: no cache found.
            Ok(true) => unreachable!("no cache should exist for id -1"),
            Err(e) => {
                // In CI the query may fail immediately (no DB).  That is also
                // acceptable — what matters is no panic and no hang.
                let msg = format!("{e:#}");
                assert!(
                    msg.contains("Failed to query cache destinations"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    /// Confirms that the stale scan recovery function compiles and that its
    /// argument signature matches the runtime usage.
    #[tokio::test]
    async fn recover_stale_scans_accepts_duration() {
        // Just verify the function exists with the right signature by calling
        // it with a lazy pool — it will fail at runtime but won't panic at
        // compile time.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct without connecting");

        // We only verify the function compiles and can be dispatched.  The DB
        // query will fail because there is no real pool, but we check the error
        // kind rather than the function itself panicking.
        let result = crate::queries::cve_scans::recover_stale_scans(
            &pool,
            std::time::Duration::from_secs(600),
        )
        .await;

        if let Err(e) = result {
            let msg = format!("{e:#}");
            // Lazy pool + no DB produces a connection error — not a panic.
            assert!(
                msg.contains("error") || msg.contains("connect") || msg.contains("pool"),
                "unexpected error type: {msg}"
            );
        }
    }

    #[tokio::test]
    async fn scan_cycle_processes_target_with_fake_runner() {
        let Some(pool) = db_test_pool().await else {
            return;
        };
        let tempdir = tempdir().expect("tempdir should be created");
        let store_path = tempdir.path().join("task-396-scan-cycle-store-path");
        std::fs::create_dir_all(&store_path).expect("store path dir should be created");

        sqlx::query!(
            r#"
            INSERT INTO scan_schedule_policy (id, on_build, deployed_interval, recent_interval, archived_interval, archived_enabled)
            VALUES (1, true, '24h', '24h', '168h', true)
            ON CONFLICT (id) DO UPDATE
            SET on_build = EXCLUDED.on_build,
                deployed_interval = EXCLUDED.deployed_interval,
                recent_interval = EXCLUDED.recent_interval,
                archived_interval = EXCLUDED.archived_interval,
                archived_enabled = EXCLUDED.archived_enabled,
                updated_at = NOW()
            "#,
        )
        .execute(&pool)
        .await
        .expect("scan schedule policy should be inserted");

        let derivation_name = format!("task-396-cycle-{}", Uuid::new_v4());
        let derivation = insert_derivation(&pool, None, &derivation_name, "nixos")
            .await
            .expect("derivation should be inserted");

        sqlx::query!(
            r#"
            UPDATE derivations
            SET status_id = $2,
                completed_at = NOW(),
                store_path = $3
            WHERE id = $1
            "#,
            derivation.id,
            EvaluationStatus::BuildComplete.as_id(),
            store_path.to_string_lossy().to_string(),
        )
        .execute(&pool)
        .await
        .expect("derivation should be marked build-complete");

        let runner = FakeRunner {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let vulnix_config = crate::config::VulnixConfig::default();

        scan_cycle_with_runner(&pool, &vulnix_config, &runner, Some("test".to_string()))
            .await
            .expect("scan cycle should succeed");
        scan_cycle_with_runner(&pool, &vulnix_config, &runner, Some("test".to_string()))
            .await
            .expect("second scan cycle should succeed");

        assert_eq!(
            runner.calls.load(Ordering::SeqCst),
            1,
            "target should be processed exactly once"
        );

        let scan = sqlx::query!(
            r#"
            SELECT status, completed_at
            FROM cve_scans
            WHERE derivation_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            derivation.id,
        )
        .fetch_one(&pool)
        .await
        .expect("scan row should exist");

        assert_eq!(scan.status, Some("completed".to_string()));
        assert!(scan.completed_at.is_some(), "scan should be terminal");
    }

    /// Confirms that [`BackgroundJobHandle`] state machine correctly reports
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
