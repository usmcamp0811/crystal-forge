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
use crate::queries::derivations::insert_derivation_with_target;

/// Extended NixEvalJobResult structure with embedded policy data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NixEvalJobResult {
    pub name: String,
    #[serde(rename = "drvPath")]
    pub drv_path: Option<String>,
    pub error: Option<String>,
    pub outputs: Option<serde_json::Value>,

    /// Policy check results embedded in the nix-eval-jobs output
    pub policies: Option<serde_json::Value>,
}

/// Evaluate a flake's nixosConfigurations with nix-eval-jobs and policy checking
///
/// This runs a SINGLE nix-eval-jobs command that evaluates all systems AND
/// checks all policies in parallel using nix-eval-jobs' parallel evaluation.
///
/// # Arguments
/// * `policies` - Deployment policies to check. Empty vector for no checking.
///
/// # Returns
/// Tuple of (evaluation results, policy check results)
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

    // Run nix-eval-jobs with our custom expression
    let mut cmd = Command::new("nix-eval-jobs");
    cmd.args([
        "--expr",
        &nix_expr,
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

    loop {
        tokio::select! {
            line_result = stdout_reader.next_line(), if !stdout_done => {
                match line_result? {
                    Some(line) if !line.trim().is_empty() => {
                        match serde_json::from_str::<NixEvalJobResult>(&line) {
                            Ok(result) => {
                                let system_name = result.name.clone();

                                debug!("📦 Evaluated: {}, policies={:?}",
                                    system_name, result.policies.is_some());

                                // Extract policy check results from the nix-eval-jobs output
                                let mut cf_agent_enabled = None;
                                if let Some(policies_json) = &result.policies {
                                    // Parse policy results that were evaluated by nix-eval-jobs
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
                                }

                                // Insert derivation with policy check results
                                let derivation_target = build_agent_target(
                                    &flake.repo_url,
                                    &commit.git_commit_hash,
                                    &system_name,
                                );

                                match insert_derivation_with_target(
                                    pool,
                                    Some(commit),
                                    &system_name,
                                    "nixos",
                                    Some(&derivation_target),
                                    cf_agent_enabled,
                                ).await {
                                    Ok(_) => {
                                        debug!("✅ Inserted/updated {} (CF agent: {:?})",
                                            system_name, cf_agent_enabled);
                                    }
                                    Err(e) => warn!("⚠️  Failed to insert {}: {}", system_name, e),
                                }

                                if system_name == target_system || target_system == "all" {
                                    found_target = true;
                                    if target_system != "all" {
                                        info!("✅ Found target system: {}", target_system);
                                    }
                                }

                                if let Some(error) = &result.error {
                                    warn!("⚠️  Evaluation error for {}: {}", system_name, error);
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
            results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
        );
    }

    // Check for strict policy failures
    let strict_failures: Vec<_> = policy_checks
        .iter()
        .filter(|c| !c.meets_requirements && policies.iter().any(|p| p.is_strict()))
        .collect();

    if !strict_failures.is_empty() {
        error!(
            "❌ {} systems failed strict policy checks",
            strict_failures.len()
        );
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

/// Build the base flake reference (git+url?rev=hash)
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

/// Build the agent target string
fn build_agent_target(repo_url: &str, commit_hash: &str, system_name: &str) -> String {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    format!("{}#nixosConfigurations.{}", flake_ref, system_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_flake_reference() {
        let ref1 = build_flake_reference("https://github.com/user/repo", "abc123");
        assert_eq!(ref1, "git+https://github.com/user/repo?rev=abc123");

        let ref2 = build_flake_reference("git+https://github.com/user/repo", "abc123");
        assert_eq!(ref2, "git+https://github.com/user/repo?rev=abc123");
    }

    #[test]
    fn test_build_agent_target() {
        let target = build_agent_target("https://github.com/user/repo", "abc123", "my-system");
        assert_eq!(
            target,
            "git+https://github.com/user/repo?rev=abc123#nixosConfigurations.my-system"
        );
    }
}
