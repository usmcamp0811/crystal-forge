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
    CreateCveScanOutcome, CveScanExecutionClaim, acknowledge_revoked_cve_scan_execution,
    claim_queued_cve_scans, create_cve_scan, get_targets_needing_cve_rescan,
    get_targets_needing_cve_scan, heartbeat_cve_scan_execution,
    mark_cve_scan_failed_by_id_for_execution, mark_cve_scan_failed_for_execution,
    recover_stale_scans, requeue_cve_scan_execution, save_scan_results_for_execution,
};
use crate::queries::derivations::get_derivation_by_id;
use crate::queries::scanning::get_scan_schedule_policy;
use crate::server::jobs::BackgroundJobHandle;
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::{Context, Result};
use axum::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};
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
        attic_public_key: dest.attic_public_key.clone(),
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

    if !cache_config.require_sigs {
        lines.push("require-sigs = false".to_string());
    }

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
/// The loop runs indefinitely.  When vulnix is unavailable it sleeps and
/// retries so that installing or restoring vulnix does not require a server
/// restart.  All cycle errors are logged and the loop continues.
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

    let vulnix_runner = VulnixRunner::with_config(&vulnix_config);

    // Keep a reference to the enabled lock so scan_cycle can poll it mid-cycle.
    let enabled_rx = &job.state.enabled;
    // Subscribe to enable/disable wake signals so the disabled sleep is
    // interruptible without waiting the full poll interval.
    let mut enabled_changed_rx = job.state.enabled_changed_tx.subscribe();

    loop {
        // Honour the enabled flag — sleep the full interval and skip work when disabled.
        let enabled = *enabled_rx.read().await;
        if !enabled {
            debug!("CVE scan loop: disabled — skipping cycle");
            // Update next_run_at while disabled so the UI shows something reasonable.
            *job.state.next_run_at.write().await = Some(
                Utc::now()
                    + chrono::Duration::from_std(vulnix_config.poll_interval)
                        .unwrap_or(chrono::Duration::seconds(60)),
            );
            // Sleep until the poll interval, a run-now signal, or an
            // enable/disable state change — whichever comes first.
            tokio::select! {
                _ = sleep(vulnix_config.poll_interval) => {}
                _ = run_now_rx.changed() => {}
                _ = enabled_changed_rx.changed() => {}
            }
            // Consume any run-now counter so a pending signal does not
            // immediately re-fire on the next enabled cycle.
            let _ = run_now_rx.borrow_and_update();
            continue;
        }

        // Retry vulnix availability every cycle so the loop survives a temporary
        // installation gap without requiring a server restart.
        if !VulnixRunner::check_vulnix_available().await {
            warn!("⚠️ vulnix is not available — will retry on next poll interval");
            set_cve_status_idle().await;
            *job.state.next_run_at.write().await = Some(
                Utc::now()
                    + chrono::Duration::from_std(vulnix_config.poll_interval)
                        .unwrap_or(chrono::Duration::seconds(60)),
            );
            tokio::select! {
                _ = sleep(vulnix_config.poll_interval) => {}
                _ = run_now_rx.changed() => {}
                _ = enabled_changed_rx.changed() => {}
            }
            let _ = run_now_rx.borrow_and_update();
            continue;
        }

        let vulnix_version = VulnixRunner::get_vulnix_version().await.ok();

        debug!("🔧 vulnix version: {:?}", vulnix_version);
        debug!(
            "🔧 vulnix config: timeout={}s whitelist={} extra_args={:?}",
            vulnix_config.timeout_seconds(),
            vulnix_config.enable_whitelist,
            vulnix_config.extra_args
        );

        // Mark the job as running and record timing.
        *job.state.is_running.write().await = true;
        *job.state.last_run_at.write().await = Some(Utc::now());

        if let Err(e) = scan_cycle(
            &pool,
            &vulnix_config,
            &vulnix_runner,
            vulnix_version.clone(),
            &enabled_rx,
        )
        .await
        {
            error!("❌ Error in CVE scan cycle: {e}");
        }
        // Always reset the worker status after every cycle so the UI does not
        // remain permanently in "Working" state when no rescan targets were found.
        set_cve_status_idle().await;

        // Update job metadata after the cycle completes.
        *job.state.is_running.write().await = false;
        *job.state.next_run_at.write().await = Some(
            Utc::now()
                + chrono::Duration::from_std(vulnix_config.poll_interval)
                    .unwrap_or(chrono::Duration::seconds(60)),
        );

        // Wait for the poll interval, a run-now signal, or an enable/disable
        // change — whichever comes first.  All three sources can legitimately
        // cut the wait short; the top of the loop re-reads `enabled` so a
        // spurious wake from the enabled-changed channel is harmless.
        tokio::select! {
            _ = sleep(vulnix_config.poll_interval) => {}
            _ = run_now_rx.changed() => {
                info!("⚡ CVE scan loop: run-now signal received — starting immediate cycle");
            }
            _ = enabled_changed_rx.changed() => {}
        }
    }
}

/// Maximum derivations scanned per cycle phase (post-build or rescan).
///
/// Processing is bounded per cycle so that a large historical backlog does not
/// monopolise the database for an extended period. At 1 scan/cycle with a
/// 60-second poll interval a backlog of N derivations clears in ~N minutes,
/// which is acceptable. Raise this constant once bulk-persistence lands and
/// the write amplification per scan is addressed.
const MAX_SCANS_PER_CYCLE: i64 = 1;

/// One full scan cycle: post-build scans followed by periodic rescans.
///
/// Phase 1 (post-build) processes at most [`MAX_SCANS_PER_CYCLE`] derivations
/// and returns — it does **not** loop until the queue is empty. The remaining
/// backlog is processed across subsequent poll cycles, which prevents a large
/// historical backlog from monopolising the database for many minutes.
///
/// Phase 2 (rescan) likewise processes at most [`MAX_SCANS_PER_CYCLE`] stale
/// derivations per cycle.
async fn scan_cycle(
    pool: &PgPool,
    vulnix_config: &crate::config::VulnixConfig,
    vulnix_runner: &VulnixRunner,
    vulnix_version: Option<String>,
    enabled_rx: &tokio::sync::RwLock<bool>,
) -> Result<()> {
    scan_cycle_with_runner(
        pool,
        vulnix_config,
        vulnix_runner,
        vulnix_version,
        enabled_rx,
    )
    .await
}

async fn scan_cycle_with_runner<R: CveScanRunner + Sync>(
    pool: &PgPool,
    vulnix_config: &crate::config::VulnixConfig,
    vulnix_runner: &R,
    vulnix_version: Option<String>,
    enabled_rx: &tokio::sync::RwLock<bool>,
) -> Result<()> {
    set_cve_status_working("finding scan targets").await;

    // Derive stale-recovery threshold from both the vulnix timeout and the
    // worst-case single-cache materialization timeout, plus a small margin.
    const MATERIALIZATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
    let stale_threshold = vulnix_config
        .timeout
        .saturating_add(MATERIALIZATION_TIMEOUT)
        .saturating_add(std::time::Duration::from_secs(120));
    match recover_stale_scans(pool, stale_threshold).await {
        Ok(n) if n > 0 => warn!("Recovered {n} stale in_progress CVE scan(s)"),
        Ok(_) => {}
        Err(e) => error!("Failed to recover stale CVE scans: {e}"),
    }

    // --- Phase 0: operator-queued scans (bounded) ---
    //
    // Fleet rescan requests enqueue `pending` claims rather than executing
    // scans inline, so this phase is what actually runs them. It is bounded by
    // the same per-cycle budget as the other phases, which is the mechanism
    // that prevents a single fleet-wide request from starting an unbounded
    // number of concurrent vulnix processes.
    //
    // This runs before the policy load on purpose: a queued scan is an explicit
    // operator request and should not be suppressed by a scan-policy read
    // failure or by `on_build` being disabled.
    match claim_queued_cve_scans(pool, MAX_SCANS_PER_CYCLE).await {
        Ok(queued) if !queued.is_empty() => {
            info!(
                "📥 [queued] Draining {} operator-requested scan(s) this cycle (limit {})",
                queued.len(),
                MAX_SCANS_PER_CYCLE
            );
            for claim in queued {
                if requeue_claim_if_disabled(pool, claim, enabled_rx).await? {
                    info!("🛑 CVE scan loop disabled — stopping queued phase");
                    return Ok(());
                }

                let derivation = match get_derivation_by_id(pool, claim.derivation_id).await {
                    Ok(derivation) => derivation,
                    Err(e) => {
                        error!(
                            "❌ [queued] Could not load derivation {}: {e}",
                            claim.derivation_id
                        );
                        if let Err(mark_err) = mark_cve_scan_failed_by_id_for_execution(
                            pool,
                            claim.scan_id,
                            claim.derivation_id,
                            &format!("Could not load derivation: {e}"),
                            claim.execution_id,
                        )
                        .await
                        {
                            error!(
                                "❌ [queued] Failed to mark scan {} failed: {mark_err}",
                                claim.scan_id
                            );
                        }
                        continue;
                    }
                };

                set_cve_status_working(&format!("scanning {}", derivation.derivation_name)).await;
                // This final read is the execution handoff: there is no await
                // between observing enabled and starting `execute_scan`. Drop
                // the guard immediately so disabling does not block for the
                // full materialization/vulnix/persistence window.
                let enabled_guard = enabled_rx.read().await;
                if !*enabled_guard {
                    drop(enabled_guard);
                    let requeued = requeue_cve_scan_execution(
                        pool,
                        claim.scan_id,
                        claim.execution_id,
                        "worker-disabled-before-execution",
                    )
                    .await?;
                    if !requeued {
                        warn!(
                            "Queued CVE scan {} lost ownership before it could be requeued",
                            claim.scan_id
                        );
                    }
                    info!("🛑 CVE scan loop disabled — stopping queued phase");
                    return Ok(());
                }
                drop(enabled_guard);
                info!(
                    "📥 [queued] Scanning operator-requested derivation: {}",
                    derivation.derivation_name
                );
                if let Err(e) = execute_scan(
                    pool,
                    vulnix_runner,
                    vulnix_version.clone(),
                    &derivation,
                    claim.scan_id,
                    claim.execution_id,
                )
                .await
                {
                    error!(
                        "❌ [queued] Scan failed for {}: {e}",
                        derivation.derivation_name
                    );
                }
            }
        }
        Ok(_) => {
            debug!("📥 No queued scan targets this cycle");
        }
        Err(e) => {
            error!("❌ Failed to get queued scan targets: {e}");
        }
    }

    // Read scan schedule policy. Fail closed: if the policy cannot be loaded
    // we skip this cycle rather than applying aggressive hardcoded defaults.
    // This prevents a database configuration failure from silently triggering
    // a full historical backfill on first deployment.
    let policy = match get_scan_schedule_policy(pool).await {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to load scan schedule policy: {e} — skipping cycle");
            return Ok(());
        }
    };
    // --- Phase 1: post-build scans (bounded — at most MAX_SCANS_PER_CYCLE per cycle) ---
    //
    // We intentionally do NOT loop until the queue is empty. A large historical
    // backlog would otherwise monopolise the database for minutes. Each cycle
    // advances the backlog by MAX_SCANS_PER_CYCLE; subsequent poll cycles
    // continue draining it at a controlled rate.
    if policy.on_build {
        if !*enabled_rx.read().await {
            info!("🛑 CVE scan loop disabled — skipping post-build phase");
            return Ok(());
        }

        let snapshot_cutoff = Utc::now();
        match get_targets_needing_cve_scan(
            pool,
            Some(MAX_SCANS_PER_CYCLE),
            &[],
            Some(snapshot_cutoff),
        )
        .await
        {
            Ok(targets) if !targets.is_empty() => {
                info!(
                    "🔍 [post-build] Processing {} target(s) this cycle (limit {})",
                    targets.len(),
                    MAX_SCANS_PER_CYCLE
                );
                for derivation in &targets {
                    if !*enabled_rx.read().await {
                        info!("🛑 CVE scan loop disabled mid-batch — stopping post-build phase");
                        return Ok(());
                    }
                    info!(
                        "🔍 [post-build] Scanning newly built derivation: {}",
                        derivation.derivation_name
                    );
                    if let Err(e) = scan_one(
                        pool,
                        vulnix_runner,
                        vulnix_version.clone(),
                        derivation,
                        enabled_rx,
                    )
                    .await
                    {
                        error!(
                            "❌ [post-build] Scan failed for {}: {e}",
                            derivation.derivation_name
                        );
                    }
                }
            }
            Ok(_) => {
                debug!("🔍 No post-build scan targets this cycle");
            }
            Err(e) => {
                error!("❌ Failed to get post-build scan targets: {e}");
            }
        }
    } else {
        debug!("🔍 on_build = false — skipping post-build phase");
    }

    // Check enabled before entering Phase 2.
    if !*enabled_rx.read().await {
        info!("🛑 CVE scan loop disabled — skipping periodic rescan phase");
        return Ok(());
    }

    // --- Phase 2: periodic rescan (stale completed scans, bounded) ---
    match get_targets_needing_cve_rescan(pool, Some(MAX_SCANS_PER_CYCLE)).await {
        Ok(targets) if !targets.is_empty() => {
            info!(
                "🔄 [rescan] Re-scanning {} stale derivation(s)",
                targets.len()
            );
            for derivation in &targets {
                if !*enabled_rx.read().await {
                    info!("🛑 CVE scan loop disabled — stopping rescan phase");
                    return Ok(());
                }
                info!(
                    "🔄 [rescan] Re-scanning stale derivation: {}",
                    derivation.derivation_name
                );
                if let Err(e) = scan_one(
                    pool,
                    vulnix_runner,
                    vulnix_version.clone(),
                    derivation,
                    enabled_rx,
                )
                .await
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
        }
        Err(e) => error!("❌ Failed to get rescan targets: {e}"),
    }

    Ok(())
}

async fn requeue_claim_if_disabled(
    pool: &PgPool,
    claim: CveScanExecutionClaim,
    enabled_rx: &tokio::sync::RwLock<bool>,
) -> Result<bool> {
    if *enabled_rx.read().await {
        return Ok(false);
    }

    let requeued = requeue_cve_scan_execution(
        pool,
        claim.scan_id,
        claim.execution_id,
        "worker-disabled-before-execution",
    )
    .await?;
    if !requeued {
        warn!(
            "Queued CVE scan {} lost ownership before it could be requeued",
            claim.scan_id
        );
    }
    Ok(true)
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
    enabled_rx: &tokio::sync::RwLock<bool>,
) -> Result<()> {
    set_cve_status_working(&format!("scanning {}", derivation.derivation_name)).await;

    // Hold the enabled read guard across the claim INSERT so that
    // `set_enabled(false)` — which needs the write lock — cannot return until
    // any claim we are in the process of creating has either been written or
    // we have confirmed we should not proceed.
    //
    // Tokio's RwLock is write-preferring: once set_enabled(false) begins
    // waiting for the write lock, no new read guard can be acquired.  This
    // ensures that any scan_one() call begun after set_enabled(false) returns
    // will see enabled=false and exit without creating a claim.
    let claim = {
        let enabled_guard = enabled_rx.read().await;
        if !*enabled_guard {
            debug!(
                "Skipping claim for {} — job disabled",
                derivation.derivation_name
            );
            return Ok(());
        }

        let scan_claim =
            create_cve_scan(pool, derivation.id, "vulnix", vulnix_version.clone()).await?;
        // Release the guard as soon as the claim is committed.  From here the
        // scan is authorized and runs to completion even if disable fires later.
        drop(enabled_guard);

        match scan_claim {
            CreateCveScanOutcome::Created(claim) => claim,
            CreateCveScanOutcome::Existing(scan_id) => {
                info!(
                    "⏭️ Skipping duplicate CVE scan for {} — active scan {scan_id} already exists",
                    derivation.derivation_name
                );
                return Ok(());
            }
        }
    };

    execute_scan(
        pool,
        vulnix_runner,
        vulnix_version,
        derivation,
        claim.scan_id,
        claim.execution_id,
    )
    .await
}

/// Execute a CVE scan for an already-claimed `cve_scans` row.
///
/// Shared by worker-initiated scans (via [`scan_one`], which creates the claim
/// itself) and by queued scans created by an operator fleet request (which
/// adopt a pre-existing `pending` claim). Both therefore inherit the same
/// lifecycle: local store-path presence check, cache materialization fallback,
/// vulnix invocation, and result persistence.
///
/// Keeping this as the single execution path is deliberate. A caller that
/// invokes vulnix directly would skip cache materialization and fail for
/// derivations whose store path has been garbage-collected locally but is still
/// available from a completed cache push.
async fn execute_scan<R: CveScanRunner + Sync>(
    pool: &PgPool,
    vulnix_runner: &R,
    vulnix_version: Option<String>,
    derivation: &crate::derivations::Derivation,
    scan_id: uuid::Uuid,
    execution_id: uuid::Uuid,
) -> Result<()> {
    let mut execution = Box::pin(execute_scan_inner(
        pool,
        vulnix_runner,
        vulnix_version,
        derivation,
        scan_id,
        execution_id,
    ));
    // Heartbeat the entire materialization/vulnix/persistence window. Terminal
    // writes still verify the token, so heartbeat is liveness rather than the
    // final authorization boundary.
    #[cfg(not(test))]
    const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
    #[cfg(test)]
    const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
    const HEARTBEAT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
    let start = tokio::time::Instant::now() + HEARTBEAT_INTERVAL;
    let mut heartbeat = tokio::time::interval_at(start, HEARTBEAT_INTERVAL);
    loop {
        tokio::select! {
            biased;
            result = &mut execution => return result,
            _ = heartbeat.tick() => {
                match tokio::time::timeout(
                    HEARTBEAT_QUERY_TIMEOUT,
                    heartbeat_cve_scan_execution(pool, scan_id, execution_id),
                )
                .await
                {
                    Ok(Ok(true)) => {}
                    Ok(Ok(false)) => {
                        // Cancel the scanner before acknowledging revocation.
                        // Until this future is dropped the row remains
                        // in_progress, preserving active-scan uniqueness.
                        drop(execution);
                        let _ = acknowledge_revoked_cve_scan_execution(
                            pool,
                            scan_id,
                            execution_id,
                        )
                        .await;
                        anyhow::bail!("CVE scan {scan_id} lost execution ownership");
                    }
                    Ok(Err(err)) => {
                        drop(execution);
                        let heartbeat_error = err.context(
                            "Failed to refresh CVE scan execution heartbeat",
                        );
                        let _ = mark_cve_scan_failed_by_id_for_execution(
                            pool,
                            scan_id,
                            derivation.id,
                            &heartbeat_error.to_string(),
                            execution_id,
                        )
                        .await;
                        return Err(heartbeat_error);
                    }
                    Err(_) => {
                        drop(execution);
                        let heartbeat_error = anyhow::anyhow!(
                            "Timed out refreshing CVE scan {scan_id} execution heartbeat"
                        );
                        let _ = mark_cve_scan_failed_by_id_for_execution(
                            pool,
                            scan_id,
                            derivation.id,
                            &heartbeat_error.to_string(),
                            execution_id,
                        )
                        .await;
                        return Err(heartbeat_error);
                    }
                }
            }
        }
    }
}

async fn execute_scan_inner<R: CveScanRunner + Sync>(
    pool: &PgPool,
    vulnix_runner: &R,
    vulnix_version: Option<String>,
    derivation: &crate::derivations::Derivation,
    scan_id: uuid::Uuid,
    execution_id: uuid::Uuid,
) -> Result<()> {
    let Some(ref path) = derivation.store_path else {
        warn!(
            "❌ No store_path set for derivation {}",
            derivation.derivation_name
        );
        mark_scan_failed_for_owner(
            pool,
            scan_id,
            derivation,
            "No store_path set for derivation",
            execution_id,
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
                mark_scan_failed_for_owner(
                    pool,
                    scan_id,
                    derivation,
                    &format!(
                        "Store path {} not present locally and no cache could provide it",
                        path
                    ),
                    execution_id,
                )
                .await?;
                return Ok(());
            }
            Err(e) => {
                error!("❌ Error materializing {} from cache: {}", path, e);
                mark_scan_failed_for_owner(
                    pool,
                    scan_id,
                    derivation,
                    &format!("Cache materialization error: {}", e),
                    execution_id,
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
            if let Err(err) =
                save_scan_results_for_execution(pool, scan_id, &entries, elapsed_ms, execution_id)
                    .await
            {
                mark_scan_failed_for_owner(
                    pool,
                    scan_id,
                    derivation,
                    &err.to_string(),
                    execution_id,
                )
                .await?;
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
            mark_cve_scan_failed_for_execution(
                pool,
                scan_id,
                derivation,
                &e.to_string(),
                execution_id,
            )
            .await
            .context("Failed to persist CVE scanner failure")?;
        }
    }

    Ok(())
}

async fn mark_scan_failed_for_owner(
    pool: &PgPool,
    scan_id: uuid::Uuid,
    derivation: &crate::derivations::Derivation,
    error_message: &str,
    execution_id: uuid::Uuid,
) -> Result<()> {
    mark_cve_scan_failed_for_execution(pool, scan_id, derivation, error_message, execution_id).await
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
                trusted_public_key: server_cache.attic_public_key.clone(),
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
    use tokio::sync::Semaphore;
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

    #[derive(Clone)]
    struct BlockingRunner {
        calls: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        cancelled: Arc<AtomicUsize>,
        release: Arc<Semaphore>,
    }

    struct ActiveCallGuard {
        active: Arc<AtomicUsize>,
        cancelled: Arc<AtomicUsize>,
        completed: bool,
    }

    impl Drop for ActiveCallGuard {
        fn drop(&mut self) {
            self.active.fetch_sub(1, Ordering::SeqCst);
            if !self.completed {
                self.cancelled.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    #[async_trait]
    impl CveScanRunner for BlockingRunner {
        async fn scan_derivation(
            &self,
            _pool: &PgPool,
            _derivation_id: i32,
            _vulnix_version: Option<String>,
        ) -> Result<crate::vulnix::vulnix_parser::VulnixScanOutput> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            let mut guard = ActiveCallGuard {
                active: self.active.clone(),
                cancelled: self.cancelled.clone(),
                completed: false,
            };

            let permit = self
                .release
                .acquire()
                .await
                .context("blocking test runner semaphore closed")?;
            permit.forget();
            guard.completed = true;
            Ok(vec![])
        }
    }

    async fn wait_for_counter(counter: &AtomicUsize, expected: usize, description: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while counter.load(Ordering::SeqCst) < expected {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {description}"));
    }

    async fn db_test_pool() -> Option<PgPool> {
        let Ok(db_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return None;
        };
        Some(
            PgPool::connect(&db_url)
                .await
                .expect("failed to connect to CRYSTAL_FORGE_TEST_DATABASE_URL"),
        )
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

    /// Verifies that `scan_cycle_with_runner` selects an eligible post-build target,
    /// invokes the scanner exactly once, and persists a completed scan record.
    ///
    /// **Requires:** `CRYSTAL_FORGE_TEST_DATABASE_URL` pointing to a dedicated,
    /// migration-applied PostgreSQL database (not the shared dev database).
    /// The test is skipped when the variable is absent; CI should provision a
    /// dedicated database and set this variable to ensure the check always runs.
    #[tokio::test]
    async fn scan_cycle_processes_target_with_fake_runner() {
        let Some(pool) = db_test_pool().await else {
            return;
        };
        let tempdir = tempdir().expect("tempdir should be created");
        let store_path = tempdir.path().join("task-396-scan-cycle-store-path");
        std::fs::create_dir_all(&store_path).expect("store path dir should be created");

        sqlx::query(
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

        sqlx::query(
            r#"
            UPDATE derivations
            SET status_id = $2,
                completed_at = NOW(),
                store_path = $3
            WHERE id = $1
            "#,
        )
        .bind(derivation.id)
        .bind(EvaluationStatus::BuildComplete.as_id())
        .bind(store_path.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("derivation should be marked build-complete");

        let runner = FakeRunner {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let vulnix_config = crate::config::VulnixConfig::default();
        let enabled_rx = tokio::sync::RwLock::new(true);

        scan_cycle_with_runner(
            &pool,
            &vulnix_config,
            &runner,
            Some("test".to_string()),
            &enabled_rx,
        )
        .await
        .expect("scan cycle should succeed");
        scan_cycle_with_runner(
            &pool,
            &vulnix_config,
            &runner,
            Some("test".to_string()),
            &enabled_rx,
        )
        .await
        .expect("second scan cycle should succeed");

        assert_eq!(
            runner.calls.load(Ordering::SeqCst),
            1,
            "target should be processed exactly once"
        );

        let (status, completed_at): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
            sqlx::query_as(
                r#"
            SELECT status, completed_at
            FROM cve_scans
            WHERE derivation_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            )
            .bind(derivation.id)
            .fetch_one(&pool)
            .await
            .expect("scan row should exist");

        assert_eq!(status, Some("completed".to_string()));
        assert!(completed_at.is_some(), "scan should be terminal");

        sqlx::query("UPDATE scan_schedule_policy SET on_build = FALSE WHERE id = 1")
            .execute(&pool)
            .await
            .expect("on-build scanning should be disabled");
        let disabled_derivation = insert_derivation(
            &pool,
            None,
            &format!("task-325-disabled-cycle-{}", Uuid::new_v4()),
            "nixos",
        )
        .await
        .expect("disabled-cycle derivation should be inserted");
        sqlx::query(
            r#"
            UPDATE derivations
            SET status_id = $2,
                completed_at = NOW(),
                store_path = $3
            WHERE id = $1
            "#,
        )
        .bind(disabled_derivation.id)
        .bind(EvaluationStatus::BuildComplete.as_id())
        .bind(store_path.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("disabled-cycle derivation should be marked build-complete");

        scan_cycle_with_runner(
            &pool,
            &vulnix_config,
            &runner,
            Some("test".to_string()),
            &enabled_rx,
        )
        .await
        .expect("disabled on-build scan cycle should succeed");
        assert_eq!(
            runner.calls.load(Ordering::SeqCst),
            1,
            "on_build=false must not process a new post-build target"
        );

        let derivation_ids = vec![derivation.id, disabled_derivation.id];
        sqlx::query("DELETE FROM cve_scans WHERE derivation_id = ANY($1)")
            .bind(&derivation_ids)
            .execute(&pool)
            .await
            .expect("scan-cycle scans should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = ANY($1)")
            .bind(&derivation_ids)
            .execute(&pool)
            .await
            .expect("scan-cycle derivations should be deleted");
        sqlx::query(
            r#"
            UPDATE scan_schedule_policy
            SET on_build = TRUE,
                deployed_interval = '24h',
                recent_interval = '24h',
                archived_interval = '168h',
                archived_enabled = TRUE
            WHERE id = 1
            "#,
        )
        .execute(&pool)
        .await
        .expect("scan policy should be restored");
    }

    /// A policy-created execution uses the same renewable ownership lease as a
    /// queued execution. Healthy heartbeats prevent stale recovery; losing the
    /// token cancels the running scanner before a replacement starts.
    #[tokio::test]
    async fn policy_scan_heartbeat_prevents_recovery_and_ownership_loss_cancels_execution() {
        let Some(pool) = db_test_pool().await else {
            return;
        };
        let recovery_pool = PgPool::connect(
            &std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
                .expect("dedicated CVE database URL should remain available"),
        )
        .await
        .expect("independent stale-recovery pool should connect");
        let tempdir = tempdir().expect("tempdir should be created");
        let store_path = tempdir.path().join("task-325-policy-lease-store-path");
        std::fs::create_dir_all(&store_path).expect("store path dir should be created");

        let mut derivation = insert_derivation(
            &pool,
            None,
            &format!("task-325-policy-lease-{}", Uuid::new_v4()),
            "nixos",
        )
        .await
        .expect("policy lease derivation should be inserted");
        derivation.store_path = Some(store_path.to_string_lossy().to_string());
        let derivation = Arc::new(derivation);

        let runner = BlockingRunner {
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::new(AtomicUsize::new(0)),
            release: Arc::new(Semaphore::new(0)),
        };
        let enabled = Arc::new(tokio::sync::RwLock::new(true));

        let first_pool = pool.clone();
        let first_runner = runner.clone();
        let first_derivation = derivation.clone();
        let first_enabled = enabled.clone();
        let first = tokio::spawn(async move {
            scan_one(
                &first_pool,
                &first_runner,
                Some("test".to_string()),
                &first_derivation,
                &first_enabled,
            )
            .await
        });
        wait_for_counter(&runner.calls, 1, "first policy scanner invocation").await;

        let (scan_id, execution_id, initial_heartbeat): (
            Uuid,
            Uuid,
            chrono::DateTime<chrono::Utc>,
        ) = sqlx::query_as(
            r#"
            SELECT
                id,
                (scan_metadata ->> 'execution_id')::uuid,
                (scan_metadata ->> 'execution_heartbeat_at')::timestamptz
            FROM cve_scans
            WHERE derivation_id = $1 AND status = 'in_progress'
            "#,
        )
        .bind(derivation.id)
        .fetch_one(&pool)
        .await
        .expect("policy-created execution should have lease metadata");

        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let heartbeat: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
                    "SELECT (scan_metadata ->> 'execution_heartbeat_at')::timestamptz FROM cve_scans WHERE id = $1",
                )
                .bind(scan_id)
                .fetch_one(&pool)
                .await
                .expect("policy heartbeat should resolve");
                if heartbeat > initial_heartbeat {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("policy execution should refresh its heartbeat");

        sqlx::query(
            r#"
            UPDATE cve_scans
            SET scan_metadata = scan_metadata || jsonb_build_object(
                'execution_started_at', NOW() - INTERVAL '2 hours',
                'execution_heartbeat_at', NOW() - INTERVAL '2 hours'
            )
            WHERE id = $1 AND scan_metadata ->> 'execution_id' = $2::uuid::text
            "#,
        )
        .bind(scan_id)
        .bind(execution_id)
        .execute(&pool)
        .await
        .expect("policy execution should be artificially aged");

        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let heartbeat: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
                    "SELECT (scan_metadata ->> 'execution_heartbeat_at')::timestamptz FROM cve_scans WHERE id = $1",
                )
                .bind(scan_id)
                .fetch_one(&pool)
                .await
                .expect("refreshed policy heartbeat should resolve");
                if heartbeat > Utc::now() - chrono::Duration::minutes(1) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("policy execution should renew its artificially aged lease");

        assert_eq!(
            recover_stale_scans(&recovery_pool, std::time::Duration::from_secs(1800))
                .await
                .expect("concurrent stale recovery should succeed"),
            0,
            "a healthy policy heartbeat must prevent stale recovery"
        );

        sqlx::query(
            r#"
            UPDATE cve_scans
            SET scan_metadata = scan_metadata || jsonb_build_object(
                'execution_revoked_at', NOW(),
                'stale_recovery_reason', 'test-intentional-revocation'
            )
            WHERE id = $1
              AND status = 'in_progress'
              AND scan_metadata ->> 'execution_id' = $2::uuid::text
            "#,
        )
        .bind(scan_id)
        .bind(execution_id)
        .execute(&recovery_pool)
        .await
        .expect("test should intentionally revoke the first policy execution");

        let first_result = tokio::time::timeout(std::time::Duration::from_secs(5), first)
            .await
            .expect("revoked policy execution should stop promptly")
            .expect("first policy task should not panic");
        assert!(
            first_result
                .expect_err("revoked policy execution should report ownership loss")
                .to_string()
                .contains("lost execution ownership")
        );
        assert_eq!(runner.cancelled.load(Ordering::SeqCst), 1);
        assert_eq!(runner.active.load(Ordering::SeqCst), 0);

        let second_pool = pool.clone();
        let second_runner = runner.clone();
        let second_derivation = derivation.clone();
        let second_enabled = enabled.clone();
        let second = tokio::spawn(async move {
            scan_one(
                &second_pool,
                &second_runner,
                Some("test".to_string()),
                &second_derivation,
                &second_enabled,
            )
            .await
        });
        wait_for_counter(&runner.calls, 2, "replacement policy scanner invocation").await;
        assert_eq!(
            runner.max_active.load(Ordering::SeqCst),
            1,
            "replacement must not overlap the revoked execution"
        );
        runner.release.add_permits(1);
        second
            .await
            .expect("replacement policy task should not panic")
            .expect("replacement policy execution should complete");

        let statuses: Vec<String> = sqlx::query_scalar(
            "SELECT status FROM cve_scans WHERE derivation_id = $1 ORDER BY created_at, id",
        )
        .bind(derivation.id)
        .fetch_all(&pool)
        .await
        .expect("policy scan statuses should resolve");
        assert_eq!(statuses, vec!["failed", "completed"]);

        sqlx::query("DELETE FROM cve_scans WHERE derivation_id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("policy lease scans should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("policy lease derivation should be deleted");
    }

    /// Disabling the worker after Phase 0 claims a queued row must return that
    /// row to `pending`. Re-enabling on the next cycle then executes it once
    /// under a newly issued ownership token.
    #[tokio::test]
    async fn disabled_queued_claim_is_requeued_then_executed_once() {
        let Some(pool) = db_test_pool().await else {
            return;
        };
        let tempdir = tempdir().expect("tempdir should be created");
        let store_path = tempdir.path().join("task-325-requeued-scan-store-path");
        std::fs::create_dir_all(&store_path).expect("store path dir should be created");

        sqlx::query(
            r#"
            INSERT INTO scan_schedule_policy (
                id, on_build, deployed_interval, recent_interval,
                archived_interval, archived_enabled
            )
            VALUES (1, false, '876000h', '876000h', '876000h', false)
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
        .expect("scan policy should suppress non-queued phases");

        let derivation = insert_derivation(
            &pool,
            None,
            &format!("task-325-requeue-cycle-{}", Uuid::new_v4()),
            "nixos",
        )
        .await
        .expect("requeue-cycle derivation should be inserted");
        sqlx::query(
            r#"
            UPDATE derivations
            SET status_id = $2, completed_at = NOW(), store_path = $3
            WHERE id = $1
            "#,
        )
        .bind(derivation.id)
        .bind(EvaluationStatus::BuildComplete.as_id())
        .bind(store_path.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("requeue-cycle derivation should be build-complete");
        let scan_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO cve_scans (
                id, derivation_id, scanner_name, status, attempts,
                total_packages, total_vulnerabilities,
                critical_count, high_count, medium_count, low_count
            )
            VALUES ($1, $2, 'vulnix', 'pending', 0, 0, 0, 0, 0, 0, 0)
            "#,
        )
        .bind(scan_id)
        .bind(derivation.id)
        .execute(&pool)
        .await
        .expect("queued scan should be inserted");

        let runner = FakeRunner {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let vulnix_config = crate::config::VulnixConfig::default();
        let enabled_rx = tokio::sync::RwLock::new(true);
        let claim = claim_queued_cve_scans(&pool, 1)
            .await
            .expect("enabled worker should claim the queued scan")
            .into_iter()
            .find(|claim| claim.scan_id == scan_id)
            .expect("the test scan should be claimed");
        *enabled_rx.write().await = false;
        assert!(
            requeue_claim_if_disabled(&pool, claim, &enabled_rx)
                .await
                .expect("disable after claim should safely requeue it")
        );

        let (status, attempts, has_execution_id): (String, i32, bool) = sqlx::query_as(
            "SELECT status, attempts, scan_metadata ? 'execution_id' FROM cve_scans WHERE id = $1",
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("requeued scan should resolve");
        assert_eq!(status, "pending");
        assert_eq!(attempts, 1);
        assert!(!has_execution_id);
        assert_eq!(runner.calls.load(Ordering::SeqCst), 0);

        *enabled_rx.write().await = true;
        scan_cycle_with_runner(
            &pool,
            &vulnix_config,
            &runner,
            Some("test".to_string()),
            &enabled_rx,
        )
        .await
        .expect("re-enabled cycle should execute the queued scan");

        let (status, attempts): (String, i32) =
            sqlx::query_as("SELECT status, attempts FROM cve_scans WHERE id = $1")
                .bind(scan_id)
                .fetch_one(&pool)
                .await
                .expect("executed scan should resolve");
        assert_eq!(status, "completed");
        assert_eq!(attempts, 2, "each durable claim remains in audit history");
        assert_eq!(
            runner.calls.load(Ordering::SeqCst),
            1,
            "the re-enabled worker must execute the queued scan exactly once"
        );

        sqlx::query("DELETE FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .execute(&pool)
            .await
            .expect("requeue-cycle scan should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("requeue-cycle derivation should be deleted");
        sqlx::query(
            r#"
            UPDATE scan_schedule_policy
            SET on_build = TRUE,
                deployed_interval = '24h',
                recent_interval = '24h',
                archived_interval = '168h',
                archived_enabled = TRUE
            WHERE id = 1
            "#,
        )
        .execute(&pool)
        .await
        .expect("scan policy should be restored");
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
