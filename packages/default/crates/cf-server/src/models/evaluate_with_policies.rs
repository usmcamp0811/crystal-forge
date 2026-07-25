use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::{Duration, Instant};

const MOCK_EVAL_TOTAL_DURATION_MS: u64 = 30_000;
const MOCK_EVAL_MIN_PER_SYSTEM_MS: u64 = 5_000;
const MOCK_EVAL_STAGE_COUNT: u64 = 5;
const EVAL_OUTPUT_IDLE_TIMEOUT_SECS: u64 = 300;
const EVAL_PROGRESS_HEARTBEAT_SECS: u64 = 30;
const CLOSURE_COUNT_MAX_CONCURRENT: usize = 2;
/// Maximum number of missing systems to attempt individual fallback evaluation
/// for. Beyond this threshold, treat as a likely process-wide evaluator failure
/// and retry the commit rather than spawning dozens of standalone evaluations.
const MAX_INDIVIDUAL_FALLBACKS: usize = 8;
/// Maximum concurrent fallback evaluations.
const FALLBACK_CONCURRENCY: usize = 2;
/// Overall deadline for the fallback phase.
const FALLBACK_PHASE_TIMEOUT: Duration = Duration::from_secs(180);
static CLOSURE_COUNT_LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();
use tracing::{debug, error, info, warn};

use crate::config::{BuildConfig, ServerConfig};
use crate::derivations::utils::{build_flake_reference, count_closure_packages};
use crate::flake::credentials::FlakeCredentialEnv;
use crate::models::commits::Commit;
use crate::models::deployment_policies::{
    DeploymentPolicy, PolicyCheckResult, build_nix_eval_expression,
};
use crate::models::flakes::Flake;
use crate::queries::build_jobs::enqueue_build_job_for_derivation;
use crate::queries::commits_artifacts::CachedSystemsState;
use crate::queries::derivations::{
    insert_derivation_with_target, mark_derivation_dry_run_complete, record_synthetic_eval_failure,
    set_closure_counts, set_expected_store_path, update_derivation_status, EvaluationStatus,
};
use crate::queries::systems::list_configuration_names_for_flake;
use crate::queue::QueueNotifier;
use crate::services::hardening_scans::trigger_immediate_hardening_scan;

fn closure_count_limiter() -> Arc<Semaphore> {
    CLOSURE_COUNT_LIMITER
        .get_or_init(|| Arc::new(Semaphore::new(CLOSURE_COUNT_MAX_CONCURRENT)))
        .clone()
}

/// NixEvalJobResult with meta field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NixEvalJobResult {
    pub attr: String,
    #[serde(rename = "attrPath")]
    pub attr_path: Vec<String>,
    /// Name is optional because nix-eval-jobs may omit it on errors
    pub name: Option<String>,
    #[serde(rename = "drvPath")]
    pub drv_path: Option<String>,
    pub error: Option<String>,
    #[serde(rename = "cacheStatus")]
    pub cache_status: Option<String>,
    pub outputs: Option<serde_json::Value>,

    /// Meta field (only present with --meta flag)
    /// Contains our policy check results in meta.policies
    pub meta: Option<serde_json::Value>,
}

fn parse_expected_store_path_from_outputs(outputs: &serde_json::Value) -> Option<String> {
    // Keep parsing intentionally strict: only use the canonical "out" output.
    // Broad scanning can pick unrelated store paths and corrupt expected_path data.
    if let Some(path) = outputs
        .get("out")
        .and_then(|out| out.get("path").or_else(|| out.get("outPath")))
        .and_then(|v| v.as_str())
        .or_else(|| outputs.get("out").and_then(|v| v.as_str()))
        .or_else(|| outputs.get("outPath").and_then(|v| v.as_str()))
    {
        if path.starts_with("/nix/store/") {
            return Some(path.to_string());
        }
    }

    None
}

async fn resolve_expected_store_path(
    drv_path: &str,
    outputs: Option<&serde_json::Value>,
) -> Option<String> {
    // Authoritative source: ask nix-store for outputs of this exact drv.
    let output = Command::new("nix-store")
        .args(["--query", "--outputs", drv_path])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        warn!(
            "Failed to resolve expected store path via nix-store for drv {}: {}",
            drv_path,
            if stderr.is_empty() {
                "<no stderr>"
            } else {
                &stderr
            }
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();

    if path.starts_with("/nix/store/") {
        Some(path)
    } else {
        if let Some(outputs) = outputs {
            if let Some(fallback) = parse_expected_store_path_from_outputs(outputs) {
                warn!(
                    "nix-store returned non-store output for drv {}, using outputs JSON fallback",
                    drv_path
                );
                return Some(fallback);
            }
        }

        warn!(
            "Could not resolve expected store path from nix-store output for drv {}: {}",
            drv_path,
            stdout.trim()
        );
        None
    }
}

/// Helper function to broadcast eval log via WebSocket AND persist to database.
///
/// This ensures logs are both:
/// - Streamed in real-time to connected WebSocket clients
/// - Persisted to eval_logs table for historical access
async fn broadcast_and_persist_eval_log(
    pool: &PgPool,
    state: Option<&crate::handlers::agent_request::CFState>,
    commit_id: i32,
    sequence: &mut i32,
    message: String,
) {
    let lower = message.to_ascii_lowercase();

    // Broadcast via WebSocket (existing infrastructure)
    if let Some(state) = state {
        crate::handlers::api::commits::broadcast_eval_log(state, commit_id, message.clone()).await;
    }

    // Persist to database (new functionality)
    // Parse log level from message format if possible
    let log_level = if message.starts_with("❌") || lower.contains("error") {
        Some("error")
    } else if message.starts_with("⚠️") || lower.contains("warning") {
        Some("warn")
    } else if message.starts_with("✅") {
        Some("info")
    } else if message.starts_with("🐛") || message.starts_with("DEBUG:") {
        Some("debug")
    } else {
        Some("info")
    };

    if let Err(e) =
        crate::queries::eval_logs::insert_eval_log(pool, commit_id, *sequence, log_level, &message)
            .await
    {
        warn!("Failed to persist eval log for commit {}: {}", commit_id, e);
    }

    *sequence += 1;
}

/// Outcome of a standalone fallback evaluation for a single system.
#[derive(Debug)]
enum FallbackEvalOutcome {
    /// The system genuinely failed standalone evaluation — this is a real
    /// NixOS configuration error that should be persisted as DryRunFailed.
    ConfirmedSystemFailure { error: String },
    /// The system evaluated successfully standalone. This indicates the
    /// original nix-eval-jobs process omission was NOT caused by a broken
    /// configuration, but likely by an evaluator-level issue (OOM, crash,
    /// intermittent network error). The evaluation should be retried rather
    /// than synthesizing a per-system failure.
    StandaloneEvaluationSucceeded,
    /// The fallback process could not be started or timed out. This is an
    /// infrastructure problem, not a system failure.
    InfrastructureFailure { error: String },
}

/// Attempt to evaluate a single nixosConfiguration attribute to capture its error.
///
/// When nix-eval-jobs silently drops a system, the process-wide stderr may not
/// contain that system's error at all, or may conflate errors. This function
/// individually evaluates one attribute to get an accurate per-system error.
///
/// Returns a typed `FallbackEvalOutcome` so the caller can distinguish a
/// genuine system failure from infrastructure or evaluator-level issues.
async fn fallback_eval_single_system(
    repo_url: &str,
    commit_hash: &str,
    system_name: &str,
    creds: Option<&FlakeCredentialEnv>,
) -> FallbackEvalOutcome {
    let flake_ref = build_flake_reference(repo_url, commit_hash);

    // Use --argstr to pass flakeRef and systemName safely instead of
    // interpolating values into a Nix expression string. This avoids
    // escaping issues with dots, backslashes, ${...}, etc.
    let nix_expr = r#"
{ flakeRef, systemName }:
let
  flake = builtins.getFlake flakeRef;
  cfg = builtins.getAttr systemName flake.nixosConfigurations;
in
  cfg.config.system.build.toplevel.drvPath
"#;

    let mut cmd = tokio::process::Command::new("nix");
    cmd.args([
        "eval",
        "--impure",
        "--expr",
        nix_expr.trim(),
        "--argstr",
        "flakeRef",
        &flake_ref,
        "--argstr",
        "systemName",
        system_name,
    ]);

    if let Some(c) = creds {
        c.apply_to_nix_command(&mut cmd);
    }

    let output = match tokio::time::timeout(Duration::from_secs(120), cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            return FallbackEvalOutcome::InfrastructureFailure {
                error: format!("Failed to run fallback eval: {}", e),
            }
        }
        Err(_) => {
            return FallbackEvalOutcome::InfrastructureFailure {
                error: format!("Fallback eval timed out for {}", system_name),
            }
        }
    };

    if output.status.success() {
        // The system actually evaluates fine — this is an evaluator omission,
        // not a broken NixOS configuration.
        return FallbackEvalOutcome::StandaloneEvaluationSucceeded;
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let error = if stderr.trim().is_empty() {
        "System evaluation failed with no error output".to_string()
    } else {
        stderr.chars().take(500).collect::<String>()
    };

    FallbackEvalOutcome::ConfirmedSystemFailure { error }
}

/// Evaluate a flake's nixosConfigurations with nix-eval-jobs and policy checking
///
/// FIXED: Now properly:
/// 1. Stores derivation_path from nix-eval-jobs
/// 2. Updates status to DryRunComplete after successful evaluation
pub async fn evaluate_with_nix_eval_jobs(
    pool: &PgPool,
    commit: &Commit,
    flake: &Flake,
    repo_url: &str,
    commit_hash: &str,
    target_system: &str,
    build_config: &BuildConfig,
    server_config: &ServerConfig,
    policies: &[DeploymentPolicy],
    cf_state: Option<&crate::handlers::agent_request::CFState>,
    queue_notifier: Option<&QueueNotifier>,
) -> Result<(Vec<NixEvalJobResult>, Vec<PolicyCheckResult>, bool)> {
    // The returned bool is `had_system_eval_errors` — whether any system had a
    // Nix evaluation error (as opposed to policy failure). The caller uses this
    // to set has_nix_eval_error in the commit metadata cache.

    // Re-evaluation safety: clear previous persisted logs for this commit so
    // (commit_id, log_sequence) uniqueness cannot collide on subsequent runs.
    crate::queries::eval_logs::delete_eval_logs_by_commit(pool, commit.id).await?;

    // Sequence counter for log persistence (1-indexed)
    let mut log_sequence = 1i32;

    let flake_ref = build_flake_reference(repo_url, commit_hash);
    let allowed_systems = load_allowed_systems(pool, flake, target_system).await?;

    // Load per-flake credentials (may be None for public flakes).
    let creds = FlakeCredentialEnv::load(pool, flake.id)
        .await
        .unwrap_or_else(|e| {
            warn!("Failed to load credentials for flake {}: {e:#}", flake.id);
            None
        });

    // Build ONE Nix expression that includes policy checks
    let nix_expr = build_nix_eval_expression(&flake_ref, policies);

    info!(
        "🚀 Running: nix-eval-jobs for {} with {} policies",
        target_system,
        policies.len()
    );

    // Broadcast detailed start information
    if let Some(state) = cf_state {
        let start_msg = format!(
            "🚀 Starting evaluation for flake: {}\n   Commit: {}\n   Target: {}\n   Workers: {}",
            flake.name,
            &commit_hash[..8.min(commit_hash.len())],
            target_system,
            server_config.eval_workers
        );
        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, start_msg)
            .await;

        if !policies.is_empty() {
            let policy_msg = format!("📋 Checking {} deployment policies:", policies.len());
            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                policy_msg,
            )
            .await;
            for policy in policies {
                let policy_detail = format!(
                    "   • {} (strict: {})",
                    policy.description(),
                    policy.is_strict()
                );
                broadcast_and_persist_eval_log(
                    pool,
                    Some(state),
                    commit.id,
                    &mut log_sequence,
                    policy_detail,
                )
                .await;
            }
        }

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "⏳ Evaluating nixosConfigurations...".to_string(),
        )
        .await;
    }

    if !policies.is_empty() {
        info!("   Policies will be evaluated in parallel by nix-eval-jobs:");
        for policy in policies {
            info!(
                "     - {} (strict={})",
                policy.description(),
                policy.is_strict()
            );
        }
    }

    debug!("📝 Nix expression:\n{}", nix_expr);

    // ── Establish expected system set BEFORE spawning the evaluator ───
    // This must happen now so that discovery failures fail the evaluation
    // before any child process starts, preventing orphan evaluators.
    // Load known system names from the artifact cache so we can detect systems
    // that nix-eval-jobs silently drops (no JSON line at all, not even an error).
    let known_cache = crate::queries::commits_artifacts::get_commit_nixos_configurations_from_cache(
        pool,
        commit.id,
    )
    .await?;

    let known_systems: Vec<String> = match known_cache {
        CachedSystemsState::Ready(systems) => systems,
        CachedSystemsState::Missing | CachedSystemsState::HydrationFailed => {
            // No cache row exists, or last hydration failed — hydrate inline now.
            // Use the credential-aware variant so private flake discovery works.
            let systems = crate::flake::commits::load_commit_nixos_configurations_with_creds(
                repo_url,
                commit_hash,
                creds.as_ref(),
            )
            .await?;

            // Persist discovered systems (including legitimately empty set)
            // without overwriting changed_files. This also marks
            // nixos_configurations_populated = true.
            if let Err(e) =
                crate::queries::commits_artifacts::upsert_commit_artifact_systems(
                    pool,
                    commit.id,
                    &systems,
                )
                .await
            {
                warn!("Failed to persist inline artifact cache: {}", e);
            }
            systems
        }
    };
    let has_known_systems = !known_systems.is_empty();
    let mut seen_systems: HashSet<String> = HashSet::new();

    // Run nix-eval-jobs with --meta flag to get policy results.
    // --impure is required because the Nix expression uses builtins.getFlake with a
    // remote git+ssh ref (e.g. git+git@github.com:...?rev=<hash>), which is only
    // permitted in impure evaluation mode.
    let mut cmd = Command::new("nix-eval-jobs");
    // Kill the child process if this future is dropped (e.g. cancellation,
    // discovery failure would no longer trigger this, but it is still a
    // valuable safety net for any other early-return path).
    cmd.kill_on_drop(true);
    cmd.args([
        "--expr",
        &nix_expr,
        "--impure", // Required: builtins.getFlake with remote git refs needs impure mode
        "--meta",   // CRITICAL: Include meta so we get policies in output!
        "--workers",
        &server_config.eval_workers.to_string(),
        "--max-memory-size",
        &server_config.eval_max_memory_mb.to_string(),
    ]);

    if server_config.eval_check_cache {
        cmd.arg("--check-cache-status");
    }
    build_config.apply_to_command(&mut cmd);

    // Inject per-flake credentials so Nix can access private repos.
    if let Some(c) = &creds {
        c.apply_to_nix_command(&mut cmd);
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let mut results = Vec::new();
    let mut policy_checks = Vec::new();
    let mut found_target = false;
    let mut stderr_output = Vec::new();
    let mut stderr_log_batch: Vec<(i32, Option<String>, String)> = Vec::new();
    const STDERR_LOG_BATCH_SIZE: usize = 100;
    let mut stdout_done = false;
    let mut stderr_done = false;

    // Track successfully evaluated derivations with their .drv paths
    let mut evaluated_derivations: Vec<(i32, String)> = Vec::new();

    // Cancellation poll interval: check the DB every 2 seconds while eval runs.
    let mut cancel_ticker = tokio::time::interval(Duration::from_secs(2));
    cancel_ticker.tick().await; // consume the immediate first tick
    let mut progress_ticker =
        tokio::time::interval(Duration::from_secs(EVAL_PROGRESS_HEARTBEAT_SECS));
    progress_ticker.tick().await; // consume immediate first tick
    let mut last_output_at = Instant::now();

    loop {
        tokio::select! {
            // Third arm: cooperative cancellation poll
            _ = cancel_ticker.tick() => {
                match crate::queries::commits::check_cancellation_requested(pool, commit.id).await {
                    Ok(true) => {
                        warn!("🚫 Cancellation requested for commit {} — killing nix-eval-jobs", commit.id);
                        let _ = child.kill().await;
                        // Transition cancelling → cancelled
                        let _ = crate::queries::commits::force_cancel_commit_evaluation(pool, commit.id).await;
                        // Run partial derivation cleanup inline (same as startup cleanup)
                        if let Err(e) = crate::queries::commits::cleanup_partial_derivations_for_commit(pool, commit.id).await {
                            warn!("Failed to clean up partial derivations for cancelled commit {}: {e}", commit.id);
                        }
                        bail!("evaluation cancelled by user request");
                    }
                    Ok(false) => {} // not cancelled, continue
                    Err(e) => {
                        warn!("Failed to check cancellation flag for commit {}: {e}", commit.id);
                    }
                }
            }
            _ = progress_ticker.tick() => {
                let idle_for = Instant::now().duration_since(last_output_at);
                if idle_for >= Duration::from_secs(EVAL_OUTPUT_IDLE_TIMEOUT_SECS) {
                    let timeout_msg = format!(
                        "❌ Evaluation produced no output for {}s; terminating nix-eval-jobs",
                        EVAL_OUTPUT_IDLE_TIMEOUT_SECS
                    );
                    error!("{} (commit_id={})", timeout_msg, commit.id);

                    if let Some(state) = cf_state {
                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, timeout_msg)
                            .await;
                    }

                    let _ = child.kill().await;
                    bail!(
                        "evaluation timed out after {}s without nix-eval-jobs output",
                        EVAL_OUTPUT_IDLE_TIMEOUT_SECS
                    );
                }

                debug!(
                    "nix-eval-jobs still running for commit {} (idle {}s)",
                    commit.id,
                    idle_for.as_secs()
                );
            }
            line_result = stdout_reader.next_line(), if !stdout_done => {
                match line_result? {
                    Some(line) if !line.trim().is_empty() => {
                        last_output_at = Instant::now();
                        match serde_json::from_str::<NixEvalJobResult>(&line) {
                            Ok(result) => {
                                let system_name = result.attr.clone();
                                if should_skip_system(&allowed_systems, &system_name) {
                                    debug!(
                                        "Skipping evaluated system {} due to flake build_scope={} and target_system={}",
                                        system_name,
                                        flake.build_scope,
                                        target_system
                                    );
                                    continue;
                                }
                                let has_error = result.error.is_some();
                                let drv_path = result.drv_path.clone();
                                let expected_store_path = if !has_error {
                                    if let Some(drv) = drv_path.as_deref() {
                                        resolve_expected_store_path(drv, result.outputs.as_ref()).await
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                debug!("📦 Evaluated: {}, drv_path={:?}, has_error={:?}",
                                    system_name, drv_path, has_error);

                                // Broadcast system evaluation result to logs
                                if let Some(state) = cf_state {
                                    if has_error {
                                        let error_msg = result.error.as_ref()
                                            .map(|e| {
                                                // Truncate long errors for readability
                                                if e.len() > 200 {
                                                    format!("{}...", &e[..200])
                                                } else {
                                                    e.clone()
                                                }
                                            })
                                            .unwrap_or_else(|| "Unknown error".to_string());
                                        let log_msg = format!("❌ {}: {}", system_name, error_msg);
                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                    } else {
                                        let log_msg = format!("✅ {} evaluated successfully", system_name);
                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                    }

                                    // Broadcast in-progress status to WebSocket clients.
                                    crate::handlers::api::commits::broadcast_system_status(
                                        state,
                                        commit.id,
                                        system_name.clone(),
                                        crate::handlers::api::commits::SystemEvalStatus::Evaluating,
                                        None,
                                    )
                                    .await;
                                }

                                // Extract policy check results from meta.policies
                                let mut cf_agent_enabled = None;
                                if let Some(meta) = &result.meta {
                                    if let Some(policies_json) = meta.get("policies") {
                                        // Parse policy results from meta.policies
                                        let check = PolicyCheckResult::from_json(
                                            system_name.clone(),
                                            policies_json,
                                            policies,
                                        );

                                        cf_agent_enabled = check.cf_agent_enabled;

                                        // Log policy results
                                        if !check.meets_requirements {
                                            let has_strict = policies.iter().any(|p| p.is_strict());
                                            for warning in &check.warnings {
                                                if has_strict {
                                                    error!("❌ {}", warning);
                                                } else {
                                                    warn!("⚠️  {}", warning);
                                                }

                                                // Broadcast policy warnings to logs
                                                if let Some(state) = cf_state {
                                                    let log_msg = format!("⚠️  {}: {}", system_name, warning);
                                                    broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                                }
                                            }
                                        } else if let Some(true) = cf_agent_enabled {
                                            info!("✅ {} has CF agent enabled", system_name);

                                            // Broadcast policy success to logs
                                            if let Some(state) = cf_state {
                                                let log_msg = format!("✅ {}: Crystal Forge agent enabled", system_name);
                                                broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                            }
                                        }

                                        policy_checks.push(check);
                                    } else {
                                        debug!("⚠️  No policies in meta for {}", system_name);
                                    }
                                } else {
                                    debug!("⚠️  No meta field for {}", system_name);
                                }

                                // Broadcast post-policy status to WebSocket clients.
                                if let Some(state) = cf_state {
                                    if has_error {
                                        let error_msg = result
                                            .error
                                            .clone()
                                            .unwrap_or_else(|| "Unknown error".to_string());
                                        crate::handlers::api::commits::broadcast_system_status(
                                            state,
                                            commit.id,
                                            system_name.clone(),
                                            crate::handlers::api::commits::SystemEvalStatus::Failed,
                                            Some(error_msg.clone()),
                                        )
                                        .await;
                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence,
                                            format!("❌ {}: {}", system_name, error_msg),
                                        )
                                        .await;
                                    } else if cf_agent_enabled == Some(true) {
                                        crate::handlers::api::commits::broadcast_system_status(
                                            state,
                                            commit.id,
                                            system_name.clone(),
                                            crate::handlers::api::commits::SystemEvalStatus::QueuedForBuild,
                                            None,
                                        )
                                        .await;
                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence,
                                            format!(
                                                "✅ {}: policy passed (CF enabled), queued for build",
                                                system_name
                                            ),
                                        )
                                        .await;
                                    } else {
                                        crate::handlers::api::commits::broadcast_system_status(
                                            state,
                                            commit.id,
                                            system_name.clone(),
                                            crate::handlers::api::commits::SystemEvalStatus::PolicyFailed,
                                            Some("CF agent not enabled in configuration".to_string()),
                                        )
                                        .await;
                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence,
                                            format!(
                                                "⚠️ {}: policy failed (CF agent not enabled)",
                                                system_name
                                            ),
                                        )
                                        .await;
                                    }
                                }

                                // Insert derivation with policy check results
                                if let Some(system_name) = result.attr_path.last() {
                                    let derivation_target = build_agent_target(
                                        &flake.repo_url,
                                        &commit.git_commit_hash,
                                        system_name,
                                    );

                                    match insert_derivation_with_target(
                                        pool,
                                        Some(commit),
                                        system_name,
                                        "nixos",
                                        Some(&derivation_target),
                                        cf_agent_enabled,
                                    ).await {
                                        Ok(deriv) => {
                                            debug!("✅ Inserted/updated {} (id={}, CF agent: {:?})",
                                                system_name, deriv.id, cf_agent_enabled);

                                            // Mark DryRunComplete and (if policy passed) enqueue.
                                            // Conditions to mark DryRunComplete:
                                            //   1. No evaluation error
                                            //   2. Has a valid .drv path
                                            // Additional condition to enqueue for build:
                                            //   3. cf_agent_enabled == Some(true) (policy passed)
                                            //      Policy-failed derivations are marked DryRunComplete
                                            //      so their eval result is recorded, but must NOT be
                                            //      queued for building.
                                            if !has_error && drv_path.is_some() {
                                                let drv = drv_path.clone().unwrap();
                                                evaluated_derivations.push((deriv.id, drv.clone()));
                                                debug!("📋 Queued {} for DryRunComplete update", system_name);

                                                match mark_derivation_dry_run_complete(pool, deriv.id, &drv).await {
                                                    Ok(_) => {
                                                        match crate::builder::create_drv_gc_root(&drv, deriv.id).await {
                                                            Ok(true) => debug!(
                                                                "📌 Rooted evaluated drv for {} (id={}, drv={})",
                                                                system_name, deriv.id, drv
                                                            ),
                                                            Ok(false) => warn!(
                                                                "⚠️  Evaluated drv for {} (id={}, drv={}) is not valid in the server store; remote builders may not be able to import it",
                                                                system_name, deriv.id, drv
                                                            ),
                                                            Err(e) => warn!(
                                                                "⚠️  Failed to create GC root for evaluated drv {} (id={}): {}",
                                                                drv, deriv.id, e
                                                            ),
                                                        }

                                                        if let Some(expected_path) = expected_store_path.as_deref() {
                                                            if let Err(e) = set_expected_store_path(pool, deriv.id, expected_path).await {
                                                                warn!(
                                                                    "⚠️  Failed to persist expected_store_path for {} (id={}): {}",
                                                                    system_name, deriv.id, e
                                                                );
                                                            }
                                                        } else {
                                                            warn!(
                                                                "⚠️  Could not resolve expected_store_path for {} (id={}) drv={}",
                                                                system_name, deriv.id, drv
                                                            );
                                                        }

                                                        // Count closure package totals asynchronously so the
                                                        // dependency graph can show build weight per system.
                                                        {
                                                            let pool2 = pool.clone();
                                                            let drv2 = drv.clone();
                                                            let deriv_id = deriv.id;
                                                            let sname = system_name.clone();
                                                            let limiter = closure_count_limiter();
                                                            info!(
                                                                "📦 Scheduling closure package count for {} (id={}, drv={})",
                                                                sname, deriv_id, drv2
                                                            );
                                                            tokio::spawn(async move {
                                                                let permit = match limiter.acquire_owned().await {
                                                                    Ok(permit) => permit,
                                                                    Err(e) => {
                                                                        warn!(
                                                                            "⚠️  Failed to acquire closure count permit for {} (id={}): {}",
                                                                            sname, deriv_id, e
                                                                        );
                                                                        return;
                                                                    }
                                                                };
                                                                info!(
                                                                    "📦 Starting closure package count for {} (id={}, drv={})",
                                                                    sname, deriv_id, drv2
                                                                );
                                                                match count_closure_packages(&drv2).await {
                                                                    Ok((total, cached)) => {
                                                                        if let Err(e) = set_closure_counts(&pool2, deriv_id, total, cached).await {
                                                                            warn!("⚠️  Failed to store closure counts for {} (id={}): {}", sname, deriv_id, e);
                                                                        } else {
                                                                            info!("📦 {} closure: {}/{} packages cached/local", sname, cached, total);
                                                                        }
                                                                    }
                                                                    Err(e) => warn!(
                                                                        "⚠️  Failed to count closure packages for {} (id={}): {}",
                                                                        sname, deriv_id, e
                                                                    ),
                                                                }
                                                                drop(permit);
                                                            });
                                                        }

                                                        // ── AUTOMATIC HARDENING SCAN ─────────────────────
                                                        // Trigger hardening scan automatically after successful eval
                                                        // Note: trigger_immediate_hardening_scan spawns its own background task
                                                        match trigger_immediate_hardening_scan(
                                                            pool.clone(),
                                                            deriv.id,
                                                            &flake_ref,
                                                            &system_name,
                                                        )
                                                        .await
                                                        {
                                                            Ok(scan_id) => {
                                                                debug!(
                                                                    "🔒 Automatically triggered hardening scan {} for {}",
                                                                    scan_id, system_name
                                                                );
                                                                if let Some(state) = cf_state {
                                                                    broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence,
                                                                        format!("🔒 {}: hardening scan queued", system_name),
                                                                    )
                                                                    .await;
                                                                }
                                                            }
                                                            Err(e) => {
                                                                warn!(
                                                                    "⚠️  Failed to trigger automatic hardening scan for {}: {}",
                                                                    system_name, e
                                                                );
                                                            }
                                                        }
                                                        // ─────────────────────────────────────────────────

                                                        // ── INCREMENTAL BUILD QUEUE ──────────────────────
                                                        // Only enqueue if policy passed.
                                                        if cf_agent_enabled == Some(true) {
                                                            match enqueue_build_job_for_derivation(pool, deriv.id).await {
                                                                Ok(true) => {
                                                                    info!(
                                                                        "🚀 Incrementally queued build job for {} (derivation {})",
                                                                        system_name, deriv.id
                                                                    );
                                                                    if let Some(state) = cf_state {
                                                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence,
                                                                            format!("🚀 {}: build job queued incrementally", system_name),
                                                                        ).await;
                                                                    }
                                                                    if let Some(qn) = queue_notifier {
                                                                        qn.notify_build_queue();
                                                                    }
                                                                }
                                                                Ok(false) => {
                                                                    debug!(
                                                                        "Build job for derivation {} already existed (idempotent); skipping",
                                                                        deriv.id
                                                                    );
                                                                }
                                                                Err(e) => {
                                                                    warn!(
                                                                        "⚠️  Failed to incrementally enqueue build job for {}: {}",
                                                                        system_name, e
                                                                    );
                                                                }
                                                            }
                                                        } else {
                                                            debug!(
                                                                "Skipping build job for {} (policy failed; cf_agent_enabled={:?})",
                                                                system_name, cf_agent_enabled
                                                            );
                                                        }
                                                        // ─────────────────────────────────────────────────
                                                    }
                                                    Err(e) => {
                                                        warn!(
                                                            "⚠️  Failed to mark derivation {} as DryRunComplete (incremental): {}",
                                                            deriv.id, e
                                                        );
                                                    }
                                                }
                                            } else {
                                                if has_error {
                                                    warn!("⚠️  {} has evaluation error, not marking complete", system_name);
                                                }
                                                if drv_path.is_none() {
                                                    warn!("⚠️  {} missing drv_path, not marking complete", system_name);
                                                }
                                            }
                                        }
                                        Err(e) => warn!("⚠️  Failed to insert {}: {}", system_name, e),
                                    }
                                }

                                if result.attr_path.last() == Some(&target_system.to_string()) || target_system == "all" {
                                    found_target = true;
                                    if target_system != "all" {
                                        info!("✅ Found target system: {}", target_system);
                                    }
                                }

                                if let Some(error) = &result.error {
                                    warn!("⚠️  Evaluation error for {}: {}", result.attr, error);
                                }

                                if has_known_systems {
                                    seen_systems.insert(system_name.clone());
                                }
                                results.push(result);
                            }
                            Err(e) => {
                                warn!("Failed to parse nix-eval-jobs output: {}\nLine: {}", e, line);
                            }
                        }
                    }
                    Some(_) => {},
                    None => stdout_done = true,
                }
            }
            line_result = stderr_reader.next_line(), if !stderr_done => {
                match line_result? {
                    Some(line) => {
                        last_output_at = Instant::now();
                        if line.contains("error:") {
                            error!("nix-eval-jobs stderr: {}", line);
                        } else {
                            debug!("nix-eval-jobs stderr: {}", line);
                        }

                        // Broadcast stderr to WebSocket clients
                        if let Some(state) = cf_state {
                            // Keep live streaming immediate.
                            crate::handlers::api::commits::broadcast_eval_log(state, commit.id, line.clone()).await;

                            // Persist stderr logs in batches to avoid per-line DB latency on hot path.
                            let level = if line.to_ascii_lowercase().contains("error") {
                                Some("error".to_string())
                            } else if line.to_ascii_lowercase().contains("warn") {
                                Some("warn".to_string())
                            } else {
                                Some("debug".to_string())
                            };
                            stderr_log_batch.push((log_sequence, level, line.clone()));
                            log_sequence += 1;

                            if stderr_log_batch.len() >= STDERR_LOG_BATCH_SIZE {
                                if let Err(e) = crate::queries::eval_logs::insert_eval_logs_batch(
                                    pool,
                                    commit.id,
                                    &stderr_log_batch,
                                )
                                .await
                                {
                                    warn!(
                                        "Failed to batch-persist stderr logs for commit {}: {}",
                                        commit.id, e
                                    );
                                }
                                stderr_log_batch.clear();
                            }
                        }

                        stderr_output.push(line);
                    }
                    None => stderr_done = true,
                }
            }
        }

        if stdout_done && stderr_done {
            break;
        }
    }

    // Flush any remaining buffered stderr logs.
    if !stderr_log_batch.is_empty() {
        if let Err(e) =
            crate::queries::eval_logs::insert_eval_logs_batch(pool, commit.id, &stderr_log_batch)
                .await
        {
            warn!(
                "Failed to flush batched stderr logs for commit {}: {}",
                commit.id, e
            );
        }
    }

    // ── Capture child exit status after consuming both streams ─────────
    // Important: do NOT bail before synthesis — we must account for every
    // expected system even when nix-eval-jobs crashed partway through.
    let child_status = child.wait().await?;

    // ── Detect systems that nix-eval-jobs silently dropped ─────────────────
    // When a system fails evaluation catastrophically, nix-eval-jobs may not
    // produce any JSON line for it at all (not even one with an error field).
    // Such systems silently disappear from the results. By comparing against
    // the known systems from the artifact cache, we detect these dropouts and
    // synthesize a failed result for each.
    //
    // This runs regardless of the child exit status so that a partial
    // evaluator crash still creates persisted failure records for all
    // expected-but-unseen systems.
    let expected_systems: Vec<String> = if has_known_systems {
        // Only consider known systems that are NOT intentionally skipped by
        // the build_scope filter. Systems excluded by cf_systems_only should
        // never be reported as failures.
        known_systems
            .iter()
            .filter(|s| !should_skip_system(&allowed_systems, s))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    // Collect missing systems that need fallback evaluation.
    let missing_systems: Vec<&str> = expected_systems
        .iter()
        .filter(|s| !seen_systems.contains(s.as_str()))
        .map(|s| s.as_str())
        .collect();

    let mut had_unexpected_success = false;
    let mut had_infra_failure = false;

    if missing_systems.len() > MAX_INDIVIDUAL_FALLBACKS {
        bail!(
            "nix-eval-jobs silently dropped {} systems (max {}); likely process-wide failure",
            missing_systems.len(),
            MAX_INDIVIDUAL_FALLBACKS,
        );
    }

    // ── Fallback phase: evaluate missing systems with timeout and cancellation ──
    // This runs sequentially (bounded by MAX_INDIVIDUAL_FALLBACKS). Concurrency
    // is a separate P2 optimization.
    if !missing_systems.is_empty() {
        warn!(
            "⚠️  {} systems were expected but never appeared in nix-eval-jobs output. Running fallback evaluations...",
            missing_systems.len()
        );

        let fallback_deadline = Instant::now() + FALLBACK_PHASE_TIMEOUT;
        let mut cancel_ticker = tokio::time::interval(Duration::from_secs(2));

        for system_name in &missing_systems {
            // Check cancellation at the start of each fallback.
            cancel_ticker.tick().await;
            if Instant::now() >= fallback_deadline {
                bail!(
                    "Fallback evaluation phase timed out after {}s",
                    FALLBACK_PHASE_TIMEOUT.as_secs()
                );
            }
            match crate::queries::commits::check_cancellation_requested(pool, commit.id).await {
                Ok(true) => {
                    warn!("🚫 Cancellation requested for commit {} during fallback phase", commit.id);
                    bail!("evaluation cancelled by user request during fallback");
                }
                Err(e) => warn!("Failed to check cancellation flag during fallback: {}", e),
                Ok(false) => {}
            }

            let outcome = fallback_eval_single_system(
                repo_url,
                commit_hash,
                system_name,
                creds.as_ref(),
            )
            .await;

            match outcome {
                FallbackEvalOutcome::ConfirmedSystemFailure { error } => {
                    warn!(
                        "⚠️  System {} was expected but never appeared in nix-eval-jobs output. Recording as failed.",
                        system_name
                    );

                    // Broadcast error log and structured status.
                    let system_owned = system_name.to_string();
                    if let Some(state) = cf_state {
                        let log_msg = format!("❌ {}: {}", system_owned, error);
                        broadcast_and_persist_eval_log(
                            pool, Some(state), commit.id, &mut log_sequence, log_msg,
                        ).await;

                        crate::handlers::api::commits::broadcast_system_status(
                            state, commit.id, system_owned.clone(),
                            crate::handlers::api::commits::SystemEvalStatus::Failed,
                            Some(error.clone()),
                        ).await;
                    }

                    // Persist a derivation record with DryRunFailed status
                    // using the atomic record_synthetic_eval_failure.
                    let derivation_target = build_agent_target(repo_url, commit_hash, &system_owned);
                    if let Err(e) = record_synthetic_eval_failure(
                        pool,
                        Some(commit.id),
                        &system_owned,
                        "nixos",
                        Some(&derivation_target),
                        &error,
                    ).await {
                        warn!("Failed to record synthetic eval failure for {}: {}", system_owned, e);
                    }

                    // Add synthetic NixEvalJobResult.
                    results.push(NixEvalJobResult {
                        attr: system_owned.clone(),
                        attr_path: vec![system_owned.clone()],
                        name: Some(system_owned.clone()),
                        drv_path: None,
                        error: Some(error),
                        cache_status: None,
                        outputs: None,
                        meta: None,
                    });

                    // Add synthetic policy check result (eval error, not policy failure).
                    policy_checks.push(PolicyCheckResult {
                        system_name: system_owned.clone(),
                        cf_agent_enabled: None,
                        has_required_packages: None,
                        custom_checks: HashMap::new(),
                        meets_requirements: false,
                        warnings: vec!["System failed to evaluate (no output from nix-eval-jobs)".to_string()],
                        failed_policies: vec![],
                        cve_checks: vec![],
                    });
                }
                FallbackEvalOutcome::StandaloneEvaluationSucceeded => {
                    warn!(
                        "⚠️  System {} was expected but never appeared, yet standalone eval succeeded. \
                         This is an evaluator omission, not a system failure.",
                        system_name
                    );
                    had_unexpected_success = true;
                }
                FallbackEvalOutcome::InfrastructureFailure { error } => {
                    warn!(
                        "⚠️  Fallback eval for {} failed with infrastructure error: {}",
                        system_name, error
                    );
                    had_infra_failure = true;
                }
            }
        }
    }

    // If a specific target was requested, validate after synthesis so that
    // synthetic failed results count as "accounted for".
    if !found_target && target_system != "all" {
        let target_was_expected = expected_systems.iter().any(|s| s == target_system);
        let target_was_accounted_for = results.iter().any(|r| r.attr == target_system);
        if !target_was_expected || !target_was_accounted_for {
            bail!(
                "nix-eval-jobs did not evaluate target system: {}\nEvaluated systems: {:?}",
                target_system,
                results.iter().map(|r| r.attr.as_str()).collect::<Vec<_>>()
            );
        }
    }

    // ── Evaluate fallback-phase outcome ──────────────────────────────────
    // If any fallback either succeeded (standalone) or had infrastructure
    // failure, the evaluation as a whole should fail and be retried — we
    // cannot claim to have a complete picture.
    if had_unexpected_success || had_infra_failure {
        if had_unexpected_success && had_infra_failure {
            bail!(
                "Fallback evaluations had {} unexpected successes and {} infrastructure failures; \
                 evaluation should be retried",
                missing_systems.len(), // approximate
                0  // placeholder — we tracked bools not counts
            );
        } else if had_unexpected_success {
            bail!(
                "One or more expected systems succeeded in standalone eval but were dropped by nix-eval-jobs; \
                 evaluation should be retried"
            );
        } else {
            bail!(
                "One or more fallback evaluations failed due to infrastructure issues; \
                 evaluation should be retried"
            );
        }
    }

    // ── Log systems that failed evaluation vs policy failures ──────────
    // Split into four groups: passed, evaluation_errors, strict_policy_failures,
    // non_strict_policy_failures. Evaluation errors are checks with
    // meets_requirements=false and failed_policies.is_empty().
    let mut passed_systems = Vec::new();
    let mut evaluation_errors = Vec::new();
    let mut strict_policy_failures = Vec::new();
    let mut non_strict_policy_failures = Vec::new();

    for check in &policy_checks {
        if check.meets_requirements {
            passed_systems.push(check);
        } else if check.failed_policies.is_empty() {
            evaluation_errors.push(check);
        } else if check
            .failed_policies
            .iter()
            .any(|(_, strict)| *strict)
        {
            strict_policy_failures.push(check);
        } else {
            non_strict_policy_failures.push(check);
        }
    }

    // Log systems that failed evaluation
    if !evaluation_errors.is_empty() {
        error!(
            "❌ {} systems failed evaluation:",
            evaluation_errors.len()
        );
        for failure in &evaluation_errors {
            error!("  - {}", failure.system_name);
            for warning in &failure.warnings {
                error!("    {}", warning);
            }
        }
    }

    // Log systems that failed strict policies
    if !strict_policy_failures.is_empty() {
        error!(
            "⚠️  {} systems failed strict deployment policies (will not be queued for build):",
            strict_policy_failures.len()
        );
        for failure in &strict_policy_failures {
            error!("  - {}", failure.system_name);
            for (policy_desc, is_strict) in &failure.failed_policies {
                if *is_strict {
                    error!("    • [STRICT] {}", policy_desc);
                }
            }
        }
    }

    // Log systems that failed only non-strict policies
    if !non_strict_policy_failures.is_empty() {
        warn!(
            "⚠️  {} systems failed non-strict deployment policies:",
            non_strict_policy_failures.len()
        );
        for failure in &non_strict_policy_failures {
            warn!("  - {}", failure.system_name);
            for (policy_desc, _) in &failure.failed_policies {
                warn!("    • {}", policy_desc);
            }
        }
    }

    // Log systems that passed all policies
    if !passed_systems.is_empty() {
        info!(
            "✅ {} systems passed all deployment policies",
            passed_systems.len()
        );
    }

    // Log overall summary
    let total_policy_checks = policy_checks.len();
    if total_policy_checks > 0 {
        info!(
            "📊 Policy evaluation summary: {} passed, {} eval errors, {} strict failures, {} non-strict failures",
            passed_systems.len(),
            evaluation_errors.len(),
            strict_policy_failures.len(),
            non_strict_policy_failures.len(),
        );
    }

    // DryRunComplete marking and incremental build job enqueuing are now done
    // per-derivation inside the streaming loop above, so no post-loop batch is needed.
    // The `create_build_jobs_for_commit` call in server/mod.rs remains as an idempotent
    // backstop that handles any derivation missed by incremental enqueuing (e.g. due to
    // a transient error), and is safe to run because of the NOT EXISTS guard.
    if evaluated_derivations.is_empty() {
        warn!("⚠️  No derivations successfully evaluated (all had errors or missing paths)");
    } else {
        info!(
            "✅ {} derivations marked DryRunComplete and incrementally queued for building",
            evaluated_derivations.len()
        );
    }

    info!("✅ Evaluated {} configurations in parallel", results.len());

    let had_system_eval_errors = results.iter().any(|r| r.error.is_some());

    // Calculate statistics for summary
    // `results` includes both stdout results and synthesized entries for
    // systems that nix-eval-jobs silently dropped, so results.len() is the
    // authoritative total.
    let successful = results.iter().filter(|r| r.error.is_none()).count();
    let failed = results.iter().filter(|r| r.error.is_some()).count();
    let total_systems = successful + failed;

    let with_agent = policy_checks
        .iter()
        .filter(|c| c.cf_agent_enabled == Some(true))
        .count();
    let coverage = if policy_checks.len() > 0 {
        (with_agent as f64 / policy_checks.len() as f64) * 100.0
    } else {
        0.0
    };

    if !policies.is_empty() && !policy_checks.is_empty() {
        info!(
            "   CF agent: {}/{} systems enabled ({:.1}%)",
            with_agent,
            policy_checks.len(),
            coverage
        );
    }

    // Broadcast comprehensive summary to WebSocket clients
    if let Some(state) = cf_state {
        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "".to_string(), // Blank line for readability
        )
        .await;

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "═══════════════════════════════════════".to_string(),
        )
        .await;

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "📊 Evaluation Summary".to_string(),
        )
        .await;

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "═══════════════════════════════════════".to_string(),
        )
        .await;

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            format!("✅ Successful: {} systems", successful),
        )
        .await;

        if failed > 0 {
            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                format!("❌ Failed: {} systems", failed),
            )
            .await;
        }

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            format!("📦 Total: {} nixosConfigurations", total_systems),
        )
        .await;

        if !policy_checks.is_empty() {
            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                "".to_string(),
            )
            .await;

            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                format!(
                    "🔐 Policy Compliance: {:.1}% ({}/{})",
                    coverage,
                    with_agent,
                    policy_checks.len()
                ),
            )
            .await;
        }

        if evaluated_derivations.len() > 0 {
            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                "".to_string(),
            )
            .await;

            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                format!(
                    "🚀 {} derivations ready for build queue",
                    evaluated_derivations.len()
                ),
            )
            .await;
        }

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "═══════════════════════════════════════".to_string(),
        )
        .await;
    }

    // ── Propagate child-process error ────────────────────────────────────
    // A nonzero child exit means nix-eval-jobs itself crashed or encountered
    // an unrecoverable error. Even when all expected systems are accounted for
    // by our results+synthesis, a process crash could have corrupted stdout or
    // skipped records — we cannot confidently accept the partial output.
    // Return Err so the evaluation is retried; any synthetic DryRunFailed
    // records already persisted are safe (record_synthetic_eval_failure
    // protects terminal build states) and will be overwritten on retry.
    if !child_status.success() {
        let stderr_text = stderr_output.join("\n");
        bail!(
            "nix-eval-jobs failed with exit code: {}\nStderr:\n{}",
            child_status.code().unwrap_or(-1),
            stderr_text.chars().take(500).collect::<String>(),
        );
    }

    Ok((results, policy_checks, had_system_eval_errors))
}

/// Dev-only deterministic mock evaluation path.
///
/// This simulates nix-eval-jobs behavior while preserving queue/log/policy/derivation
/// transitions so UI and process workflows can be validated quickly.
#[allow(clippy::too_many_arguments)]
pub async fn evaluate_with_mock_eval_jobs(
    pool: &PgPool,
    commit: &Commit,
    flake: &Flake,
    repo_url: &str,
    commit_hash: &str,
    target_system: &str,
    _build_config: &BuildConfig,
    _server_config: &ServerConfig,
    _policies: &[DeploymentPolicy],
    configured_systems: &[String],
    cf_state: Option<&crate::handlers::agent_request::CFState>,
    queue_notifier: Option<&QueueNotifier>,
) -> Result<(Vec<NixEvalJobResult>, Vec<PolicyCheckResult>, bool)> {
    // Re-evaluation safety: clear previous persisted logs for this commit so
    // (commit_id, log_sequence) uniqueness cannot collide on subsequent runs.
    crate::queries::eval_logs::delete_eval_logs_by_commit(pool, commit.id).await?;

    // Sequence counter for persisted/mock streamed logs (1-indexed)
    let mut log_sequence = 1i32;

    let systems = resolve_mock_systems(&flake.name, target_system, configured_systems)?;
    let stage_delay = mock_eval_stage_delay(systems.len());

    crate::queries::commits_artifacts::upsert_commit_artifact_cache(pool, commit.id, &systems, &[])
        .await?;

    if let Some(state) = cf_state {
        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            format!(
                "🧪 MOCK MODE: evaluating {} system(s) for {}@{}",
                systems.len(),
                flake.name,
                &commit_hash[..8.min(commit_hash.len())]
            ),
        )
        .await;
    }

    let mut results = Vec::with_capacity(systems.len());
    let mut checks = Vec::with_capacity(systems.len());

    for (idx, system_name) in systems.iter().enumerate() {
        if let Some(state) = cf_state {
            crate::handlers::api::commits::broadcast_system_status(
                state,
                commit.id,
                system_name.clone(),
                crate::handlers::api::commits::SystemEvalStatus::Evaluating,
                None,
            )
            .await;
            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                format!(
                    "⏳ {}: queued in mock pipeline (system {}/{})",
                    system_name,
                    idx + 1,
                    systems.len()
                ),
            )
            .await;
        }

        for (progress, stage) in [
            (10, "resolving flake input graph"),
            (30, "checking nixosConfigurations outputs"),
            (55, "expanding module graph"),
            (80, "running policy prechecks"),
            (95, "finalizing derivation metadata"),
        ] {
            if let Some(state) = cf_state {
                broadcast_and_persist_eval_log(
                    pool,
                    Some(state),
                    commit.id,
                    &mut log_sequence,
                    format!("⏳ {} [{}%]: {}", system_name, progress, stage),
                )
                .await;
            }
            tokio::time::sleep(stage_delay).await;
        }

        let flake_ref = build_flake_reference(repo_url, commit_hash);
        let drv_path = format!(
            "/nix/store/mock-{}-{}.drv",
            &commit_hash[..8.min(commit_hash.len())],
            system_name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                })
                .collect::<String>()
        );
        let derivation_target = format!("{}#nixosConfigurations.{}", flake_ref, system_name);

        let policy_failed = should_mock_policy_fail(systems.len(), idx);

        let derivation = insert_derivation_with_target(
            pool,
            Some(commit),
            system_name,
            "nixos",
            Some(&derivation_target),
            Some(!policy_failed),
        )
        .await?;

        mark_derivation_dry_run_complete(pool, derivation.id, &drv_path).await?;

        // Incremental enqueue: queue build job immediately for passing mock systems.
        if !policy_failed {
            match enqueue_build_job_for_derivation(pool, derivation.id).await {
                Ok(true) => {
                    info!(
                        "🚀 [mock] Incrementally queued build job for {} (derivation {})",
                        system_name, derivation.id
                    );
                    if let Some(state) = cf_state {
                        broadcast_and_persist_eval_log(
                            pool,
                            Some(state),
                            commit.id,
                            &mut log_sequence,
                            format!("🚀 {}: build job queued incrementally (mock)", system_name),
                        )
                        .await;
                    }
                    if let Some(qn) = queue_notifier {
                        qn.notify_build_queue();
                    }
                }
                Ok(false) => {
                    debug!(
                        "[mock] Build job for derivation {} already existed; skipping",
                        derivation.id
                    );
                }
                Err(e) => {
                    warn!(
                        "⚠️  [mock] Failed to incrementally enqueue build job for {}: {}",
                        system_name, e
                    );
                }
            }
        }

        let check = PolicyCheckResult {
            system_name: system_name.clone(),
            cf_agent_enabled: Some(!policy_failed),
            has_required_packages: None,
            custom_checks: HashMap::new(),
            meets_requirements: !policy_failed,
            warnings: if policy_failed {
                vec!["Mock policy failure for UI validation".to_string()]
            } else {
                vec![]
            },
            failed_policies: if policy_failed {
                vec![("Crystal Forge agent must be enabled".to_string(), true)]
            } else {
                vec![]
            },
            cve_checks: vec![],
        };
        checks.push(check);

        results.push(NixEvalJobResult {
            attr: system_name.clone(),
            attr_path: vec!["nixosConfigurations".to_string(), system_name.clone()],
            name: Some(system_name.clone()),
            drv_path: Some(drv_path),
            error: None,
            cache_status: Some("unknown".to_string()),
            outputs: None,
            meta: None,
        });

        if let Some(state) = cf_state {
            if policy_failed {
                crate::handlers::api::commits::broadcast_system_status(
                    state,
                    commit.id,
                    system_name.clone(),
                    crate::handlers::api::commits::SystemEvalStatus::PolicyFailed,
                    Some("Mock policy failure".to_string()),
                )
                .await;
                broadcast_and_persist_eval_log(
                    pool,
                    Some(state),
                    commit.id,
                    &mut log_sequence,
                    format!(
                        "⚠️ {}: policy check failed (mock), build skipped",
                        system_name
                    ),
                )
                .await;
            } else {
                crate::handlers::api::commits::broadcast_system_status(
                    state,
                    commit.id,
                    system_name.clone(),
                    crate::handlers::api::commits::SystemEvalStatus::QueuedForBuild,
                    None,
                )
                .await;
                broadcast_and_persist_eval_log(
                    pool,
                    Some(state),
                    commit.id,
                    &mut log_sequence,
                    format!(
                        "✅ {}: policy passed (CF enabled), queued for build",
                        system_name
                    ),
                )
                .await;
            }
        }
    }

    // Mock evaluations never have system eval errors, so had_system_eval_errors is false.
    let had_system_eval_errors = false;
    Ok((results, checks, had_system_eval_errors))
}

fn build_agent_target(repo_url: &str, commit_hash: &str, system_name: &str) -> String {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    format!("{}#nixosConfigurations.{}", flake_ref, system_name)
}

#[cfg(test)]
pub async fn load_allowed_systems_for_test(
    pool: &PgPool,
    flake: &Flake,
    target_system: &str,
) -> Result<Option<Vec<String>>> {
    load_allowed_systems(pool, flake, target_system).await
}

#[cfg(test)]
pub fn should_skip_system_for_test(
    allowed_systems: &Option<Vec<String>>,
    system_name: &str,
) -> bool {
    should_skip_system(allowed_systems, system_name)
}

async fn load_allowed_systems(
    pool: &PgPool,
    flake: &Flake,
    target_system: &str,
) -> Result<Option<Vec<String>>> {
    if target_system != "all" {
        return Ok(None);
    }
    if flake.build_scope != "cf_systems_only" {
        return Ok(None);
    }

    let mut systems = list_configuration_names_for_flake(pool, flake.id).await?;
    systems.sort();
    systems.dedup();
    Ok(Some(systems))
}

fn should_skip_system(allowed_systems: &Option<Vec<String>>, system_name: &str) -> bool {
    match allowed_systems {
        Some(systems) => !systems.iter().any(|configured| configured == system_name),
        None => false,
    }
}

fn resolve_mock_systems(
    flake_name: &str,
    target_system: &str,
    configured_systems: &[String],
) -> Result<Vec<String>> {
    let mut systems = if configured_systems.is_empty() {
        vec![
            format!("{}-control-0", flake_name),
            format!("{}-worker-0", flake_name),
            format!("{}-worker-1", flake_name),
        ]
    } else {
        configured_systems.to_vec()
    };

    systems.sort();

    if target_system != "all" {
        systems.retain(|s| s == target_system);
    }

    if systems.is_empty() {
        bail!("mock evaluation has no matching systems to evaluate");
    }

    Ok(systems)
}

fn mock_eval_stage_delay(system_count: usize) -> std::time::Duration {
    let systems = std::cmp::max(system_count as u64, 1);
    let per_system = std::cmp::max(
        MOCK_EVAL_TOTAL_DURATION_MS / systems,
        MOCK_EVAL_MIN_PER_SYSTEM_MS,
    );
    let per_stage = std::cmp::max(per_system / MOCK_EVAL_STAGE_COUNT, 750);
    std::time::Duration::from_millis(per_stage)
}

fn should_mock_policy_fail(system_count: usize, idx: usize) -> bool {
    system_count > 1 && idx == 1
}

/// Update commit metadata cache with evaluation summary statistics
///
/// This function should be called after evaluation completes (success or failure)
/// to cache the results for fast flakes view loading.
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `commit_id` - ID of the evaluated commit
/// * `policy_checks` - Policy check results for all systems
/// * `has_nix_eval_error` - Whether the evaluation had a Nix error (vs policy failure)
pub async fn update_commit_metadata_cache(
    pool: &PgPool,
    commit_id: i32,
    policy_checks: &[PolicyCheckResult],
    has_nix_eval_error: bool,
) -> Result<()> {
    let (
        total_systems,
        systems_passed,
        systems_failed_strict,
        systems_failed_non_strict,
        systems_with_eval_error,
        has_policy_failures,
        all_systems_passed,
    ) = summarize_commit_metadata(policy_checks, has_nix_eval_error);

    sqlx::query!(
        r#"
        INSERT INTO commit_metadata_cache (
            commit_id,
            total_systems,
            systems_passed_policy,
            systems_failed_policy_strict,
            systems_failed_policy_non_strict,
            systems_with_eval_error,
            has_nix_eval_error,
            has_policy_failures,
            all_systems_passed
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (commit_id) DO UPDATE SET
            total_systems = EXCLUDED.total_systems,
            systems_passed_policy = EXCLUDED.systems_passed_policy,
            systems_failed_policy_strict = EXCLUDED.systems_failed_policy_strict,
            systems_failed_policy_non_strict = EXCLUDED.systems_failed_policy_non_strict,
            systems_with_eval_error = EXCLUDED.systems_with_eval_error,
            has_nix_eval_error = EXCLUDED.has_nix_eval_error,
            has_policy_failures = EXCLUDED.has_policy_failures,
            all_systems_passed = EXCLUDED.all_systems_passed,
            cached_at = CURRENT_TIMESTAMP
        "#,
        commit_id,
        total_systems,
        systems_passed,
        systems_failed_strict,
        systems_failed_non_strict,
        systems_with_eval_error,
        has_nix_eval_error,
        has_policy_failures,
        all_systems_passed
    )
    .execute(pool)
    .await?;

    debug!(
        "💾 Updated commit metadata cache for commit {}: {}/{} systems passed",
        commit_id, systems_passed, total_systems
    );

    Ok(())
}

fn summarize_commit_metadata(
    policy_checks: &[PolicyCheckResult],
    has_nix_eval_error: bool,
) -> (i32, i32, i32, i32, i32, bool, bool) {
    let total_systems = policy_checks.len() as i32;

    let systems_passed = policy_checks
        .iter()
        .filter(|c| c.meets_requirements)
        .count() as i32;

    let systems_failed_strict = policy_checks
        .iter()
        .filter(|c| {
            !c.meets_requirements && c.failed_policies.iter().any(|(_, is_strict)| *is_strict)
        })
        .count() as i32;

    // A failed check with no failed_policies indicates evaluation-level failure
    // for that system (not a policy failure). We keep this distinct from
    // non-strict policy failures to avoid misleading API counts.
    let systems_with_eval_error_from_checks = policy_checks
        .iter()
        .filter(|c| !c.meets_requirements && c.failed_policies.is_empty())
        .count() as i32;

    let systems_failed_non_strict = policy_checks
        .iter()
        .filter(|c| {
            !c.meets_requirements
                && !c.failed_policies.is_empty()
                && !c.failed_policies.iter().any(|(_, is_strict)| *is_strict)
        })
        .count() as i32;

    let systems_with_eval_error = systems_with_eval_error_from_checks;
    let has_policy_failures = systems_failed_strict > 0 || systems_failed_non_strict > 0;

    // If evaluation itself failed, this commit cannot be considered fully passing,
    // even when per-system checks are unavailable.
    let all_systems_passed =
        !has_nix_eval_error && total_systems > 0 && systems_passed == total_systems;

    (
        total_systems,
        systems_passed,
        systems_failed_strict,
        systems_failed_non_strict,
        systems_with_eval_error,
        has_policy_failures,
        all_systems_passed,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        mock_eval_stage_delay, resolve_mock_systems, should_mock_policy_fail,
        summarize_commit_metadata,
    };

    #[test]
    fn mock_systems_fallback_and_filtering() {
        let all =
            resolve_mock_systems("demo", "all", &[]).expect("fallback systems should resolve");
        assert_eq!(
            all,
            vec![
                "demo-control-0".to_string(),
                "demo-worker-0".to_string(),
                "demo-worker-1".to_string()
            ]
        );

        let filtered =
            resolve_mock_systems("demo", "demo-worker-1", &all).expect("target should filter");
        assert_eq!(filtered, vec!["demo-worker-1".to_string()]);
    }

    #[test]
    fn mock_systems_errors_when_target_missing() {
        let systems = vec!["alpha".to_string(), "beta".to_string()];
        let err = resolve_mock_systems("demo", "gamma", &systems)
            .expect_err("missing target should return error");
        assert!(
            err.to_string()
                .contains("mock evaluation has no matching systems to evaluate")
        );
    }

    #[test]
    fn mock_eval_stage_delay_targets_human_observable_runtime() {
        assert_eq!(mock_eval_stage_delay(3).as_millis(), 2000);
        assert_eq!(mock_eval_stage_delay(1).as_millis(), 6000);
        assert_eq!(mock_eval_stage_delay(10).as_millis(), 1000);
    }

    #[test]
    fn mock_policy_fail_pattern_is_deterministic() {
        assert!(!should_mock_policy_fail(1, 0));
        assert!(!should_mock_policy_fail(3, 0));
        assert!(should_mock_policy_fail(3, 1));
        assert!(!should_mock_policy_fail(3, 2));
    }

    #[test]
    fn test_policy_check_result_tracks_failed_policies_with_strictness() {
        use crate::models::deployment_policies::{DeploymentPolicy, PolicyCheckResult};
        use serde_json::json;

        let policies = vec![
            DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
            DeploymentPolicy::RequirePackages {
                packages: vec!["git".to_string()],
                strict: false,
            },
        ];

        // System failing strict policy only
        let policies_json = json!({
            "cfAgentEnabled": false,
            "hasRequiredPackages_1": true
        });

        let result =
            PolicyCheckResult::from_json("test-system".to_string(), &policies_json, &policies);

        assert!(!result.meets_requirements);
        assert_eq!(result.failed_policies.len(), 1);
        assert_eq!(
            result.failed_policies[0].0,
            "Crystal Forge agent must be enabled"
        );
        assert!(result.failed_policies[0].1); // is_strict = true

        // System failing non-strict policy only
        let policies_json_2 = json!({
            "cfAgentEnabled": true,
            "hasRequiredPackages_1": false
        });

        let result_2 =
            PolicyCheckResult::from_json("test-system-2".to_string(), &policies_json_2, &policies);

        // Non-strict failures should warn, but not block requirements.
        assert!(result_2.meets_requirements);
        assert_eq!(result_2.failed_policies.len(), 1);
        assert_eq!(result_2.failed_policies[0].0, "Required packages: git");
        assert!(!result_2.failed_policies[0].1); // is_strict = false

        // System failing both
        let policies_json_3 = json!({
            "cfAgentEnabled": false,
            "hasRequiredPackages_1": false
        });

        let result_3 =
            PolicyCheckResult::from_json("test-system-3".to_string(), &policies_json_3, &policies);

        assert!(!result_3.meets_requirements);
        assert_eq!(result_3.failed_policies.len(), 2);

        // Should have one strict and one non-strict failure
        let strict_count = result_3
            .failed_policies
            .iter()
            .filter(|(_, is_strict)| *is_strict)
            .count();
        let non_strict_count = result_3
            .failed_policies
            .iter()
            .filter(|(_, is_strict)| !*is_strict)
            .count();

        assert_eq!(strict_count, 1);
        assert_eq!(non_strict_count, 1);
    }

    #[test]
    fn metadata_summary_marks_eval_error_commit_as_not_all_passed() {
        let (
            total,
            passed,
            strict_failed,
            non_strict_failed,
            eval_failed,
            has_policy_failures,
            all_passed,
        ) = summarize_commit_metadata(&[], true);
        assert_eq!(total, 0);
        assert_eq!(passed, 0);
        assert_eq!(strict_failed, 0);
        assert_eq!(non_strict_failed, 0);
        assert_eq!(eval_failed, 0);
        assert!(!has_policy_failures);
        assert!(!all_passed);
    }

    #[test]
    fn metadata_summary_does_not_treat_empty_failed_policies_as_non_strict_failure() {
        use crate::models::deployment_policies::PolicyCheckResult;

        let checks = vec![PolicyCheckResult {
            system_name: "alpha".to_string(),
            cf_agent_enabled: None,
            has_required_packages: None,
            custom_checks: std::collections::HashMap::new(),
            meets_requirements: false,
            warnings: vec!["evaluation failed".to_string()],
            failed_policies: vec![],
            cve_checks: vec![],
        }];

        let (
            _total,
            _passed,
            strict_failed,
            non_strict_failed,
            eval_failed,
            has_policy_failures,
            all_passed,
        ) = summarize_commit_metadata(&checks, false);

        assert_eq!(strict_failed, 0);
        assert_eq!(non_strict_failed, 0);
        assert_eq!(eval_failed, 1);
        assert!(!has_policy_failures);
        assert!(!all_passed);
    }
}
