use crate::models::config::VulnixConfig;
use crate::vulnix::vulnix_parser::{VulnixEntry, VulnixParser};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::process::Command;
use tokio::process::Command as AsyncCommand;
use tracing::{debug, error, info, warn};

/// Array of VulnixEntry - this is what vulnix outputs as JSON
pub type VulnixScanOutput = Vec<VulnixEntry>;

#[derive(Debug)]
pub struct VulnixRunner {
    config: VulnixConfig,
}
// TODO: get from CyrstalForgeConfig
impl VulnixRunner {
    pub fn new() -> Self {
        Self {
            config: VulnixConfig::default(),
        }
    }

    pub fn with_config(config: &VulnixConfig) -> Self {
        Self { config: config.clone() }
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
    pub async fn scan_target(
        &self,
        pool: &PgPool,
        evaluation_target_id: i32,
        vulnix_version: Option<String>,
    ) -> Result<VulnixScanOutput> {
        // Get the evaluation target and extract derivation path
        let target =
            crate::queries::evaluation_targets::get_target_by_id(pool, evaluation_target_id)
                .await?;

        let derivation_path = target.derivation_path.ok_or_else(|| {
            anyhow!(
                "Evaluation target {} has no derivation path",
                evaluation_target_id
            )
        })?;

        info!(
            "🔍 Scanning target {} with derivation path: {}",
            evaluation_target_id, derivation_path
        );

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
}

impl Default for VulnixRunner {
    fn default() -> Self {
        Self::new()
    }
}
