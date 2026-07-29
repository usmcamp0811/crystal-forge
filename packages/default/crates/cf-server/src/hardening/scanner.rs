//! Scanner for extracting and analyzing systemd service configurations.
//!
//! Uses `nix eval` to extract systemd.services from NixOS configurations.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::Duration;
use tracing::debug;

use crate::models::evaluate_with_policies::{
    CappedOutput, NixEvalProcessGuard, heavy_nix_limiter, read_capped,
};

use super::scoring::{ServiceScoreResult, calculate_scan_statistics, calculate_service_score};

const HARDENING_SCAN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const HARDENING_STDOUT_MAX_BYTES: usize = 64 * 1024 * 1024;
const HARDENING_STDERR_MAX_BYTES: usize = 256 * 1024;

async fn read_capped_with_overflow_signal<R>(
    mut reader: R,
    limit: usize,
    overflow: std::sync::Arc<tokio::sync::Notify>,
) -> std::io::Result<CappedOutput>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut output = CappedOutput::default();
    let mut overflow_reported = false;
    let mut chunk = [0u8; 8192];
    loop {
        let count = reader.read(&mut chunk).await?;
        if count == 0 {
            return Ok(output);
        }
        output.push(&chunk[..count], limit);
        if output.is_truncated() && !overflow_reported {
            overflow.notify_one();
            overflow_reported = true;
        }
    }
}

/// Scanner for extracting systemd service hardening information.
pub struct HardeningScanner {
    /// Optional Nix command override (for testing)
    nix_command: String,
    timeout: Duration,
    stdout_max_bytes: usize,
    stderr_max_bytes: usize,
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
            timeout: HARDENING_SCAN_TIMEOUT,
            stdout_max_bytes: HARDENING_STDOUT_MAX_BYTES,
            stderr_max_bytes: HARDENING_STDERR_MAX_BYTES,
        }
    }

    /// Create scanner with custom nix command (for testing).
    #[cfg(test)]
    pub fn with_nix_command(nix_command: impl Into<String>) -> Self {
        Self {
            nix_command: nix_command.into(),
            timeout: HARDENING_SCAN_TIMEOUT,
            stdout_max_bytes: HARDENING_STDOUT_MAX_BYTES,
            stderr_max_bytes: HARDENING_STDERR_MAX_BYTES,
        }
    }

    #[cfg(test)]
    fn with_test_limits(mut self, timeout: Duration, stdout_max_bytes: usize) -> Self {
        self.timeout = timeout;
        self.stdout_max_bytes = stdout_max_bytes;
        self
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

        let _heavy_nix_permit = heavy_nix_limiter()
            .acquire_owned()
            .await
            .context("heavy Nix evaluation limiter was closed")?;

        let mut command = Command::new(&self.nix_command);
        command
            .args([
                "eval",
                "--json",
                "--no-write-lock-file",
                "--accept-flake-config",
                "--apply",
                r#"
                    services:
                      builtins.listToAttrs (
                        builtins.concatMap
                          (name:
                            let
                              attempted = builtins.tryEval services.${name};
                            in
                              if attempted.success then
                                let
                                  svc = attempted.value;
                                  serviceConfigAttempt = builtins.tryEval (svc.serviceConfig or {});
                                  typeAttempt = builtins.tryEval (svc.Type or null);
                                  descriptionAttempt = builtins.tryEval (svc.description or null);
                                  enableAttempt = builtins.tryEval (svc.enable or null);
                                in
                                  [
                                    {
                                      inherit name;
                                      value = {
                                        serviceConfig =
                                          if serviceConfigAttempt.success
                                          then serviceConfigAttempt.value
                                          else {};
                                        Type =
                                          if typeAttempt.success
                                          then typeAttempt.value
                                          else null;
                                        description =
                                          if descriptionAttempt.success
                                          then descriptionAttempt.value
                                          else null;
                                        enable =
                                          if enableAttempt.success
                                          then enableAttempt.value
                                          else null;
                                      };
                                    }
                                  ]
                              else
                                [])
                          (builtins.attrNames services)
                      )
                "#,
                &target,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);

        let child = command.spawn().context("Failed to execute nix eval")?;
        let mut guard = NixEvalProcessGuard::from_spawned_child(child, "hardening nix eval")?;
        let leader_pid = guard
            .child_mut()
            .id()
            .context("hardening nix eval lost its leader PID")?;
        let pgid = guard.pgid();
        let stdout = guard
            .child_mut()
            .stdout
            .take()
            .context("hardening nix eval stdout was not piped")?;
        let stderr = guard
            .child_mut()
            .stderr
            .take()
            .context("hardening nix eval stderr was not piped")?;
        let overflow = std::sync::Arc::new(tokio::sync::Notify::new());
        let mut stdout_task = tokio::spawn(read_capped_with_overflow_signal(
            stdout,
            self.stdout_max_bytes,
            overflow.clone(),
        ));
        let mut stderr_task = tokio::spawn(read_capped(stderr, self.stderr_max_bytes));

        debug!(
            process_kind = "hardening",
            config_name, leader_pid, pgid, "heavy_nix_process_started"
        );

        let deadline = tokio::time::Instant::now() + self.timeout;
        let status = tokio::select! {
            status = guard.wait() => status.context("Failed to wait for hardening nix eval")?,
            _ = overflow.notified() => {
                guard.terminate().await;
                stdout_task.abort();
                stderr_task.abort();
                bail!(
                    "hardening result exceeded stdout limit of {} bytes for {}",
                    self.stdout_max_bytes,
                    config_name
                );
            }
            _ = tokio::time::sleep_until(deadline) => {
                guard.terminate().await;
                stdout_task.abort();
                stderr_task.abort();
                bail!(
                    "hardening scan timed out after {} seconds for {config_name}",
                    self.timeout.as_secs_f64()
                );
            }
        };

        let drained = tokio::time::timeout_at(deadline, async {
            tokio::join!(&mut stdout_task, &mut stderr_task)
        })
        .await;
        let (stdout_result, stderr_result) = match drained {
            Ok(results) => results,
            Err(_) => {
                guard.terminate().await;
                stdout_task.abort();
                stderr_task.abort();
                bail!("hardening scan timed out while draining evaluator output for {config_name}");
            }
        };
        let stdout = stdout_result.context("hardening stdout reader task failed")??;
        let stderr = stderr_result.context("hardening stderr reader task failed")??;

        if stdout.is_truncated() {
            guard.terminate().await;
            bail!(
                "hardening result exceeded stdout limit of {} bytes for {}",
                self.stdout_max_bytes,
                config_name
            );
        }
        guard.disarm_after_output_drained();

        debug!(
            process_kind = "hardening",
            config_name,
            leader_pid,
            pgid,
            stdout_bytes = stdout.total_bytes,
            "heavy_nix_process_finished"
        );

        if !status.success() {
            bail!("nix eval failed: {}", stderr.diagnostic_excerpt(4096));
        }

        let services: HashMap<String, SystemdServiceConfig> = serde_json::from_slice(&stdout.bytes)
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

    #[cfg(unix)]
    fn test_script(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fake-nix");
        std::fs::write(&path, format!("#!/bin/sh\n{contents}\n")).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        (directory, path)
    }

    #[cfg(unix)]
    fn process_is_alive(pid: libc::pid_t) -> bool {
        unsafe { libc::kill(pid, 0) == 0 }
    }

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

    #[tokio::test]
    #[cfg(unix)]
    async fn oversized_hardening_output_is_rejected_without_waiting_for_process_exit() {
        let (_directory, script) =
            test_script("i=0; while [ $i -lt 1024 ]; do printf x; i=$((i + 1)); done; sleep 60");
        let scanner = HardeningScanner::with_nix_command(script.to_string_lossy())
            .with_test_limits(Duration::from_secs(5), 64);

        let started = std::time::Instant::now();
        let error = scanner
            .extract_services("ignored", "test")
            .await
            .expect_err("oversized stdout must fail the scan");

        assert!(
            error
                .to_string()
                .contains("exceeded stdout limit of 64 bytes")
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn timed_out_hardening_scan_kills_descendant_process_group() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("descendant.pid");
        let script_body = format!("sleep 60 & echo $! > '{}'; wait", pid_file.display());
        let (_script_directory, script) = test_script(&script_body);
        let scanner = HardeningScanner::with_nix_command(script.to_string_lossy())
            .with_test_limits(Duration::from_millis(100), 1024);

        let error = scanner
            .extract_services("ignored", "test")
            .await
            .expect_err("hung hardening scan must time out");
        assert!(error.to_string().contains("timed out"));

        let descendant_pid: libc::pid_t = std::fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while process_is_alive(descendant_pid) {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("timed-out hardening descendant must be killed and reaped");
    }
}
