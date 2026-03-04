use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::config::{BuildConfig, ServerConfig};
use crate::models::commits::Commit;
use crate::models::deployment_policies::{
    DeploymentPolicy, PolicyCheckResult, build_nix_eval_expression,
};
use crate::models::flakes::Flake;
use crate::queries::derivations::{
    EvaluationStatus, insert_derivation_with_target, mark_derivation_dry_run_complete,
};

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
) -> Result<(Vec<NixEvalJobResult>, Vec<PolicyCheckResult>)> {
    let flake_ref = build_flake_reference(repo_url, commit_hash);

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
            flake.name, &commit_hash[..8.min(commit_hash.len())], target_system, server_config.eval_workers
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
                crate::handlers::api::commits::broadcast_eval_log(state, commit.id, policy_detail).await;
            }
        }
        
        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            "⏳ Evaluating nixosConfigurations...".to_string()
        ).await;
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
                                let has_error = result.error.is_some();
                                let drv_path = result.drv_path.clone();

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

                                            // CRITICAL: Track derivations that evaluated successfully
                                            // Only mark as complete if:
                                            // 1. No evaluation error
                                            // 2. Has a valid .drv path
                                            if !has_error && drv_path.is_some() {
                                                evaluated_derivations.push((
                                                    deriv.id,
                                                    drv_path.clone().unwrap()
                                                ));
                                                debug!("📋 Queued {} for DryRunComplete update", system_name);
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

    // Check for strict policy failures
    let strict_failures: Vec<_> = policy_checks
        .iter()
        .filter(|c| !c.meets_requirements && policies.iter().any(|p| p.is_strict()))
        .collect();

    if !strict_failures.is_empty() {
        error!("{}", strict_failures.len());
        for failure in &strict_failures {
            error!("  - {}", failure.system_name);
            for warning in &failure.warnings {
                error!("    • {}", warning);
            }
        }
        bail!(
            "{} systems failed strict deployment policies",
            strict_failures.len()
        );
    }

    // ============================================================================
    // CRITICAL FIX: Update successfully evaluated derivations
    // Sets BOTH derivation_path AND status to DryRunComplete
    // ============================================================================
    if !evaluated_derivations.is_empty() {
        info!(
            "🔄 Marking {} derivations as DryRunComplete with .drv paths...",
            evaluated_derivations.len()
        );

        for (deriv_id, drv_path) in &evaluated_derivations {
            match mark_derivation_dry_run_complete(pool, *deriv_id, drv_path).await {
                Ok(_) => {
                    debug!(
                        "✅ Marked derivation {} as DryRunComplete with path {}",
                        deriv_id, drv_path
                    );
                }
                Err(e) => {
                    warn!(
                        "⚠️  Failed to mark derivation {} as complete: {}",
                        deriv_id, e
                    );
                }
            }
        }

        info!(
            "✅ {} derivations now ready for building!",
            evaluated_derivations.len()
        );
        info!("   - Status: DryRunComplete (5)");
        info!("   - Derivation paths: populated");
        info!("   - Workers can now claim and build");
    } else {
        warn!("⚠️  No derivations successfully evaluated (all had errors or missing paths)");
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
            "".to_string()  // Blank line for readability
        ).await;
        
        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            "═══════════════════════════════════════".to_string()
        ).await;
        
        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            "📊 Evaluation Summary".to_string()
        ).await;
        
        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            "═══════════════════════════════════════".to_string()
        ).await;
        
        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            format!("✅ Successful: {} systems", successful)
        ).await;
        
        if failed > 0 {
            crate::handlers::api::commits::broadcast_eval_log(
                state,
                commit.id,
                format!("❌ Failed: {} systems", failed)
            ).await;
        }
        
        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            format!("📦 Total: {} nixosConfigurations", total_systems)
        ).await;
        
        if !policy_checks.is_empty() {
            crate::handlers::api::commits::broadcast_eval_log(
                state,
                commit.id,
                "".to_string()
            ).await;
            
            crate::handlers::api::commits::broadcast_eval_log(
                state,
                commit.id,
                format!("🔐 Policy Compliance: {:.1}% ({}/{})",
                    coverage, with_agent, policy_checks.len())
            ).await;
        }
        
        if evaluated_derivations.len() > 0 {
            crate::handlers::api::commits::broadcast_eval_log(
                state,
                commit.id,
                "".to_string()
            ).await;
            
            crate::handlers::api::commits::broadcast_eval_log(
                state,
                commit.id,
                format!("🚀 {} derivations ready for build queue", evaluated_derivations.len())
            ).await;
        }
        
        crate::handlers::api::commits::broadcast_eval_log(
            state,
            commit.id,
            "═══════════════════════════════════════".to_string()
        ).await;
    }

    Ok((results, policy_checks))
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
