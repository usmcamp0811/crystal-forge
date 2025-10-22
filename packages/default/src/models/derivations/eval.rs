use super::Derivation;
use crate::models::commits::Commit;
use crate::models::config::BuildConfig;
use crate::models::flakes::Flake;
use crate::queries::derivations::insert_derivation_with_target;
use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use sqlx::PgPool;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

#[derive(Debug)]
pub struct PackageInfo {
    pub pname: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NixEvalJobResult {
    pub attr: String,
    #[serde(rename = "attrPath")]
    pub attr_path: Vec<String>,
    #[serde(rename = "drvPath")]
    pub drv_path: Option<String>,
    pub outputs: Option<std::collections::HashMap<String, String>>,
    pub system: Option<String>,
    pub error: Option<String>,
    #[serde(rename = "cacheStatus")]
    pub cache_status: Option<String>, // "local", "cached", "notBuilt"
}

impl Derivation {
    /// Resolve a .drv path to its output store path(s)
    pub async fn resolve_drv_to_store_path(drv_path: &str) -> Result<String> {
        if !drv_path.ends_with(".drv") {
            // Already a store path, return as-is
            return Ok(drv_path.to_string());
        }

        let output = Command::new("nix-store")
            .args(["--query", "--outputs", drv_path])
            .output()
            .await
            .context("Failed to execute nix-store --query --outputs")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("nix-store --query --outputs failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let store_paths: Vec<&str> = stdout.trim().lines().collect();

        if store_paths.is_empty() {
            anyhow::bail!("No output paths found for derivation: {}", drv_path);
        }

        // Return the first output path (typically there's only one)
        Ok(store_paths[0].to_string())
    }

    /// Use nix-eval-jobs to evaluate all nixosConfigurations in parallel
    /// Returns results for all systems found in the flake
    pub async fn evaluate_with_nix_eval_jobs(
        pool: &PgPool,
        commit: &Commit,
        flake: &Flake,
        repo_url: &str,
        commit_hash: &str,
        target_system: &str,
        build_config: &BuildConfig,
    ) -> Result<Vec<NixEvalJobResult>> {
        // TODO: Eventually rework this to be generic and handle any flake output
        // Have nixosConfigurations be an arg
        let flake_ref =
            crate::models::derivations::utils::build_flake_reference(repo_url, commit_hash);

        let mut cmd = Command::new("nix-eval-jobs");
        // TODO: get memory per worker from config
        cmd.args([
            "--flake",
            &format!("{}#nixosConfigurations", flake_ref),
            "--check-cache-status", // This tells us what's already built!
            "--workers",
            &num_cpus::get().to_string(),
            "--max-memory-size",
            "4096", // 4GB per worker
        ]);

        build_config.apply_to_command(&mut cmd);

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        info!("🚀 Running: nix-eval-jobs for {}", target_system);
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
                                Ok(result) => {
                                    debug!("📦 Evaluated: attr={:?}, cache={:?}", result.attr_path, result.cache_status);

                                    // Insert immediately as we discover systems
                                    if let Some(system_name) = result.attr_path.last() {
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
                            // Log stderr immediately for debugging
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

        info!("✅ Evaluated {} configurations in parallel", results.len());

        Ok(results)
    }
}

/// Resolve a .drv path to its output store path(s) - static version
pub async fn resolve_drv_to_store_path_static(drv_path: &str) -> Result<String> {
    if !drv_path.ends_with(".drv") {
        // Already a store path, return as-is
        return Ok(drv_path.to_string());
    }

    let output = Command::new("nix-store")
        .args(["--query", "--outputs", drv_path])
        .output()
        .await
        .context("Failed to execute nix-store --query --outputs")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("nix-store --query --outputs failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let store_paths: Vec<&str> = stdout.trim().lines().collect();

    if store_paths.is_empty() {
        anyhow::bail!("No output paths found for derivation: {}", drv_path);
    }

    // Return the first output path (typically there's only one)
    Ok(store_paths[0].to_string())
}

/// Parse derivation paths from nix build stderr output (legacy function, prefer eval_main_drv_path)
pub fn parse_derivation_paths(stderr: &str, flake_target: &str) -> Result<(String, Vec<String>)> {
    let mut derivation_paths = Vec::new();
    let mut collecting = false;

    for line in stderr.lines() {
        let t = line.trim();
        if t.starts_with("these ") && t.contains("derivations will be built:") {
            collecting = true;
            debug!("🔍 Found derivation list header: {}", t);
            continue;
        }
        if collecting {
            if t.starts_with("/nix/store/") && t.ends_with(".drv") {
                derivation_paths.push(t.to_string());
                debug!("🔍 Found derivation: {}", t);
            } else if t.is_empty() || t.starts_with("[{") {
                collecting = false;
                debug!("🔍 Stopped collecting derivations at: '{}'", t);
            }
        }
    }

    if derivation_paths.is_empty() {
        bail!("no-derivations");
    }

    let main = derivation_paths
        .iter()
        .find(|p| {
            if flake_target.contains("nixosConfigurations") {
                p.contains("nixos-system")
            } else {
                true
            }
        })
        .cloned()
        .ok_or_else(|| {
            error!("❌ Could not find main derivation. Available paths:");
            for path in &derivation_paths {
                error!("  - {}", path);
            }
            anyhow!(
                "Could not determine main derivation from {} candidates",
                derivation_paths.len()
            )
        })?;

    let deps = derivation_paths
        .into_iter()
        .filter(|p| p != &main)
        .collect::<Vec<_>>();

    Ok((main, deps))
}

/// Parse derivation path to extract package information
pub fn parse_derivation_path(drv_path: &str) -> Option<PackageInfo> {
    // Derivation paths look like: /nix/store/hash-name-version.drv
    let filename = drv_path.split('/').last()?.strip_suffix(".drv")?;

    // Split by first dash to separate hash from name-version
    let (_, name_version) = filename.split_once('-')?;

    // Handle NixOS system derivations specifically
    if name_version.starts_with("nixos-system-") {
        // Pattern: nixos-system-HOSTNAME-VERSION
        // Example: nixos-system-aws-test-25.05.20250802.b6bab62
        let system_part = name_version.strip_prefix("nixos-system-")?;

        // Find the last dash to separate hostname from version
        if let Some((hostname, version)) = system_part.rsplit_once('-') {
            // Check if the last part looks like a version (contains dots or digits)
            if version.chars().any(|c| c.is_ascii_digit() || c == '.') {
                return Some(PackageInfo {
                    pname: Some(format!("nixos-system-{}", hostname)),
                    version: Some(version.to_string()),
                });
            }
        }

        // If we can't parse version, use the whole system part as pname
        return Some(PackageInfo {
            pname: Some(format!("{}", system_part)),
            version: None,
        });
    }

    // Handle regular packages
    // Try to split name and version
    // This is heuristic - Nix derivation naming isn't perfectly consistent
    if let Some((name, version)) = name_version.rsplit_once('-') {
        // Check if the last part looks like a version (starts with digit)
        if version.chars().next()?.is_ascii_digit() {
            return Some(PackageInfo {
                pname: Some(name.to_string()),
                version: Some(version.to_string()),
            });
        }
    }

    // If we can't parse version, use the whole name_version as pname
    Some(PackageInfo {
        pname: Some(name_version.to_string()),
        version: None,
    })
}

// ============================================================================
// OPTIONAL: Add these helper functions to eval.rs if needed
// ============================================================================

/// Check if this derivation has Crystal Forge agent enabled
pub async fn is_cf_agent_enabled(flake_target: &str, build_config: &BuildConfig) -> Result<bool> {
    // Test environment check
    if std::env::var("CF_TEST_ENVIRONMENT").is_ok() || flake_target.contains("cf-test-sys") {
        return Ok(true);
    }

    // Extract the system name from the flake target
    let system_name = flake_target
        .split('#')
        .nth(1)
        .and_then(|s| s.split('.').nth(1))
        .unwrap_or("unknown");

    // Extract the flake URL (before the #)
    let flake_url = flake_target.split('#').next().unwrap_or(flake_target);

    // Use a simpler evaluation that's less likely to fail
    let eval_expr = format!(
        "let flake = builtins.getFlake \"{}\"; cfg = flake.nixosConfigurations.{}.config.services.crystal-forge or {{}}; in {{ enable = cfg.enable or false; client_enable = cfg.client.enable or false; }}",
        flake_url, system_name
    );

    let mut cmd = Command::new("nix");
    cmd.args(["eval", "--json", "--expr", &eval_expr]);
    build_config.apply_to_command(&mut cmd);

    match cmd.output().await {
        Ok(output) if output.status.success() => {
            let json_str = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(json) => {
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
                Err(e) => {
                    warn!("Failed to parse CF agent status JSON: {}", e);
                    Ok(false)
                }
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("CF agent evaluation failed: {}", stderr);
            Ok(false)
        }
        Err(e) => {
            warn!("Failed to run CF agent evaluation: {}", e);
            Ok(false)
        }
    }
}

/// Parse derivation dependencies from `nix derivation show` JSON output
pub fn parse_input_drvs_from_json(json_str: &str) -> Result<Vec<String>> {
    let parsed: serde_json::Value = serde_json::from_str(json_str)?;

    let mut deps = Vec::new();

    // nix derivation show returns a map of derivation paths to their info
    if let Some(obj) = parsed.as_object() {
        for (_drv_path, drv_info) in obj {
            if let Some(input_drvs) = drv_info.get("inputDrvs") {
                if let Some(inputs) = input_drvs.as_object() {
                    for (input_drv, _outputs) in inputs {
                        deps.push(input_drv.to_string());
                    }
                }
            }
        }
    }

    Ok(deps)
}
