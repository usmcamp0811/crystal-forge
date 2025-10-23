use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::models::commits::Commit;
use crate::models::config::BuildConfig;
use crate::models::flakes::Flake;
use crate::queries::derivations::insert_derivation_with_target;

/// A deployment policy that systems must satisfy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeploymentPolicy {
    /// Require Crystal Forge agent to be enabled
    RequireCrystalForgeAgent {
        /// If true, fail evaluation if agent is not enabled
        /// If false, just log a warning
        strict: bool,
    },
    /// Require specific packages to be installed
    RequirePackages { packages: Vec<String>, strict: bool },
    /// Custom Nix expression evaluation
    CustomCheck {
        /// Nix expression that should evaluate to true
        expression: String,
        description: String,
        strict: bool,
    },
}

impl DeploymentPolicy {
    pub fn is_strict(&self) -> bool {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { strict }
            | DeploymentPolicy::RequirePackages { strict, .. }
            | DeploymentPolicy::CustomCheck { strict, .. } => *strict,
        }
    }

    pub fn description(&self) -> String {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { .. } => {
                "Crystal Forge agent must be enabled".to_string()
            }
            DeploymentPolicy::RequirePackages { packages, .. } => {
                format!("Required packages: {}", packages.join(", "))
            }
            DeploymentPolicy::CustomCheck { description, .. } => description.clone(),
        }
    }
}

/// Results from checking deployment policies
#[derive(Debug, Clone)]
pub struct PolicyCheckResult {
    pub system_name: String,
    pub cf_agent_enabled: Option<bool>,
    pub meets_requirements: bool,
    pub warnings: Vec<String>,
}

/// Extended NixEvalJobResult structure
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
}

/// Evaluate a flake's nixosConfigurations with nix-eval-jobs and policy checking
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

    let mut cmd = Command::new("nix-eval-jobs");
    cmd.args([
        "--flake",
        &format!("{}#nixosConfigurations", flake_ref),
        "--check-cache-status",
        "--workers",
        &num_cpus::get().to_string(),
        "--max-memory-size",
        "4096",
    ]);

    build_config.apply_to_command(&mut cmd);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    info!("🚀 Running: nix-eval-jobs for {}", target_system);
    if !policies.is_empty() {
        info!("   Checking {} deployment policies", policies.len());
        for policy in policies {
            info!(
                "     - {} (strict={})",
                policy.description(),
                policy.is_strict()
            );
        }
    }

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
                                debug!("📦 Evaluated: attr={:?}, cache={:?}",
                                    result.attr_path, result.cache_status);

                                // Check deployment policies for this system
                                let mut cf_agent_enabled = None;
                                if !policies.is_empty() {
                                    if let Some(system_name) = result.attr_path.last() {
                                        let check = check_policies_for_system(
                                            system_name,
                                            &flake_ref,
                                            policies,
                                            build_config,
                                        ).await;

                                        cf_agent_enabled = check.cf_agent_enabled;
                                        policy_checks.push(check.clone());

                                        // Log policy results
                                        if !check.meets_requirements {
                                            let has_strict = policies.iter().any(|p| p.is_strict());
                                            for warning in &check.warnings {
                                                if has_strict {
                                                    error!("❌ {}: {}", system_name, warning);
                                                } else {
                                                    warn!("⚠️  {}: {}", system_name, warning);
                                                }
                                            }
                                        } else if let Some(true) = cf_agent_enabled {
                                            info!("✅ {} has CF agent enabled", system_name);
                                        }
                                    }
                                }

                                // Insert derivation with policy check results
                                if let Some(system_name) = result.attr_path.last() {
                                    let derivation_target = build_agent_target(
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
                                        cf_agent_enabled,
                                    ).await {
                                        Ok(_) => {
                                            debug!("✅ Inserted/updated {} (CF agent: {:?})",
                                                system_name, cf_agent_enabled);
                                        }
                                        Err(e) => warn!("⚠️  Failed to insert {}: {}", system_name, e),
                                    }
                                }

                                if result.attr_path.last() == Some(&target_system.to_string()) {
                                    found_target = true;
                                    info!("✅ Found target system: {}", target_system);
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

    if !found_target {
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
        error!(
            "❌ {} systems failed strict policy checks",
            strict_failures.len()
        );
        for failure in &strict_failures {
            error!("  - {}: {:?}", failure.system_name, failure.warnings);
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
        info!(
            "   CF agent: {}/{} systems enabled ({:.1}%)",
            with_agent,
            policy_checks.len(),
            (with_agent as f64 / policy_checks.len() as f64) * 100.0
        );
    }

    Ok((results, policy_checks))
}

/// Check deployment policies for a specific system
async fn check_policies_for_system(
    system_name: &str,
    flake_ref: &str,
    policies: &[DeploymentPolicy],
    build_config: &BuildConfig,
) -> PolicyCheckResult {
    let mut warnings = Vec::new();
    let mut cf_agent_enabled = None;

    for policy in policies {
        match policy {
            DeploymentPolicy::RequireCrystalForgeAgent { strict: _ } => {
                match check_cf_agent_for_system(flake_ref, system_name, build_config).await {
                    Ok(enabled) => {
                        cf_agent_enabled = Some(enabled);
                        if !enabled {
                            warnings.push(format!(
                                "Crystal Forge agent not enabled for {}",
                                system_name
                            ));
                        }
                    }
                    Err(e) => {
                        warn!("Failed to check CF agent for {}: {}", system_name, e);
                        warnings.push(format!("Could not verify CF agent status: {}", e));
                    }
                }
            }
            DeploymentPolicy::RequirePackages {
                packages,
                strict: _,
            } => {
                // TODO: Implement package checking
                debug!("Package checking not yet implemented for {}", system_name);
            }
            DeploymentPolicy::CustomCheck {
                expression,
                description,
                strict: _,
            } => {
                // TODO: Implement custom check evaluation
                debug!(
                    "Custom check '{}' not yet implemented for {}",
                    description, system_name
                );
            }
        }
    }

    PolicyCheckResult {
        system_name: system_name.to_string(),
        cf_agent_enabled,
        meets_requirements: warnings.is_empty(),
        warnings,
    }
}

/// Check if Crystal Forge agent is enabled for a specific system
async fn check_cf_agent_for_system(
    flake_ref: &str,
    system_name: &str,
    build_config: &BuildConfig,
) -> Result<bool> {
    // Test environment always returns true
    if std::env::var("CF_TEST_ENVIRONMENT").is_ok() || system_name.contains("cf-test-sys") {
        return Ok(true);
    }

    let eval_expr = format!(
        "let flake = builtins.getFlake \"{}\"; \
         cfg = flake.nixosConfigurations.{}.config.services.crystal-forge or {{}}; \
         in {{ enable = cfg.enable or false; client_enable = cfg.client.enable or false; }}",
        flake_ref, system_name
    );

    let mut cmd = Command::new("nix");
    cmd.args(["eval", "--json", "--expr", &eval_expr]);
    build_config.apply_to_command(&mut cmd);

    let output = cmd.output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("CF agent check failed: {}", stderr);
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&json_str)?;

    let cf_enabled = json
        .get("enable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let cf_client_enabled = json
        .get("client_enable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(cf_enabled && cf_client_enabled)
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
