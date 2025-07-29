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
        Self {
            config: config.clone(),
        }
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
        // Check if the derivation path actually exists on the filesystem
        if !tokio::fs::try_exists(&derivation_path)
            .await
            .unwrap_or(false)
        {
            return Err(anyhow!(
                "Derivation path does not exist on filesystem: {}",
                derivation_path
            ));
        }
        info!(
            "🔍 Scanning target {} with derivation path: {}",
            evaluation_target_id, derivation_path
        );
        // Build vulnix command
        let mut cmd = AsyncCommand::new("vulnix");
        cmd.arg("--json").arg(derivation_path);
        if self.config.enable_whitelist {
            cmd.arg("--whitelist").arg("/etc/vulnix-whitelist.toml");
        }
        // Add extra args
        for arg in &self.config.extra_args {
            cmd.arg(arg);
        }

        // Log the exact command being executed
        let program = cmd.as_std().get_program();
        let args: Vec<&std::ffi::OsStr> = cmd.as_std().get_args().collect();
        let args_str: Vec<String> = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();
        info!("🔧 Executing command: {:?} {}", program, args_str.join(" "));

        match tokio::time::timeout(self.config.timeout, cmd.output()).await {
            Ok(Ok(output)) => {
                let stdout_msg = String::from_utf8_lossy(&output.stdout);
                let stderr_msg = String::from_utf8_lossy(&output.stderr);

                info!("🔍 Vulnix exit code: {}", output.status);
                info!("🔍 Stdout length: {} bytes", output.stdout.len());
                info!("🔍 Stderr length: {} bytes", output.stderr.len());

                // Log first and last 200 chars of stdout for debugging
                if !stdout_msg.is_empty() {
                    let stdout_preview = if stdout_msg.len() > 400 {
                        format!(
                            "{}...{}",
                            &stdout_msg[..200],
                            &stdout_msg[stdout_msg.len() - 200..]
                        )
                    } else {
                        stdout_msg.to_string()
                    };
                    info!("🔍 Stdout preview: {}", stdout_preview.replace('\n', "\\n"));
                }

                // Always log stderr if present
                if !stderr_msg.is_empty() {
                    info!("🔍 Stderr content: {}", stderr_msg);
                }

                // Vulnix exit codes:
                // 0 = success, no vulnerabilities found
                // 2 = success, vulnerabilities found
                // other = actual failure
                let exit_code = output.status.code().unwrap_or(-1);
                if output.status.success() || exit_code == 2 {
                    // Parse vulnix JSON output directly
                    let vulnix_entries: VulnixScanOutput = serde_json::from_str(&stdout_msg)
                        .map_err(|e| anyhow!("Failed to parse vulnix JSON output: {}", e))?;
                    info!(
                        "✅ Vulnix scan completed successfully with {} entries",
                        vulnix_entries.len()
                    );
                    Ok(vulnix_entries)
                } else {
                    error!("❌ Vulnix scan failed with exit code: {}", output.status);
                    error!("❌ stderr: {}", stderr_msg);
                    Err(anyhow!("Vulnix scan failed: {}", stderr_msg))
                }
            }
            Ok(Err(e)) => {
                error!("❌ Failed to execute vulnix command: {}", e);
                Err(anyhow!("Failed to execute vulnix: {}", e))
            }
            Err(_) => {
                error!(
                    "❌ Vulnix scan timed out after {} seconds",
                    self.config.timeout_seconds()
                );
                Err(anyhow!(
                    "Vulnix scan timed out after {} seconds",
                    self.config.timeout_seconds()
                ))
            }
        }
    }
}

impl Default for VulnixRunner {
    fn default() -> Self {
        Self::new()
    }
}
