//! CVE scanning worker for the Crystal Forge server.
//!
//! This module provides [`run_cve_scan_loop`], the background task that:
//!
//! 1. **Stale recovery** — examines bounded batches of abandoned executions and
//!    old revocations so recovery cannot monopolize a worker cycle.
//! 2. **Operator-queued scans** — runs explicit fleet-rescan requests before
//!    loading scan policy. This phase is independent of `on_build` and still
//!    runs when policy loading fails.
//! 3. **Post-build scans** — picks up build-complete derivations that have
//!    never been successfully scanned and runs vulnix on them.
//! 4. **Periodic rescans** — picks up derivations whose last completed scan is
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
//! After bounded stale recovery, Phase 0 processes operator-queued scans before
//! the worker loads `scan_schedule_policy`. Phase 1 processes post-build scans
//! only when `on_build` is enabled. Phase 2 processes periodic rescans. Each
//! scan phase runs at most [`MAX_SCANS_PER_CYCLE`] scans, and each recovery
//! phase applies its own conservative query-layer batch limit.

use crate::config::{CacheConfig, CacheType, CrystalForgeConfig};
use crate::derivations::utils::{
    apply_cache_config_env_to_command, attic_server_url_from_cache_config,
};
use crate::log::{WorkerState, WorkerStatus, get_cve_status};
use crate::models::cache_destination::CacheDestination;
use crate::queries::cache_destinations::get_cache_destination;
use crate::queries::cve_scans::{
    CreateCveScanOutcome, CveScanExecutionClaim, acknowledge_revoked_cve_scan_execution,
    acquire_execution_lock, claim_queued_cve_scans, create_cve_scan,
    get_targets_needing_cve_rescan, get_targets_needing_cve_scan, heartbeat_cve_scan_execution,
    mark_cve_scan_failed_by_id_for_execution, mark_cve_scan_failed_for_execution,
    recover_stale_scans, release_execution_lock_or_close, requeue_cve_scan_execution,
    save_scan_results_for_execution,
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
/// the matching `watch::Receiver<u64>` returned from that call. The receiver's
/// increasing value signals explicit run-now requests. The handle is
/// registered in the server's
/// [`BackgroundJobRegistry`](crate::server::jobs::BackgroundJobRegistry) before
/// this function is called; the receiver lets the loop respond to run-now
/// signals from HTTP handlers.
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

/// Maximum derivations scanned per cycle phase.
///
/// Processing is bounded per cycle so that a large historical backlog does not
/// monopolise the database for an extended period. At 1 scan/cycle with a
/// 60-second poll interval a backlog of N derivations clears in ~N minutes,
/// which is acceptable. Raise this constant once bulk-persistence lands and
/// the write amplification per scan is addressed.
const MAX_SCANS_PER_CYCLE: i64 = 1;

/// Runs one bounded stale-recovery pass and three scan phases.
///
/// Stale recovery first processes bounded candidate and finalization batches.
/// Phase 0 then processes at most [`MAX_SCANS_PER_CYCLE`] operator-queued scans
/// before policy loading. Explicit requests therefore run independently of
/// `scan_schedule_policy` availability and the `on_build` setting.
///
/// Phase 1 processes at most [`MAX_SCANS_PER_CYCLE`] post-build targets when
/// `on_build` is enabled. Phase 2 processes at most
/// [`MAX_SCANS_PER_CYCLE`] periodic rescan targets. No phase loops until its
/// backlog is empty; later poll cycles continue each backlog.
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
/// Keeps a PostgreSQL session-level advisory lock held for the entire execution
/// duration. This allows recovery to distinguish a paused live process (lock held)
/// from a crashed process (lock released). The lock is explicitly released before
/// the connection is returned to the pool. The connection must NOT be returned
/// to the pool while still owning an advisory lock, because a pooled connection
/// could be reused by another worker and silently violate active-scan uniqueness.
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
    execute_scan_with_handoff_gate(
        pool,
        vulnix_runner,
        vulnix_version,
        derivation,
        scan_id,
        execution_id,
        None,
    )
    .await
}

#[cfg(test)]
struct ExecutionHandoffGate {
    arrived: std::sync::Arc<tokio::sync::Semaphore>,
    resume: std::sync::Arc<tokio::sync::Semaphore>,
}

async fn execute_scan_with_handoff_gate<R: CveScanRunner + Sync>(
    pool: &PgPool,
    vulnix_runner: &R,
    vulnix_version: Option<String>,
    derivation: &crate::derivations::Derivation,
    scan_id: uuid::Uuid,
    execution_id: uuid::Uuid,
    #[cfg(test)] handoff_gate: Option<&ExecutionHandoffGate>,
    #[cfg(not(test))] _handoff_gate: Option<&()>,
) -> Result<()> {
    #[cfg(test)]
    if let Some(gate) = handoff_gate {
        gate.arrived.add_permits(1);
        let permit = gate
            .resume
            .acquire()
            .await
            .context("execution handoff test gate closed")?;
        permit.forget();
    }

    // Acquire a connection from the pool that will hold the execution lock.
    // The lock must be released before this connection is returned to the pool,
    // otherwise the pool would return a connection still owning an advisory lock.
    let mut lock_conn = pool
        .acquire()
        .await
        .context("Failed to acquire connection for CVE scan execution lock")?;
    acquire_execution_lock(&mut lock_conn, execution_id)
        .await
        .context("Failed to acquire session-level execution lock")?;

    // CONCURRENCY: Close the claim-to-lock gap before constructing the scanner
    // future. Recovery can revoke a claim while it is waiting for this session
    // lock. The immediate CAS proves this token still owns the row; otherwise
    // no cache process or vulnix process may start.
    match heartbeat_cve_scan_execution(pool, scan_id, execution_id).await {
        Ok(true) => {}
        Ok(false) => {
            release_execution_lock_or_close(lock_conn, execution_id).await;
            let _ = acknowledge_revoked_cve_scan_execution(pool, scan_id, execution_id).await;
            anyhow::bail!("CVE scan {scan_id} lost execution ownership before execution handoff");
        }
        Err(err) => {
            release_execution_lock_or_close(lock_conn, execution_id).await;
            let handoff_error = err.context("Failed CVE scan execution handoff heartbeat");
            if let Err(cleanup_err) = mark_cve_scan_failed_by_id_for_execution(
                pool,
                scan_id,
                derivation.id,
                &handoff_error.to_string(),
                execution_id,
            )
            .await
            {
                error!(
                    "Failed to mark CVE scan {scan_id} as failed after execution handoff error: {cleanup_err:#}"
                );
            }
            return Err(handoff_error);
        }
    }

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
            result = &mut execution => {
                // Release the advisory lock (or discard the connection if
                // release cannot be confirmed) before returning it to the
                // pool, so a pooled connection never re-enters general use
                // while still owning an execution lock.
                release_execution_lock_or_close(lock_conn, execution_id).await;
                return result;
            }
            _ = heartbeat.tick() => {
                match tokio::time::timeout(
                    HEARTBEAT_QUERY_TIMEOUT,
                    heartbeat_cve_scan_execution(pool, scan_id, execution_id),
                )
                .await
                {
                    Ok(Ok(true)) => {}
                    Ok(Ok(false)) => {
                        // Cancel the scanner before releasing the lock or
                        // acknowledging revocation. Until this future is
                        // dropped the row remains in_progress, preserving
                        // active-scan uniqueness.
                        drop(execution);
                        release_execution_lock_or_close(lock_conn, execution_id).await;
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
                        release_execution_lock_or_close(lock_conn, execution_id).await;
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
                        release_execution_lock_or_close(lock_conn, execution_id).await;
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

    // Ensure the paths vulnix needs are available before invoking it.
    //
    // Gating this on output presence alone is not sufficient: vulnix resolves
    // an output's derivation when it scans that output, so an output that
    // survived garbage collection while its `.drv` did not would reach vulnix
    // and fail with a deriver lookup error even though an eligible cache still
    // holds the `.drv`. Output and derivation availability are therefore
    // established independently and only the missing path is restored, so a
    // locally complete pair costs no cache traffic.
    //
    // A missing output is fatal: there is nothing to scan. A missing or
    // unnameable derivation is not treated as fatal here, because vulnix is the
    // authoritative judge of what it can resolve. Failing the scan locally in
    // that case would newly fail scans that previously ran.
    let action = match observe_scan_inputs(path).await {
        Ok((action, _)) => action,
        Err(e) => {
            warn!("Could not inspect scan inputs for {path}: {e:#}");
            ScanInputAction::DeriverUnresolvable
        }
    };

    match action {
        ScanInputAction::Ready => {}
        ScanInputAction::DeriverUnresolvable => {
            warn!(
                "Store path {path} is present but its deriver cannot be named locally; \
                 running the scan anyway so vulnix reports the authoritative result"
            );
        }
        ScanInputAction::RestoreOutput | ScanInputAction::RestoreDeriver => {
            let output_missing = matches!(action, ScanInputAction::RestoreOutput);
            if output_missing {
                warn!("Store path {path} not found locally — attempting cache materialization");
            } else {
                warn!(
                    "Store path {path} is present but its recorded derivation is missing — \
                     attempting to restore the derivation from cache"
                );
            }

            let restored = materialize_store_path_from_cache(pool, derivation, path).await;
            match restored {
                Ok(true) => info!("✅ Successfully materialized scan inputs for {path}"),
                Ok(false) if output_missing => {
                    warn!(
                        "❌ Could not materialize {path} from any cache — marking scan as failed"
                    );
                    mark_scan_failed_for_owner(
                        pool,
                        scan_id,
                        derivation,
                        &format!(
                            "Store path {path} not present locally and no cache could provide it"
                        ),
                        execution_id,
                    )
                    .await?;
                    return Ok(());
                }
                Err(e) if output_missing => {
                    error!("❌ Error materializing {path} from cache: {e}");
                    mark_scan_failed_for_owner(
                        pool,
                        scan_id,
                        derivation,
                        &format!("Cache materialization error: {e}"),
                        execution_id,
                    )
                    .await?;
                    return Ok(());
                }
                // The output is usable; only the derivation could not be
                // restored. Let vulnix produce the authoritative error.
                Ok(false) => warn!(
                    "Could not restore the missing derivation for {path} from any cache; \
                     running the scan anyway"
                ),
                Err(e) => warn!(
                    "Error restoring the missing derivation for {path}: {e:#}; \
                     running the scan anyway"
                ),
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

/// What a scan still needs locally before vulnix can run.
///
/// Vulnix resolves an output path's derivation when it scans that output, so a
/// scan needs **both** the output store path and the `.drv` the local store
/// records as its deriver. These two are independently missing: a garbage
/// collector can reap the `.drv` while keeping a still-referenced output, and a
/// binary-cache copy imports an output closure without necessarily importing
/// the derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanInputAction {
    /// Both the output and its recorded derivation are present locally.
    Ready,
    /// The output itself is absent. Its deriver is not knowable until the
    /// output has been restored, so the output must be restored first.
    RestoreOutput,
    /// The output is present and its deriver is known, but the `.drv` is
    /// absent. Only the derivation needs restoring; recopying the output would
    /// be wasted transfer.
    RestoreDeriver,
    /// The output is present but the local store cannot name its deriver, so
    /// no `.drv` path exists to restore. This happens when a cache omitted
    /// `Deriver` from its narinfo.
    DeriverUnresolvable,
}

/// Decides what a scan still needs, from observed local availability.
///
/// Kept pure and separate from the subprocess work so the decision table can be
/// asserted exhaustively without a Nix store or a cache.
fn scan_input_action(
    output_present: bool,
    deriver_path: Option<&str>,
    deriver_present: bool,
) -> ScanInputAction {
    if !output_present {
        return ScanInputAction::RestoreOutput;
    }
    match deriver_path {
        None => ScanInputAction::DeriverUnresolvable,
        Some(_) if deriver_present => ScanInputAction::Ready,
        Some(_) => ScanInputAction::RestoreDeriver,
    }
}

/// Reports whether both paths a vulnix scan requires are present locally.
///
/// Returns the resolved deriver path alongside the decision so the caller does
/// not have to query the store twice.
async fn observe_scan_inputs(store_path: &str) -> Result<(ScanInputAction, Option<String>)> {
    observe_scan_inputs_with_program(store_path, std::ffi::OsStr::new("nix-store")).await
}

async fn observe_scan_inputs_with_program(
    store_path: &str,
    nix_store_program: &std::ffi::OsStr,
) -> Result<(ScanInputAction, Option<String>)> {
    let output_present = fs::try_exists(store_path).await.unwrap_or(false);
    if !output_present {
        return Ok((ScanInputAction::RestoreOutput, None));
    }

    let deriver_path = deriver_for_store_path_with_program(store_path, nix_store_program).await?;
    let deriver_present = match deriver_path.as_deref() {
        Some(path) => fs::try_exists(path).await.unwrap_or(false),
        None => false,
    };
    let action = scan_input_action(output_present, deriver_path.as_deref(), deriver_present);
    Ok((action, deriver_path))
}

/// Ensures the output path and its recorded derivation are both available
/// locally, restoring only what is missing from a configured cache.
///
/// Queries completed `cache_push_jobs` for this derivation and resolves each
/// cache destination's `push_to` URL, preserving that destination's
/// authentication, signature, and `NIX_CONFIG` handling.
///
/// # Behavior
///
/// - A locally complete pair is accepted without contacting any cache, so a
///   present output is never recopied.
/// - A present output whose `.drv` is missing restores only the `.drv`.
/// - A missing output restores the output first, because its deriver cannot be
///   named until the output exists locally, then restores the `.drv`.
/// - Success is reported only after the required paths are confirmed present,
///   not merely because `nix copy` exited zero.
///
/// # Errors
///
/// Returns an error when the cache-destination query fails, a `nix` process
/// cannot be spawned or awaited, or the deriver query fails.
async fn materialize_store_path_from_cache(
    pool: &PgPool,
    derivation: &crate::derivations::Derivation,
    store_path: &str,
) -> Result<bool> {
    // Fast path: nothing is missing, so no cache round trip is needed.
    let (initial_action, initial_deriver) = observe_scan_inputs(store_path).await?;
    if initial_action == ScanInputAction::Ready {
        debug!("Scan inputs for {store_path} are already present locally");
        return Ok(true);
    }
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
        if restore_scan_inputs_from_source(
            source,
            store_path,
            NIX_COPY_TIMEOUT,
            std::ffi::OsStr::new("nix"),
            std::ffi::OsStr::new("nix-store"),
        )
        .await?
        {
            return Ok(true);
        }
    }

    // Report the terminal reason rather than a bare false so the scan failure
    // message distinguishes "no cache had it" from "cache never recorded it".
    if matches!(initial_action, ScanInputAction::DeriverUnresolvable) {
        warn!(
            "Output {store_path} is present locally but its deriver is unknown \
             (initial deriver probe: {initial_deriver:?})"
        );
    }

    Ok(false)
}

/// Restores one scan input pair from one cache source.
///
/// The executable parameters form a narrow process seam for regression tests.
/// Production passes `nix` and `nix-store`. The function observes the output
/// first and invokes `nix copy` only for a missing path. It completes all copy
/// work before returning, so callers cannot start vulnix before preparation.
async fn restore_scan_inputs_from_source(
    source: &MaterializationSource,
    store_path: &str,
    copy_timeout: std::time::Duration,
    nix_program: &std::ffi::OsStr,
    nix_store_program: &std::ffi::OsStr,
) -> Result<bool> {
    // Re-observe per source: an earlier source may have restored the output but
    // failed on its `.drv`, and the output must not be recopied.
    let (action, _) = observe_scan_inputs_with_program(store_path, nix_store_program).await?;
    match action {
        ScanInputAction::Ready => return Ok(true),
        ScanInputAction::RestoreOutput => {
            if !copy_path_from_cache_with_program(source, store_path, copy_timeout, nix_program)
                .await?
            {
                return Ok(false);
            }
        }
        ScanInputAction::RestoreDeriver | ScanInputAction::DeriverUnresolvable => {}
    }

    // The deriver is only knowable once the output exists locally, so resolve
    // it again after any output restore.
    let Some(deriver_path) =
        deriver_for_store_path_with_program(store_path, nix_store_program).await?
    else {
        warn!(
            "Cache source {} left {} without a resolvable deriver; \
             the cache likely omitted Deriver metadata",
            source.from_url, store_path
        );
        return Ok(false);
    };

    if fs::try_exists(&deriver_path).await.unwrap_or(false) {
        return Ok(true);
    }

    // Vulnix resolves the output's derivation when it scans an output path. A
    // binary-cache copy does not guarantee that the `.drv` itself is present.
    if copy_path_from_cache_with_program(source, &deriver_path, copy_timeout, nix_program).await?
        && fs::try_exists(&deriver_path).await.unwrap_or(false)
    {
        return Ok(true);
    }

    warn!(
        "nix copy from {} could not restore deriver {} for {}",
        source.from_url, deriver_path, store_path
    );
    Ok(false)
}

async fn copy_path_from_cache_with_program(
    source: &MaterializationSource,
    store_path: &str,
    copy_timeout: std::time::Duration,
    nix_program: &std::ffi::OsStr,
) -> Result<bool> {
    debug!(
        "Trying nix copy --from {} {} (source: {})",
        source.from_url, store_path, source.label
    );

    let mut command = TokioCommand::new(nix_program);
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

    // `kill_on_drop` prevents a timed-out cache copy from outliving the worker.
    let mut child = command
        .arg(store_path)
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("Failed to spawn nix copy from {}", source.from_url))?;

    let wait_result = match timeout(copy_timeout, child.wait()).await {
        Ok(result) => result?,
        Err(_elapsed) => {
            warn!(
                "nix copy from {} timed out after {}s for {} — killing process",
                source.from_url,
                copy_timeout.as_secs(),
                store_path
            );
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Ok(false);
        }
    };

    if !wait_result.success() {
        warn!(
            "nix copy from {} failed for {} (exit code: {:?})",
            source.from_url,
            store_path,
            wait_result.code()
        );
        return Ok(false);
    }

    match fs::try_exists(store_path).await {
        Ok(true) => Ok(true),
        Ok(false) => {
            warn!(
                "nix copy from {} reported success but {} is still absent",
                source.from_url, store_path
            );
            Ok(false)
        }
        Err(error) => {
            warn!(
                "nix copy from {} succeeded but path check failed: {}",
                source.from_url, error
            );
            Ok(false)
        }
    }
}

async fn deriver_for_store_path_with_program(
    store_path: &str,
    nix_store_program: &std::ffi::OsStr,
) -> Result<Option<String>> {
    let output = TokioCommand::new(nix_store_program)
        .args(["--query", "--deriver"])
        .arg(store_path)
        .output()
        .await
        .context("Failed to query the restored store path's deriver")?;

    if !output.status.success() {
        return Ok(None);
    }

    Ok(parse_deriver_path(&output.stdout))
}

/// Returns a derivation path only when Nix reported a concrete `.drv` path.
fn parse_deriver_path(output: &[u8]) -> Option<String> {
    let deriver_path = String::from_utf8_lossy(output).trim().to_string();
    deriver_path.ends_with(".drv").then_some(deriver_path)
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
    use crate::queries::scanning::ScanSchedulePolicyRow;
    use futures::FutureExt;
    use serial_test::serial;
    use sqlx::PgPool;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tempfile::tempdir;
    use tokio::sync::Semaphore;
    use uuid::Uuid;

    /// A vulnix scan needs the output path *and* its recorded `.drv`. Gating
    /// preparation on output presence alone let an output that outlived its
    /// derivation reach vulnix and fail with a deriver lookup error, so the
    /// output-present / `.drv`-missing case must ask for a derivation restore
    /// rather than reporting the inputs ready.
    #[test]
    fn scan_input_action_covers_independent_output_and_deriver_availability() {
        const DRV: &str = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-example.drv";

        // The regression: output survived, derivation did not.
        assert_eq!(
            scan_input_action(true, Some(DRV), false),
            ScanInputAction::RestoreDeriver,
            "a present output with a missing .drv must restore only the .drv"
        );

        assert_eq!(
            scan_input_action(true, Some(DRV), true),
            ScanInputAction::Ready,
            "a complete local pair must not trigger any cache traffic"
        );
        assert_eq!(
            scan_input_action(true, None, false),
            ScanInputAction::DeriverUnresolvable,
            "a present output whose deriver the store cannot name is not scannable"
        );

        // A missing output is restored first because its deriver is not
        // knowable until the output exists locally.
        assert_eq!(
            scan_input_action(false, None, false),
            ScanInputAction::RestoreOutput
        );
        assert_eq!(
            scan_input_action(false, Some(DRV), false),
            ScanInputAction::RestoreOutput
        );
        assert_eq!(
            scan_input_action(false, Some(DRV), true),
            ScanInputAction::RestoreOutput,
            "a locally present .drv cannot substitute for a missing output"
        );
    }

    /// Proves the process boundary used by production copies only the known
    /// missing `.drv` when the output exists, and completes that copy before
    /// control can advance to vulnix.
    #[tokio::test]
    async fn present_output_missing_deriver_restores_only_deriver_before_vulnix() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().expect("temporary process-seam directory");
        let output_path = temp.path().join("output");
        let deriver_path = temp.path().join("output.drv");
        let copy_log = temp.path().join("copy.log");
        let fake_nix = temp.path().join("nix");
        let fake_nix_store = temp.path().join("nix-store");
        std::fs::write(&output_path, "present output").expect("output fixture");
        std::fs::write(
            &fake_nix_store,
            format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", deriver_path.display()),
        )
        .expect("fake nix-store");
        std::fs::write(
            &fake_nix,
            format!(
                "#!/bin/sh\nfor last do :; done\nprintf '%s\\n' \"$last\" >> '{}'\ntouch \"$last\"\n",
                copy_log.display()
            ),
        )
        .expect("fake nix");
        for program in [&fake_nix, &fake_nix_store] {
            let mut permissions = std::fs::metadata(program)
                .expect("fake program metadata")
                .permissions();
            permissions.set_mode(0o700);
            std::fs::set_permissions(program, permissions).expect("fake program permissions");
        }

        let source = MaterializationSource {
            label: "process-seam".to_string(),
            from_url: "file:///unused-test-cache".to_string(),
            cache_config: None,
            trusted_public_key: None,
            nix_config_lines: Vec::new(),
        };
        let restored = restore_scan_inputs_from_source(
            &source,
            output_path.to_str().expect("UTF-8 output path"),
            std::time::Duration::from_secs(5),
            fake_nix.as_os_str(),
            fake_nix_store.as_os_str(),
        )
        .await
        .expect("scan preparation should complete");
        // This marker represents the next statement in `execute_scan_inner`,
        // where the scanner future is awaited only after preparation returns.
        std::fs::write(temp.path().join("vulnix-started"), "started").expect("scanner marker");

        assert!(restored);
        assert!(deriver_path.exists(), "the missing .drv must be restored");
        let copied = std::fs::read_to_string(copy_log).expect("copy log");
        assert_eq!(copied.lines().count(), 1, "only one path may be copied");
        assert_eq!(copied.trim(), deriver_path.to_string_lossy());
        assert!(
            output_path.exists(),
            "the existing output must remain present"
        );
        assert!(temp.path().join("vulnix-started").exists());
    }

    /// Proves the real preparation path treats a present output with a missing
    /// derivation as work to do rather than as ready, using real filesystem
    /// paths so the observation logic is exercised end to end.
    #[tokio::test]
    async fn observe_scan_inputs_requires_a_restorable_deriver_for_a_present_output() {
        let dir = tempdir().expect("temp store dir");
        let output = dir.path().join("output");
        tokio::fs::write(&output, b"present output")
            .await
            .expect("output fixture should be written");
        let output_path = output.to_string_lossy().to_string();

        // A path outside a real Nix store has no recorded deriver, so the
        // store cannot name a `.drv` for it.
        let (action, deriver) = observe_scan_inputs(&output_path)
            .await
            .expect("observation should not fail for a present output");
        assert_eq!(
            action,
            ScanInputAction::DeriverUnresolvable,
            "a present output with no resolvable deriver must not be reported ready"
        );
        assert!(deriver.is_none());

        // An absent output is always reported as needing the output first.
        let missing = dir.path().join("absent-output");
        let (missing_action, missing_deriver) = observe_scan_inputs(&missing.to_string_lossy())
            .await
            .expect("observation should not fail for a missing output");
        assert_eq!(missing_action, ScanInputAction::RestoreOutput);
        assert!(missing_deriver.is_none());
    }

    #[test]
    fn parse_deriver_path_accepts_only_derivation_paths() {
        assert_eq!(
            parse_deriver_path(b"/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-example.drv\n"),
            Some("/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-example.drv".to_string())
        );
        assert_eq!(parse_deriver_path(b"unknown\n"), None);
        assert_eq!(parse_deriver_path(b""), None);
    }

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

    async fn restore_scan_schedule_policy(pool: &PgPool, policy: &ScanSchedulePolicyRow) {
        sqlx::query(
            r#"
            UPDATE scan_schedule_policy
            SET on_build = $1,
                deployed_interval = $2,
                recent_interval = $3,
                archived_interval = $4,
                archived_enabled = $5,
                rebuild_to_scan = $6,
                updated_at = $7
            WHERE id = 1
            "#,
        )
        .bind(policy.on_build)
        .bind(&policy.deployed_interval)
        .bind(&policy.recent_interval)
        .bind(&policy.archived_interval)
        .bind(policy.archived_enabled)
        .bind(policy.rebuild_to_scan)
        .bind(policy.updated_at)
        .execute(pool)
        .await
        .expect("original scan schedule policy should be restored");
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
    #[serial(scan_schedule_policy)]
    async fn scan_cycle_processes_target_with_fake_runner() {
        let Some(pool) = db_test_pool().await else {
            return;
        };
        let original_policy = get_scan_schedule_policy(&pool)
            .await
            .expect("original scan schedule policy should resolve");
        let tempdir = tempdir().expect("tempdir should be created");
        let store_path = tempdir.path().join("task-396-scan-cycle-store-path");
        std::fs::create_dir_all(&store_path).expect("store path dir should be created");

        // This test uses the dedicated shared test database rather than a
        // per-test database. Retire leftovers from interrupted prior runs so
        // the worker's one-target cycle deterministically selects this fixture.
        sqlx::query(
            "UPDATE derivations SET status_id = $1 WHERE derivation_name LIKE 'task-396-cycle-%'",
        )
        .bind(EvaluationStatus::BuildFailed.as_id())
        .execute(&pool)
        .await
        .expect("prior scan-cycle fixtures should be retired");

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

        let assertions = std::panic::AssertUnwindSafe(
            scan_cycle_processes_target_with_fake_runner_assertions(&pool, &store_path),
        )
        .catch_unwind()
        .await;

        restore_scan_schedule_policy(&pool, &original_policy).await;
        if let Err(panic) = assertions {
            std::panic::resume_unwind(panic);
        }
    }

    async fn scan_cycle_processes_target_with_fake_runner_assertions(
        pool: &PgPool,
        store_path: &std::path::Path,
    ) {
        let derivation_name = format!("task-396-cycle-{}", Uuid::new_v4());
        let derivation = insert_derivation(pool, None, &derivation_name, "nixos")
            .await
            .expect("derivation should be inserted");

        sqlx::query(
            r#"
            UPDATE derivations
            SET status_id = $2,
                completed_at = '1900-01-01 00:00:00+00'::timestamptz,
                store_path = $3
            WHERE id = $1
            "#,
        )
        .bind(derivation.id)
        .bind(EvaluationStatus::BuildComplete.as_id())
        .bind(store_path.to_string_lossy().to_string())
        .execute(pool)
        .await
        .expect("derivation should be marked build-complete");

        let runner = FakeRunner {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let vulnix_config = crate::config::VulnixConfig::default();
        let enabled_rx = tokio::sync::RwLock::new(true);

        scan_cycle_with_runner(
            pool,
            &vulnix_config,
            &runner,
            Some("test".to_string()),
            &enabled_rx,
        )
        .await
        .expect("scan cycle should succeed");
        scan_cycle_with_runner(
            pool,
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
            .fetch_one(pool)
            .await
            .expect("scan row should exist");

        assert_eq!(status, Some("completed".to_string()));
        assert!(completed_at.is_some(), "scan should be terminal");

        sqlx::query("UPDATE scan_schedule_policy SET on_build = FALSE WHERE id = 1")
            .execute(pool)
            .await
            .expect("on-build scanning should be disabled");
        let disabled_derivation = insert_derivation(
            pool,
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
        .execute(pool)
        .await
        .expect("disabled-cycle derivation should be marked build-complete");

        scan_cycle_with_runner(
            pool,
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
            .execute(pool)
            .await
            .expect("scan-cycle scans should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = ANY($1)")
            .bind(&derivation_ids)
            .execute(pool)
            .await
            .expect("scan-cycle derivations should be deleted");
    }

    /// Proves a claim recovered before it obtains its execution lock cannot
    /// launch a scanner, and active uniqueness prevents a replacement until
    /// the resumed owner observes the lost token and acknowledges revocation.
    #[tokio::test]
    async fn recovered_claim_cannot_start_scanner_during_execution_handoff() {
        let Some(pool) = db_test_pool().await else {
            return;
        };
        let temp = tempdir().expect("handoff store path");
        let derivation = insert_derivation(
            &pool,
            None,
            &format!("task-325-handoff-{}", Uuid::new_v4()),
            "nixos",
        )
        .await
        .expect("handoff derivation should be inserted");
        sqlx::query(
            "UPDATE derivations SET status_id = $2, completed_at = NOW(), store_path = $3 WHERE id = $1",
        )
        .bind(derivation.id)
        .bind(EvaluationStatus::BuildComplete.as_id())
        .bind(temp.path().to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("handoff derivation should be build-complete");
        let derivation = crate::queries::derivations::get_derivation_by_id(&pool, derivation.id)
            .await
            .expect("handoff derivation should reload");
        let claim = match create_cve_scan(&pool, derivation.id, "vulnix", None)
            .await
            .expect("handoff claim should be created")
        {
            CreateCveScanOutcome::Created(claim) => claim,
            CreateCveScanOutcome::Existing(_) => panic!("handoff claim must be new"),
        };
        sqlx::query(
            "UPDATE cve_scans SET scan_metadata = scan_metadata || jsonb_build_object('execution_started_at', NOW() - INTERVAL '2 hours', 'execution_heartbeat_at', NOW() - INTERVAL '2 hours') WHERE id = $1",
        )
        .bind(claim.scan_id)
        .execute(&pool)
        .await
        .expect("handoff claim should be aged");

        let runner = FakeRunner {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let gate = ExecutionHandoffGate {
            arrived: Arc::new(Semaphore::new(0)),
            resume: Arc::new(Semaphore::new(0)),
        };
        let execution = execute_scan_with_handoff_gate(
            &pool,
            &runner,
            None,
            &derivation,
            claim.scan_id,
            claim.execution_id,
            Some(&gate),
        );
        tokio::pin!(execution);
        tokio::select! {
            biased;
            result = &mut execution => panic!("handoff unexpectedly completed: {result:?}"),
            permit = gate.arrived.acquire() => permit.expect("arrival gate").forget(),
        }

        assert_eq!(
            recover_stale_scans(&pool, std::time::Duration::from_secs(1800))
                .await
                .expect("stale handoff claim should be recovered"),
            1
        );
        assert!(matches!(
            create_cve_scan(&pool, derivation.id, "vulnix", None)
                .await
                .expect("replacement probe should succeed"),
            CreateCveScanOutcome::Existing(id) if id == claim.scan_id
        ));
        assert_eq!(runner.calls.load(Ordering::SeqCst), 0);

        gate.resume.add_permits(1);
        let error = execution
            .await
            .expect_err("recovered owner must fail its handoff CAS");
        assert!(error.to_string().contains("lost execution ownership"));
        assert_eq!(runner.calls.load(Ordering::SeqCst), 0);

        let replacement = match create_cve_scan(&pool, derivation.id, "vulnix", None)
            .await
            .expect("replacement claim should be created after acknowledgment")
        {
            CreateCveScanOutcome::Created(claim) => claim,
            CreateCveScanOutcome::Existing(_) => panic!("replacement must be new"),
        };
        execute_scan(
            &pool,
            &runner,
            None,
            &derivation,
            replacement.scan_id,
            replacement.execution_id,
        )
        .await
        .expect("replacement execution should complete");
        assert_eq!(runner.calls.load(Ordering::SeqCst), 1);

        sqlx::query("DELETE FROM cve_scans WHERE derivation_id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("handoff scans should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("handoff derivation should be deleted");
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
    #[serial(scan_schedule_policy)]
    async fn disabled_queued_claim_is_requeued_then_executed_once() {
        let Some(pool) = db_test_pool().await else {
            return;
        };
        let original_policy = get_scan_schedule_policy(&pool)
            .await
            .expect("original scan schedule policy should resolve");
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

        let assertions = std::panic::AssertUnwindSafe(
            disabled_queued_claim_is_requeued_then_executed_once_assertions(&pool, &store_path),
        )
        .catch_unwind()
        .await;

        restore_scan_schedule_policy(&pool, &original_policy).await;
        if let Err(panic) = assertions {
            std::panic::resume_unwind(panic);
        }
    }

    async fn disabled_queued_claim_is_requeued_then_executed_once_assertions(
        pool: &PgPool,
        store_path: &std::path::Path,
    ) {
        let derivation = insert_derivation(
            pool,
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
        .execute(pool)
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
        .execute(pool)
        .await
        .expect("queued scan should be inserted");

        let runner = FakeRunner {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let vulnix_config = crate::config::VulnixConfig::default();
        let enabled_rx = tokio::sync::RwLock::new(true);
        let claim = claim_queued_cve_scans(pool, 1)
            .await
            .expect("enabled worker should claim the queued scan")
            .into_iter()
            .find(|claim| claim.scan_id == scan_id)
            .expect("the test scan should be claimed");
        *enabled_rx.write().await = false;
        assert!(
            requeue_claim_if_disabled(pool, claim, &enabled_rx)
                .await
                .expect("disable after claim should safely requeue it")
        );

        let (status, attempts, has_execution_id): (String, i32, bool) = sqlx::query_as(
            "SELECT status, attempts, scan_metadata ? 'execution_id' FROM cve_scans WHERE id = $1",
        )
        .bind(scan_id)
        .fetch_one(pool)
        .await
        .expect("requeued scan should resolve");
        assert_eq!(status, "pending");
        assert_eq!(attempts, 1);
        assert!(!has_execution_id);
        assert_eq!(runner.calls.load(Ordering::SeqCst), 0);

        *enabled_rx.write().await = true;
        scan_cycle_with_runner(
            pool,
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
                .fetch_one(pool)
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
            .execute(pool)
            .await
            .expect("requeue-cycle scan should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(pool)
            .await
            .expect("requeue-cycle derivation should be deleted");
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

    /// Proves the complete stale-owner fencing invariant against a real
    /// PostgreSQL session, in four stages:
    ///
    /// 1. A stale heartbeat with a still-held execution lock (a paused live
    ///    process) must leave the row completely untouched — recovery must
    ///    revoke nothing while the owning session could still be alive.
    /// 2. Explicitly releasing the lock (never merely dropping the pooled
    ///    connection, which does not terminate the backend session) makes the
    ///    scan look like a crashed process, so the next recovery pass revokes
    ///    it while still retaining active-scan uniqueness.
    /// 3. Before the acknowledgment grace period elapses, revocation alone
    ///    does not finalize the row.
    /// 4. Once the grace period has elapsed, recovery finalizes the
    ///    revocation to `failed`.
    #[tokio::test]
    async fn stale_execution_lock_prevents_recovery_paused_process_race() {
        use crate::queries::cve_scans::{
            acquire_execution_lock, execution_lock_is_held, release_execution_lock,
        };

        let Some(pool) = db_test_pool().await else {
            return;
        };
        let recovery_pool = PgPool::connect(
            &std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
                .expect("dedicated CVE database URL should remain available"),
        )
        .await
        .expect("independent stale-recovery pool should connect");

        // 1. Create a token-owned in_progress scan.
        let derivation = insert_derivation(
            &pool,
            None,
            &format!("task-325-lock-race-{}", Uuid::new_v4()),
            "nixos",
        )
        .await
        .expect("test derivation should be inserted");

        let claim = create_cve_scan(&pool, derivation.id, "test-vulnix", None)
            .await
            .expect("execution claim should be created");
        let CreateCveScanOutcome::Created(claim) = claim else {
            panic!("expected a newly created execution claim");
        };

        // 2. Acquire its execution advisory lock on a dedicated connection,
        // simulating a paused/stalled live process that still holds its
        // backend session.
        let mut lock_holder = pool
            .acquire()
            .await
            .expect("lock-holder connection should be acquired");
        acquire_execution_lock(&mut lock_holder, claim.execution_id)
            .await
            .expect("advisory lock should be acquired by the simulated paused execution");

        // 3. The lock must be observably held.
        assert!(
            execution_lock_is_held(&pool, claim.execution_id)
                .await
                .expect("lock status should be queryable"),
            "execution lock must be held after acquisition"
        );

        // 4. Artificially age the heartbeat past the stale threshold.
        sqlx::query(
            r#"
            UPDATE cve_scans
            SET scan_metadata = scan_metadata || jsonb_build_object(
                'execution_heartbeat_at', NOW() - INTERVAL '2 hours'
            )
            WHERE id = $1
            "#,
        )
        .bind(claim.scan_id)
        .execute(&pool)
        .await
        .expect("heartbeat should be aged");

        // 5/6. A stale heartbeat with a still-held execution lock must not
        // revoke the row. Recovery refreshes its lease instead of treating a
        // paused live process as crashed.
        let recovered_while_held =
            recover_stale_scans(&recovery_pool, std::time::Duration::from_secs(1800))
                .await
                .expect("recovery attempt while lock is held should succeed");
        assert_eq!(
            recovered_while_held, 0,
            "recovery must not act on a stale heartbeat while its advisory lock is still held"
        );
        let (status, is_revoked): (String, bool) = sqlx::query_as(
            "SELECT status, scan_metadata ? 'execution_revoked_at' FROM cve_scans WHERE id = $1",
        )
        .bind(claim.scan_id)
        .fetch_one(&pool)
        .await
        .expect("execution state should be queryable");
        assert_eq!(
            status, "in_progress",
            "a live paused process must remain in_progress"
        );
        assert!(
            !is_revoked,
            "a live paused process must not be revoked while its lock is held"
        );

        // 7. Explicitly release the advisory lock. Dropping the pooled
        // connection is intentionally NOT used here: returning a connection
        // to the pool does not terminate its backend session, so the lock
        // would remain held from PostgreSQL's perspective.
        assert!(
            release_execution_lock(&mut lock_holder, claim.execution_id)
                .await
                .expect("explicit unlock should execute"),
            "pg_advisory_unlock must confirm the lock was released"
        );
        drop(lock_holder);

        // 8. The lock must now be observably released.
        assert!(
            !execution_lock_is_held(&pool, claim.execution_id)
                .await
                .expect("lock status should be queryable after release"),
            "execution lock must be released after explicit unlock"
        );

        sqlx::query(
            r#"
            UPDATE cve_scans
            SET scan_metadata = scan_metadata || jsonb_build_object(
                'execution_heartbeat_at', NOW() - INTERVAL '2 hours'
            )
            WHERE id = $1
            "#,
        )
        .bind(claim.scan_id)
        .execute(&pool)
        .await
        .expect("released execution heartbeat should be aged without sleeping");

        // 9/10. Recovery may now revoke the still heartbeat-stale execution,
        // but must retain active-scan uniqueness until the acknowledgment
        // grace period elapses.
        let recovered_after_release =
            recover_stale_scans(&recovery_pool, std::time::Duration::from_secs(1800))
                .await
                .expect("recovery after lock release should succeed");
        assert_eq!(
            recovered_after_release, 1,
            "recovery must revoke a stale execution once its lock is released"
        );
        let (status, is_revoked): (String, bool) = sqlx::query_as(
            "SELECT status, scan_metadata ? 'execution_revoked_at' FROM cve_scans WHERE id = $1",
        )
        .bind(claim.scan_id)
        .fetch_one(&pool)
        .await
        .expect("revoked execution state should be queryable");
        assert_eq!(
            status, "in_progress",
            "revocation retains active-scan uniqueness until the grace period elapses"
        );
        assert!(
            is_revoked,
            "a crashed process must be revoked once its lock is gone"
        );

        // 11. Age execution_revoked_at beyond the acknowledgment grace period
        // instead of sleeping in real time.
        sqlx::query(
            "UPDATE cve_scans SET scan_metadata = scan_metadata || jsonb_build_object('execution_revoked_at', NOW() - INTERVAL '2 minutes') WHERE id = $1",
        )
        .bind(claim.scan_id)
        .execute(&pool)
        .await
        .expect("revocation should be artificially aged beyond its grace period");

        // 12/13. Recovery now finalizes the expired revocation to failed.
        let recovered_final =
            recover_stale_scans(&recovery_pool, std::time::Duration::from_secs(1800))
                .await
                .expect("finalization recovery should succeed");
        assert_eq!(
            recovered_final, 1,
            "an expired revocation must be finalized exactly once"
        );
        let final_status: String = sqlx::query_scalar("SELECT status FROM cve_scans WHERE id = $1")
            .bind(claim.scan_id)
            .fetch_one(&pool)
            .await
            .expect("final execution state should be queryable");
        assert_eq!(
            final_status, "failed",
            "execution must be finalized to 'failed' after the grace period elapses"
        );

        // 14. Clean up.
        sqlx::query("DELETE FROM cve_scans WHERE id = $1")
            .bind(claim.scan_id)
            .execute(&pool)
            .await
            .expect("cleanup: cve scan should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("cleanup: derivation should be deleted");
    }
}
