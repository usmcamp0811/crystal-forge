use crate::config::VulnixConfig;
use crate::vulnix::vulnix_parser::VulnixEntry;

use anyhow::{Result, anyhow};
use sqlx::PgPool;
use std::process::Command;
use tokio::process::Command as AsyncCommand;
use tracing::{error, info};

/// Array of VulnixEntry - this is what vulnix outputs as JSON
pub type VulnixScanOutput = Vec<VulnixEntry>;

fn parse_successful_vulnix_output(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<VulnixScanOutput> {
    // vulnix uses exit 2 to report that the valid JSON result contains at
    // least one unwhitelisted vulnerability. Exit 1 and all other statuses
    // are process failures.
    if !matches!(exit_code, Some(0 | 2)) {
        let stderr = stderr.trim();
        let stderr = if stderr.is_empty() {
            "vulnix produced no stderr output"
        } else {
            stderr
        };
        return Err(anyhow!(
            "Vulnix scan process failed with exit code {}: {}",
            exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string()),
            stderr
        ));
    }

    serde_json::from_str(stdout)
        .map_err(|error| anyhow!("Failed to parse vulnix JSON output: {error}"))
}

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

    /// Scan a specific derivation
    pub async fn scan_derivation(
        &self,
        pool: &PgPool,
        derivation_id: i32,
        vulnix_version: Option<String>,
    ) -> Result<VulnixScanOutput> {
        // Fetch store path in a separate scope so connection is released
        let store_path = {
            let derivation =
                crate::queries::derivations::get_derivation_by_id(pool, derivation_id).await?;
            derivation
                .store_path
                .ok_or_else(|| anyhow!("Derivation {} has no store_path", derivation_id))?
        }; // Connection released here when `derivation` goes out of scope

        // Only scan if the path exists
        if !tokio::fs::try_exists(&store_path).await.unwrap_or(false) {
            return Err(anyhow!(
                "Derivation store_path does not exist: {}",
                store_path
            ));
        }

        info!(
            "🔍 Scanning derivation {} with store path: {}",
            derivation_id, store_path
        );

        // Build vulnix command
        let mut cmd = AsyncCommand::new("vulnix");
        // Ownership heartbeats may cancel a scan if its lease is lost. Ensure
        // dropping the command future terminates the child instead of leaving
        // an unowned vulnix process running in the background.
        cmd.kill_on_drop(true);
        cmd.arg("--json").arg(&store_path);

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

                if matches!(output.status.code(), Some(0 | 2)) {
                    let vulnix_entries = parse_successful_vulnix_output(
                        output.status.code(),
                        &stdout_msg,
                        &stderr_msg,
                    )?;
                    info!(
                        "✅ Vulnix scan completed successfully with {} entries",
                        vulnix_entries.len()
                    );
                    Ok(vulnix_entries)
                } else {
                    error!("❌ Vulnix scan failed with exit code: {}", output.status);
                    error!("❌ stderr: {}", stderr_msg);
                    parse_successful_vulnix_output(output.status.code(), &stdout_msg, &stderr_msg)
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

    /// Backward compatibility method - delegates to scan_derivation
    pub async fn scan_target(
        &self,
        pool: &PgPool,
        derivation_id: i32,
        vulnix_version: Option<String>,
    ) -> Result<VulnixScanOutput> {
        self.scan_derivation(pool, derivation_id, vulnix_version)
            .await
    }
}

impl Default for VulnixRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_successful_vulnix_output;

    #[test]
    fn nonzero_vulnix_exit_returns_stderr_without_parsing_json() {
        let error = parse_successful_vulnix_output(
            Some(1),
            "",
            "vulnix.nix.DeriverLookupError: Cannot determine deriver",
        )
        .expect_err("a nonzero vulnix exit must fail before JSON parsing");

        let message = error.to_string();
        assert!(message.contains("DeriverLookupError"));
        assert!(!message.contains("EOF while parsing"));
    }

    #[test]
    fn malformed_json_from_successful_vulnix_exit_is_a_parse_error() {
        let error = parse_successful_vulnix_output(Some(0), "not-json", "")
            .expect_err("successful malformed output must fail parsing");

        assert!(
            error
                .to_string()
                .contains("Failed to parse vulnix JSON output")
        );
    }

    #[test]
    fn valid_json_from_successful_vulnix_exit_is_accepted() {
        let entries = parse_successful_vulnix_output(Some(0), "[]", "")
            .expect("successful valid output should parse");

        assert!(entries.is_empty());
    }

    #[test]
    fn valid_nonempty_json_from_vulnerability_exit_is_accepted() {
        let json = r#"[{"name":"openssl-3.0.0","pname":"openssl","version":"3.0.0","derivation":"/nix/store/example-openssl.drv","affected_by":["CVE-2026-0001"],"whitelisted":[],"cvssv3_basescore":{"CVE-2026-0001":9.8}}]"#;

        let entries = parse_successful_vulnix_output(Some(2), json, "")
            .expect("exit 2 with valid vulnerability JSON must parse");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].affected_by, ["CVE-2026-0001"]);
    }

    #[test]
    fn malformed_json_from_vulnerability_exit_is_a_parse_error() {
        let error = parse_successful_vulnix_output(Some(2), "not-json", "")
            .expect_err("exit 2 does not make malformed JSON valid");

        assert!(
            error
                .to_string()
                .contains("Failed to parse vulnix JSON output")
        );
    }
}
