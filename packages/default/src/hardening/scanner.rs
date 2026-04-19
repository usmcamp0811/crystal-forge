//! Scanner for extracting and analyzing systemd service configurations.
//!
//! Uses `nix eval` to extract systemd.services from NixOS configurations.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, warn};

use super::scoring::{ServiceScoreResult, calculate_scan_statistics, calculate_service_score};

/// Scanner for extracting systemd service hardening information.
pub struct HardeningScanner {
    /// Optional Nix command override (for testing)
    nix_command: String,
}

impl Default for HardeningScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl HardeningScanner {
    pub fn new() -> Self {
        Self {
            nix_command: "nix".to_string(),
        }
    }

    /// Create scanner with custom nix command (for testing).
    #[cfg(test)]
    pub fn with_nix_command(nix_command: impl Into<String>) -> Self {
        Self {
            nix_command: nix_command.into(),
        }
    }

    /// Extract systemd services configuration from a NixOS flake output.
    ///
    /// Runs: `nix eval <flake_path>#nixosConfigurations.<config>.config.systemd.services --json`
    pub async fn extract_services(
        &self,
        flake_ref: &str,
        config_name: &str,
    ) -> Result<HashMap<String, SystemdServiceConfig>> {
        let target =
            format!("{flake_ref}#nixosConfigurations.{config_name}.config.systemd.services");

        debug!(
            "Evaluating systemd services for {} at {}",
            config_name, flake_ref
        );

        let output = Command::new(&self.nix_command)
            .args([
                "eval",
                "--json",
                "--no-write-lock-file",
                "--accept-flake-config",
                &target,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute nix eval")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("nix eval failed: {}", stderr);
        }

        let services: HashMap<String, SystemdServiceConfig> =
            serde_json::from_slice(&output.stdout)
                .context("Failed to parse nix eval output as JSON")?;

        debug!(
            "Found {} systemd services in {}",
            services.len(),
            config_name
        );

        Ok(services)
    }

    /// Scan a NixOS configuration and calculate hardening scores for all services.
    pub async fn scan_config(&self, flake_ref: &str, config_name: &str) -> Result<ScanResult> {
        let services = self.extract_services(flake_ref, config_name).await?;
        self.analyze_services(services)
    }

    /// Analyze extracted services and calculate scores.
    pub fn analyze_services(
        &self,
        services: HashMap<String, SystemdServiceConfig>,
    ) -> Result<ScanResult> {
        let mut service_results = Vec::new();

        for (name, service) in services {
            // Skip certain system-internal services that shouldn't be scored
            if Self::should_skip_service(&name) {
                debug!("Skipping internal service: {}", name);
                continue;
            }

            // Get the serviceConfig which contains the hardening directives
            let service_config = service
                .service_config
                .as_ref()
                .map(|v| v.clone())
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

            let score_result = calculate_service_score(&service_config);

            service_results.push(ServiceScanResult {
                name,
                service_type: service.service_type.clone(),
                score_result,
            });
        }

        // Calculate aggregate statistics
        let scores: Vec<_> = service_results
            .iter()
            .map(|r| r.score_result.clone())
            .collect();

        let (
            well_hardened_count,
            moderately_hardened_count,
            poorly_hardened_count,
            vulnerable_count,
            total_services,
            overall_score,
        ) = calculate_scan_statistics(&scores);

        Ok(ScanResult {
            services: service_results,
            total_services,
            well_hardened_count,
            moderately_hardened_count,
            poorly_hardened_count,
            vulnerable_count,
            overall_score,
        })
    }

    /// Determine if a service should be skipped in scoring.
    ///
    /// We skip certain internal/generated services that don't represent
    /// user-configured services and would skew results.
    fn should_skip_service(name: &str) -> bool {
        // Skip oneshot generators, systemd internal services, etc.
        let skip_prefixes = [
            "systemd-",         // systemd internal services
            "dbus",             // D-Bus services (managed by systemd)
            "user@",            // User session services
            "getty@",           // Terminal login services
            "serial-getty@",    // Serial terminal services
            "container-getty@", // Container terminal services
        ];

        let skip_suffixes = [
            "-generator", // Generator services
            ".slice",     // Resource control slices
            ".socket",    // Socket activation units
            ".timer",     // Timer units
            ".path",      // Path activation units
            ".mount",     // Mount units
            ".automount", // Automount units
            ".swap",      // Swap units
            ".target",    // Target units
            ".scope",     // Scope units
        ];

        // Check prefixes
        for prefix in skip_prefixes {
            if name.starts_with(prefix) {
                return true;
            }
        }

        // Check suffixes
        for suffix in skip_suffixes {
            if name.ends_with(suffix) {
                return true;
            }
        }

        false
    }
}

/// Parsed systemd service configuration from NixOS.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemdServiceConfig {
    /// Type of service (simple, oneshot, forking, etc.)
    #[serde(rename = "serviceConfig")]
    pub service_config: Option<Value>,

    /// Unit type
    #[serde(rename = "Type")]
    pub service_type: Option<String>,

    /// Description
    pub description: Option<String>,

    /// Whether the service is enabled
    pub enable: Option<bool>,

    /// User to run as
    pub user: Option<String>,

    /// Group to run as
    pub group: Option<String>,
}

/// Result of scanning a single service.
#[derive(Debug, Clone)]
pub struct ServiceScanResult {
    pub name: String,
    pub service_type: Option<String>,
    pub score_result: ServiceScoreResult,
}

/// Result of scanning an entire NixOS configuration.
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// Per-service results
    pub services: Vec<ServiceScanResult>,
    /// Total number of services scanned
    pub total_services: i32,
    /// Count of well-hardened services (80-100)
    pub well_hardened_count: i32,
    /// Count of moderately hardened services (60-79)
    pub moderately_hardened_count: i32,
    /// Count of poorly hardened services (40-59)
    pub poorly_hardened_count: i32,
    /// Count of vulnerable services (0-39)
    pub vulnerable_count: i32,
    /// Overall score (average across all services)
    pub overall_score: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_skip_service() {
        // Should skip
        assert!(HardeningScanner::should_skip_service("systemd-journald"));
        assert!(HardeningScanner::should_skip_service("systemd-logind"));
        assert!(HardeningScanner::should_skip_service("dbus"));
        assert!(HardeningScanner::should_skip_service("user@1000"));
        assert!(HardeningScanner::should_skip_service("getty@tty1"));

        // Should NOT skip
        assert!(!HardeningScanner::should_skip_service("nginx"));
        assert!(!HardeningScanner::should_skip_service("postgresql"));
        assert!(!HardeningScanner::should_skip_service("sshd"));
        assert!(!HardeningScanner::should_skip_service(
            "crystal-forge-server"
        ));
        assert!(!HardeningScanner::should_skip_service("my-custom-service"));
    }

    #[test]
    fn test_analyze_empty_services() {
        let scanner = HardeningScanner::new();
        let result = scanner.analyze_services(HashMap::new()).unwrap();

        assert_eq!(result.total_services, 0);
        assert_eq!(result.overall_score, None);
    }

    #[test]
    fn test_analyze_services_with_hardening() {
        let scanner = HardeningScanner::new();

        let mut services = HashMap::new();
        services.insert(
            "my-hardened-service".to_string(),
            SystemdServiceConfig {
                service_config: Some(serde_json::json!({
                    "PrivateTmp": true,
                    "NoNewPrivileges": true,
                    "ProtectSystem": "strict",
                    "ProtectHome": "tmpfs",
                    "CapabilityBoundingSet": [],
                    "SystemCallFilter": ["@system-service"]
                })),
                service_type: Some("simple".to_string()),
                description: Some("A hardened service".to_string()),
                enable: Some(true),
                user: None,
                group: None,
            },
        );

        let result = scanner.analyze_services(services).unwrap();

        assert_eq!(result.total_services, 1);
        assert!(result.overall_score.is_some());
        let score = result.overall_score.unwrap();
        assert!(score > 0, "Score should be positive for hardened service");
    }
}
