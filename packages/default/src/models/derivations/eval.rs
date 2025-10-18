use super::Derivation;
use anyhow::{Context, Result, anyhow, bail};
use tokio::process::Command;
use tracing::{debug, error};

#[derive(Debug)]
pub struct PackageInfo {
    pub pname: Option<String>,
    pub version: Option<String>,
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
