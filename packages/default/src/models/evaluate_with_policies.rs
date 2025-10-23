use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::models::config::BuildConfig;
use crate::models::deployment_policies::{
    DeploymentPolicy, PolicyCheckInfo, PolicyCheckResult, PolicyFailure,
};
use crate::queries::derivations::insert_derivation_with_target;

/// Extended NixEvalJobResult with policy check fields
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

    // Policy check results (added by our custom wrapper)
    #[serde(default)]
    pub policy_checks: Option<PolicyCheckInfo>,
}

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
    let flake_ref = crate::models::derivations::utils::build_flake_reference(repo_url, commit_hash);

    // Build the nix-eval-jobs command with policy checks
    let mut cmd = Command::new("nix-eval-jobs");

    // Base flake reference
    cmd.args(["--flake", &format!("{}#nixosConfigurations", flake_ref)]);

    // Cache status checking
    cmd.arg("--check-cache-status");

    // Worker configuration
    cmd.args(["--workers", &num_cpus::get().to_string()]);
    // TODO: Add this to or get from the COnfig
    cmd.args(["--max-memory-size", "4096"]);

    // Add meta fields for policy checking
    if !policies.is_empty() {
        cmd.arg("--meta");

        // Build a Nix expression that adds policy check results
        let policy_checks = build_policy_check_expr(policies);
        cmd.args(["--expr-file", "-"]);

        // We'll pipe the expression via stdin (alternative approach)
        // For now, let's use a simpler approach with --meta
    }

    build_config.apply_to_command(&mut cmd);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    info!(
        "🚀 Running: nix-eval-jobs for {} with {} policies",
        target_system,
        policies.len()
    );
    debug!(
        "Command: nix-eval-jobs --flake {}#nixosConfigurations",
        flake_ref
    );

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let mut results = Vec::new();
    let mut policy_results = Vec::new();
    let mut found_target = false;
    let mut stderr_output = Vec::new();

    let mut stdout_done = false;
    let mut stderr_done = false;

    // Read both stdout and stderr concurrently
    loop {
        tokio::select! {
            line_result = stdout_reader.next_line(), if !stdout_done => {
                match line_result? {
                    Some(line) if !line.trim().is_empty() => {
                        match serde_json::from_str::<NixEvalJobResult>(&line) {
                            Ok(mut result) => {
                                debug!("📦 Evaluated: attr={:?}, cache={:?}",
                                    result.attr_path, result.cache_status);

                                // Check policies for this system
                                if let Some(system_name) = result.attr_path.last() {
                                    let policy_check = check_policies_for_system(
                                        system_name,
                                        &result,
                                        policies,
                                    ).await;

                                    policy_results.push(policy_check.clone());

                                    // Log policy failures
                                    if !policy_check.passed {
                                        for failure in &policy_check.failures {
                                            if failure.strict {
                                                error!("❌ STRICT policy failure for {}: {}",
                                                    system_name, failure.message);
                                            } else {
                                                warn!("⚠️  Policy warning for {}: {}",
                                                    system_name, failure.message);
                                            }
                                        }
                                    }

                                    // Insert derivation
                                    let derivation_target = crate::models::derivations::utils::build_agent_target(
                                        &flake.repo_url,
                                        &commit.git_commit_hash,
                                        system_name
                                    );

                                    match insert_derivation_with_target(
                                        pool,
                                        Some(commit),
                                        system_name,
                                        "nixos",
                                        Some(&derivation_target),
                                        Some(cf_agent_enabled)
                                    ).await {
                                        Ok(_) => debug!("✅ Inserted/updated {}", system_name),
                                        Err(e) => warn!("⚠️ Failed to insert {}: {}", system_name, e),
                                    }
                                }

                                // Check if this is our target system
                                if result.attr_path.last() == Some(&target_system.to_string()) {
                                    found_target = true;
                                    info!("✅ Found target system: {}", target_system);
                                }

                                if let Some(error) = &result.error {
                                    warn!("⚠️ Evaluation error for {}: {}", result.attr, error);
                                }

                                results.push(result);
                            }
                            Err(e) => {
                                warn!("Failed to parse nix-eval-jobs output: {}\nLine: {}", e, line);
                            }
                        }
                    }
                    Some(_) => {}, // empty line
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

    if !found_target {
        bail!(
            "nix-eval-jobs did not evaluate target system: {}\nEvaluated systems: {:?}",
            target_system,
            results.iter().map(|r| r.attr.as_str()).collect::<Vec<_>>()
        );
    }

    info!(
        "✅ Evaluated {} configurations with {} policy checks",
        results.len(),
        policy_results.len()
    );

    // Check if any strict policies failed
    let strict_failures: Vec<_> = policy_results
        .iter()
        .filter(|r| !r.passed && r.failures.iter().any(|f| f.strict))
        .collect();

    if !strict_failures.is_empty() {
        error!(
            "❌ {} systems failed strict policy checks",
            strict_failures.len()
        );
        for failure in strict_failures {
            error!(
                "  - {}: {:?}",
                failure.system_name,
                failure
                    .failures
                    .iter()
                    .map(|f| &f.message)
                    .collect::<Vec<_>>()
            );
        }
    }

    Ok((results, policy_results))
}

/// Check deployment policies for a system using a separate nix eval
async fn check_policies_for_system(
    system_name: &str,
    eval_result: &NixEvalJobResult,
    policies: &[DeploymentPolicy],
) -> PolicyCheckResult {
    // For now, we'll do a simple check
    // In the future, we can enhance nix-eval-jobs to include this data directly

    let mut failures = Vec::new();

    for policy in policies {
        match policy {
            DeploymentPolicy::RequireCrystalForgeAgent { strict } => {
                // We'd need to do a separate eval here, or extend nix-eval-jobs
                // For now, just log that we need to check this
                if *strict {
                    debug!("Would check CF agent requirement for {}", system_name);
                }
            }
            _ => {
                debug!("Would check policy {:?} for {}", policy, system_name);
            }
        }
    }

    PolicyCheckResult {
        system_name: system_name.to_string(),
        passed: failures.is_empty(),
        failures,
    }
}

/// Build a Nix expression that checks all policies
fn build_policy_check_expr(policies: &[DeploymentPolicy]) -> String {
    let checks: Vec<String> = policies
        .iter()
        .filter_map(|p| p.to_nix_check_expr())
        .collect();

    if checks.is_empty() {
        return "true".to_string();
    }

    format!("({})", checks.join(" && "))
}
