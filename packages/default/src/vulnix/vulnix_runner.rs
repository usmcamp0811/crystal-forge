use crate::vulnix::vulnix_parser::VulnixParser;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use tokio::process::Command as AsyncCommand;
use tracing::{debug, error, info, warn};

// TODO: Add vulnix config things to the CF config
#[derive(Debug, Clone)]
pub struct VulnixConfig {
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub enable_whitelist: bool,
    pub extra_args: Vec<String>,
}

impl Default for VulnixConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 300,
            max_retries: 2,
            enable_whitelist: true,
            extra_args: vec![],
        }
    }
}

/// Single entry from vulnix JSON output - represents one affected derivation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VulnixEntry {
    /// Package name and version
    pub name: String,
    /// Package name without version
    pub pname: String,
    /// Version only
    pub version: String,
    /// List of applicable CVE identifiers
    pub affected_by: Vec<String>,
    /// List of CVE identifiers which are masked by whitelist entries
    pub whitelisted: Vec<String>,
    /// Pathname of the scanned derivation file
    pub derivation: String,
    /// Dict of CVSS v3 impact base scores for each CVE found
    pub cvssv3_basescore: HashMap<String, f64>,
}

/// Array of VulnixEntry - this is what vulnix outputs as JSON
pub type VulnixScanOutput = Vec<VulnixEntry>;

#[derive(Debug)]
pub struct VulnixRunner {
    config: VulnixConfig,
}

impl VulnixRunner {
    pub fn new() -> Self {
        Self {
            config: VulnixConfig::default(),
        }
    }

    pub fn with_config(config: VulnixConfig) -> Self {
        Self { config }
    }

    /// Check if vulnix is available on the system
    pub async fn check_vulnix_available() -> bool {
        match Command::new("vulnix").arg("--version").output() {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    /// Get vulnix version string
    pub async fn get_vulnix_version() -> Result<String> {
        let output = AsyncCommand::new("vulnix")
            .arg("--version")
            .output()
            .await?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(version)
        } else {
            Err(anyhow!("Failed to get vulnix version"))
        }
    }

    /// Scan a specific derivation path
    pub async fn scan_path(
        &self,
        derivation_path: &str,
        evaluation_target_id: i32,
        vulnix_version: Option<String>,
    ) -> Result<VulnixScanOutput> {
        info!("🔍 Scanning derivation path: {}", derivation_path);

        // Build vulnix command
        let mut cmd = AsyncCommand::new("vulnix");
        cmd.arg("--json").arg("--system").arg(derivation_path);

        if self.config.enable_whitelist {
            cmd.arg("--whitelist").arg("/etc/vulnix-whitelist.toml");
        }

        // Add extra args
        for arg in &self.config.extra_args {
            cmd.arg(arg);
        }

        // Execute scan with timeout
        let timeout = tokio::time::Duration::from_secs(self.config.timeout_seconds);

        match tokio::time::timeout(timeout, cmd.output()).await {
            Ok(Ok(output)) => {
                if output.status.success() {
                    let json_output = String::from_utf8_lossy(&output.stdout);

                    // Parse vulnix JSON output directly
                    let vulnix_entries: VulnixScanOutput = serde_json::from_str(&json_output)
                        .map_err(|e| anyhow!("Failed to parse vulnix JSON output: {}", e))?;

                    info!(
                        "✅ Vulnix scan completed successfully with {} entries",
                        vulnix_entries.len()
                    );
                    Ok(vulnix_entries)
                } else {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    Err(anyhow!("Vulnix scan failed: {}", error_msg))
                }
            }
            Ok(Err(e)) => Err(anyhow!("Failed to execute vulnix: {}", e)),
            Err(_) => Err(anyhow!(
                "Vulnix scan timed out after {} seconds",
                self.config.timeout_seconds
            )),
        }
    }

    // Scan an evaluation target (fallback method)
    // pub async fn scan_evaluation_target(
    //     &self,
    //     evaluation_target_id: i32,
    //     vulnix_version: Option<String>,
    // ) -> Result<DatabaseScanResult> {
    //     warn!(
    //         "🔄 Fallback scan for evaluation target {}",
    //         evaluation_target_id
    //     );
    //
    //     // For now, create an empty scan result
    //     // In a real implementation, this would scan the system or use a different method
    //     let mut scan_result =
    //         DatabaseScanResult::new(evaluation_target_id, "vulnix".to_string(), vulnix_version);
    //
    //     // Simulate a quick scan
    //     tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    //     scan_result.complete();
    //
    //     warn!(
    //         "⚠️ Performed empty fallback scan for target {}",
    //         evaluation_target_id
    //     );
    //     Ok(scan_result)
    // }

    // Scan multiple paths concurrently
    //     pub async fn scan_paths_concurrent(
    //         &self,
    //         paths: Vec<String>,
    //         evaluation_target_id: i32,
    //         scanner_version: Option<String>,
    //     ) -> Result<Vec<Result<DatabaseScanResult>>> {
    //         info!(
    //             "🔍 Starting concurrent vulnix scans for {} paths",
    //             paths.len()
    //         );
    //
    //         let tasks: Vec<_> = paths
    //             .into_iter()
    //             .map(|path| {
    //                 let scanner_version = scanner_version.clone();
    //                 async move {
    //                     self.scan_path(&path, evaluation_target_id, scanner_version)
    //                         .await
    //                 }
    //             })
    //             .collect();
    //
    //         let results: Vec<_> = futures::future::join_all(tasks).await;
    //
    //         let success_count = results.iter().filter(|r| r.is_ok()).count();
    //         info!(
    //             "✅ Completed {} of {} concurrent scans successfully",
    //             success_count,
    //             results.len()
    //         );
    //
    //         Ok(results)
    //     }
}

impl Default for VulnixRunner {
    fn default() -> Self {
        Self::new()
    }
}
