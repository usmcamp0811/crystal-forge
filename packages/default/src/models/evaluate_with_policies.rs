use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const MOCK_EVAL_TOTAL_DURATION_MS: u64 = 30_000;
const MOCK_EVAL_MIN_PER_SYSTEM_MS: u64 = 5_000;
const MOCK_EVAL_STAGE_COUNT: u64 = 5;
use tracing::{debug, error, info, warn};

use crate::config::{BuildConfig, ServerConfig};
use crate::flake::credentials::FlakeCredentialEnv;
use crate::models::commits::Commit;
use crate::models::deployment_policies::{
    DeploymentPolicy, PolicyCheckResult, build_nix_eval_expression,
};
use crate::models::flakes::Flake;
use crate::queries::build_jobs::enqueue_build_job_for_derivation;
use crate::queries::derivations::{
    insert_derivation_with_target, mark_derivation_dry_run_complete, set_expected_store_path,
};
use crate::queries::systems::list_configuration_names_for_flake;
use crate::queue::QueueNotifier;

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
) -> Result<(Vec<NixEvalJobResult>, Vec<PolicyCheckResult>)> {
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
        crate::handlers::api::commits::broadcast_eval_log(state, commit.id, start_msg).await;

        if !policies.is_empty() {
            let policy_msg = format!("📋 Checking {} deployment policies:", policies.len());
            crate::handlers::api::commits::broadcast_eval_log(state, commit.id, policy_msg).await;
            for policy in policies {
                let policy_detail = format!(
                    "   • {} (strict: {})",
                    policy.description(),
                    policy.is_strict()
                );
                crate::handlers::api::commits::broadcast_eval_log(state, commit.id, policy_detail)
                    .await;
            }
        }

        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
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

    // Run nix-eval-jobs with --meta flag to get policy results
    let mut cmd = Command::new("nix-eval-jobs");
    cmd.args([
        "--expr",
        &nix_expr,
        "--meta", // CRITICAL: Include meta so we get policies in output!
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
    let mut stdout_done = false;
    let mut stderr_done = false;

    // Track successfully evaluated derivations with their .drv paths
    let mut evaluated_derivations: Vec<(i32, String)> = Vec::new();

    loop {
        tokio::select! {
            line_result = stdout_reader.next_line(), if !stdout_done => {
                match line_result? {
                    Some(line) if !line.trim().is_empty() => {
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
                                        crate::handlers::api::commits::broadcast_eval_log(state, commit.id, log_msg).await;
                                    } else {
                                        let log_msg = format!("✅ {} evaluated successfully", system_name);
                                        crate::handlers::api::commits::broadcast_eval_log(state, commit.id, log_msg).await;
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
                                                    crate::handlers::api::commits::broadcast_eval_log(state, commit.id, log_msg).await;
                                                }
                                            }
                                        } else if let Some(true) = cf_agent_enabled {
                                            info!("✅ {} has CF agent enabled", system_name);

                                            // Broadcast policy success to logs
                                            if let Some(state) = cf_state {
                                                let log_msg = format!("✅ {}: Crystal Forge agent enabled", system_name);
                                                crate::handlers::api::commits::broadcast_eval_log(state, commit.id, log_msg).await;
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
                                        crate::handlers::api::commits::broadcast_eval_log(
                                            state,
                                            commit.id,
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
                                        crate::handlers::api::commits::broadcast_eval_log(
                                            state,
                                            commit.id,
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
                                        crate::handlers::api::commits::broadcast_eval_log(
                                            state,
                                            commit.id,
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
                                                                        crate::handlers::api::commits::broadcast_eval_log(
                                                                            state,
                                                                            commit.id,
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
                        if line.contains("error:") {
                            error!("nix-eval-jobs stderr: {}", line);
                        } else {
                            debug!("nix-eval-jobs stderr: {}", line);
                        }

                        // Broadcast stderr to WebSocket clients
                        if let Some(state) = cf_state {
                            crate::handlers::api::commits::broadcast_eval_log(state, commit.id, line.clone()).await;
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

    let status = child.wait().await?;
    if !status.success() {
        let stderr_text = stderr_output.join("\n");
        bail!(
            "nix-eval-jobs failed with exit code: {}\nStderr:\n{}",
            status.code().unwrap_or(-1),
            stderr_text
        );
    }

    if !found_target && target_system != "all" {
        bail!(
            "nix-eval-jobs did not evaluate target system: {}\nEvaluated systems: {:?}",
            target_system,
            results.iter().map(|r| r.attr.as_str()).collect::<Vec<_>>()
        );
    }

    // Log policy failures per-system but DON'T fail entire evaluation
    // Separate systems by whether they failed any strict policy
    let mut systems_with_strict_failures = Vec::new();
    let mut systems_with_only_non_strict_failures = Vec::new();
    let mut passed_systems = Vec::new();

    for check in &policy_checks {
        if check.meets_requirements {
            passed_systems.push(check);
        } else {
            // Check if this system failed any strict policy
            let has_strict_failure = check
                .failed_policies
                .iter()
                .any(|(_, is_strict)| *is_strict);

            if has_strict_failure {
                systems_with_strict_failures.push(check);
            } else {
                systems_with_only_non_strict_failures.push(check);
            }
        }
    }

    // Log systems that failed strict policies
    if !systems_with_strict_failures.is_empty() {
        error!(
            "⚠️  {} systems failed strict deployment policies (will not be queued for build):",
            systems_with_strict_failures.len()
        );
        for failure in &systems_with_strict_failures {
            error!("  - {}", failure.system_name);
            // Log only the strict policy failures for clarity
            for (policy_desc, is_strict) in &failure.failed_policies {
                if *is_strict {
                    error!("    • [STRICT] {}", policy_desc);
                }
            }
        }
        // DO NOT bail!() - let evaluation continue for systems that passed
    }

    // Log systems that failed only non-strict policies
    if !systems_with_only_non_strict_failures.is_empty() {
        warn!(
            "⚠️  {} systems failed non-strict deployment policies:",
            systems_with_only_non_strict_failures.len()
        );
        for failure in &systems_with_only_non_strict_failures {
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
    let total_systems = policy_checks.len();
    let failed_count =
        systems_with_strict_failures.len() + systems_with_only_non_strict_failures.len();
    if total_systems > 0 {
        info!(
            "📊 Policy evaluation summary: {}/{} systems passed, {} failed ({} strict, {} non-strict)",
            passed_systems.len(),
            total_systems,
            failed_count,
            systems_with_strict_failures.len(),
            systems_with_only_non_strict_failures.len()
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

    // Calculate statistics for summary
    let total_systems = results.len();
    let successful = results.iter().filter(|r| r.error.is_none()).count();
    let failed = total_systems - successful;

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
        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            "".to_string(), // Blank line for readability
        )
        .await;

        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            "═══════════════════════════════════════".to_string(),
        )
        .await;

        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            "📊 Evaluation Summary".to_string(),
        )
        .await;

        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            "═══════════════════════════════════════".to_string(),
        )
        .await;

        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            format!("✅ Successful: {} systems", successful),
        )
        .await;

        if failed > 0 {
            crate::handlers::api::commits::broadcast_eval_log(
                state,
                commit.id,
                format!("❌ Failed: {} systems", failed),
            )
            .await;
        }

        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            format!("📦 Total: {} nixosConfigurations", total_systems),
        )
        .await;

        if !policy_checks.is_empty() {
            crate::handlers::api::commits::broadcast_eval_log(state, commit.id, "".to_string())
                .await;

            crate::handlers::api::commits::broadcast_eval_log(
                state,
                commit.id,
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
            crate::handlers::api::commits::broadcast_eval_log(state, commit.id, "".to_string())
                .await;

            crate::handlers::api::commits::broadcast_eval_log(
                state,
                commit.id,
                format!(
                    "🚀 {} derivations ready for build queue",
                    evaluated_derivations.len()
                ),
            )
            .await;
        }

        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            "═══════════════════════════════════════".to_string(),
        )
        .await;
    }

    Ok((results, policy_checks))
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
) -> Result<(Vec<NixEvalJobResult>, Vec<PolicyCheckResult>)> {
    let systems = resolve_mock_systems(&flake.name, target_system, configured_systems)?;
    let stage_delay = mock_eval_stage_delay(systems.len());

    crate::queries::commits_artifacts::upsert_commit_artifact_cache(pool, commit.id, &systems, &[])
        .await?;

    if let Some(state) = cf_state {
        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
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
            crate::handlers::api::commits::broadcast_eval_log(
                state,
                commit.id,
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
                crate::handlers::api::commits::broadcast_eval_log(
                    state,
                    commit.id,
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
                        crate::handlers::api::commits::broadcast_eval_log(
                            state,
                            commit.id,
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
                crate::handlers::api::commits::broadcast_eval_log(
                    state,
                    commit.id,
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
                crate::handlers::api::commits::broadcast_eval_log(
                    state,
                    commit.id,
                    format!(
                        "✅ {}: policy passed (CF enabled), queued for build",
                        system_name
                    ),
                )
                .await;
            }
        }
    }

    Ok((results, checks))
}

fn build_flake_reference(repo_url: &str, commit_hash: &str) -> String {
    if repo_url.starts_with("git+") {
        if repo_url.contains("?rev=") {
            repo_url.to_string()
        } else {
            format!("{}?rev={}", repo_url, commit_hash)
        }
    } else {
        let separator = if repo_url.contains('?') { "&" } else { "?" };
        format!("git+{}{separator}rev={}", repo_url, commit_hash)
    }
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
            "hasRequiredPackages": true
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
            "hasRequiredPackages": false
        });

        let result_2 =
            PolicyCheckResult::from_json("test-system-2".to_string(), &policies_json_2, &policies);

        assert!(!result_2.meets_requirements);
        assert_eq!(result_2.failed_policies.len(), 1);
        assert_eq!(result_2.failed_policies[0].0, "Required packages: git");
        assert!(!result_2.failed_policies[0].1); // is_strict = false

        // System failing both
        let policies_json_3 = json!({
            "cfAgentEnabled": false,
            "hasRequiredPackages": false
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
