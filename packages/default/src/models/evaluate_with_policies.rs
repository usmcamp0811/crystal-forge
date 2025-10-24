use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::models::commits::Commit;
use crate::models::config::BuildConfig;
use crate::models::deployment_policies::{
    DeploymentPolicy, PolicyCheckResult, build_nix_eval_expression,
};
use crate::models::flakes::Flake;
use crate::queries::derivations::{EvaluationStatus, insert_derivation_with_target};

/// NixEvalJobResult with meta field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NixEvalJobResult {
    pub attr: String,
    #[serde(rename = "attrPath")]
    pub attr_path: Vec<String>,
    pub name: String,
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
    policies: &[DeploymentPolicy],
) -> Result<(Vec<NixEvalJobResult>, Vec<PolicyCheckResult>)> {
    let flake_ref = build_flake_reference(repo_url, commit_hash);

    // Build ONE Nix expression that includes policy checks
    let nix_expr = build_nix_eval_expression(&flake_ref, policies);

    info!(
        "🚀 Running: nix-eval-jobs for {} with {} policies",
        target_system,
        policies.len()
    );
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
        &num_cpus::get().to_string(),
        "--max-memory-size",
        "4096",
    ]);

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
                                            }
                                        } else if let Some(true) = cf_agent_enabled {
                                            info!("✅ {} has CF agent enabled", system_name);
                                        }

                                        policy_checks.push(check);
                                    } else {
                                        debug!("⚠️  No policies in meta for {}", system_name);
                                    }
                                } else {
                                    debug!("⚠️  No meta field for {}", system_name);
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
            match sqlx::query!(
                r#"
                UPDATE derivations
                SET 
                    status_id = $1,           -- DryRunComplete (5)
                    derivation_path = $2,     -- Store the .drv path!
                    completed_at = NOW()
                WHERE id = $3
                "#,
                EvaluationStatus::DryRunComplete.as_id(), // Status 5
                drv_path,                                 // The .drv path from nix-eval-jobs
                deriv_id
            )
            .execute(pool)
            .await
            {
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
    if !policies.is_empty() && !policy_checks.is_empty() {
        let with_agent = policy_checks
            .iter()
            .filter(|c| c.cf_agent_enabled == Some(true))
            .count();
        let coverage = if policy_checks.len() > 0 {
            (with_agent as f64 / policy_checks.len() as f64) * 100.0
        } else {
            0.0
        };
        info!(
            "   CF agent: {}/{} systems enabled ({:.1}%)",
            with_agent,
            policy_checks.len(),
            coverage
        );
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
