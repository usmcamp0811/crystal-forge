use crate::vulnix::vulnix_parser::{VulnixParser, VulnixScanResult};
use anyhow::{Context, Result};
use std::time::Instant;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

/// Configuration for vulnix scans
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
            timeout_seconds: 300, // 5 minutes
            max_retries: 2,
            enable_whitelist: true,
            extra_args: Vec::new(),
        }
    }
}

/// Async runner for vulnix scans
pub struct VulnixRunner {
    config: VulnixConfig,
}

impl VulnixRunner {
    /// Create a new VulnixRunner with default configuration
    pub fn new() -> Self {
        Self {
            config: VulnixConfig::default(),
        }
    }

    /// Create a new VulnixRunner with custom configuration
    pub fn with_config(config: VulnixConfig) -> Self {
        Self { config }
    }

    /// Run vulnix on a specific Nix path and return parsed results
    ///
    /// # Arguments
    /// * `nix_path` - Path to scan (derivation, store path, or gc root)
    /// * `evaluation_target_id` - Database ID of the evaluation target
    /// * `scanner_version` - Optional vulnix version for metadata
    ///
    /// # Examples
    /// ```no_run
    /// # use crystal_forge::vulnix::vulnix_runner::VulnixRunner;
    /// # async fn example() -> anyhow::Result<()> {
    /// let runner = VulnixRunner::new();
    /// let result = runner.scan_path(
    ///     "/nix/store/abc123-openssl-1.1.1w.drv",
    ///     42,
    ///     None
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn scan_path(
        &self,
        nix_path: &str,
        evaluation_target_id: i32,
        scanner_version: Option<String>,
    ) -> Result<VulnixScanResult> {
        let start_time = Instant::now();

        info!("🔍 Starting vulnix scan for: {}", nix_path);
        debug!("📋 Evaluation Target ID: {}", evaluation_target_id);

        // Validate the path exists (basic check)
        if !Self::validate_nix_path(nix_path) {
            anyhow::bail!("Invalid or non-existent Nix path: {}", nix_path);
        }

        // Build command arguments
        let mut args = vec!["--json".to_string()];

        // Add whitelist support if enabled
        if self.config.enable_whitelist {
            args.push("--whitelist".to_string());
        }

        // Add extra arguments from config
        args.extend(self.config.extra_args.clone());

        // Add the target path
        args.push(nix_path.to_string());

        // Execute vulnix with retries
        let json_output = self
            .execute_vulnix_with_retries(args)
            .await
            .with_context(|| format!("Failed to scan path: {}", nix_path))?;

        // Parse the JSON output
        let mut result =
            VulnixParser::parse_and_convert(&json_output, evaluation_target_id, scanner_version)
                .context("Failed to parse vulnix JSON output")?;

        // Set scan timing
        result.scan.finish_timing(start_time);

        let summary = result.summary();
        info!(
            "✅ vulnix scan completed in {:.2}s: {}",
            start_time.elapsed().as_secs_f64(),
            summary
        );

        if summary.critical_count > 0 {
            warn!(
                "⚠️  {} critical vulnerabilities found!",
                summary.critical_count
            );
        }

        Ok(result)
    }

    /// Run vulnix on the current system (evaluation target)
    pub async fn scan_evaluation_target(
        &self,
        evaluation_target_id: i32,
        scanner_version: Option<String>,
    ) -> Result<VulnixScanResult> {
        let start_time = Instant::now();

        info!("🔍 Starting vulnix scan for current system");
        debug!("📋 Evaluation Target ID: {}", evaluation_target_id);

        // Build command arguments for system scan
        let mut args = vec!["--json".to_string(), "--system".to_string()];

        if self.config.enable_whitelist {
            args.push("--whitelist".to_string());
        }

        args.extend(self.config.extra_args.clone());

        // Execute vulnix with retries
        let json_output = self
            .execute_vulnix_with_retries(args)
            .await
            .context("Failed to scan current system")?;

        debug!(
            "📄 vulnix system scan JSON output: {} bytes",
            json_output.len()
        );

        // Parse the JSON output
        let mut result =
            VulnixParser::parse_and_convert(&json_output, evaluation_target_id, scanner_version)
                .context("Failed to parse vulnix scan JSON output")?;

        // Set scan timing
        result.scan.finish_timing(start_time);

        let summary = result.summary();
        info!(
            "✅ vulnix scan completed in {:.2}s: {}",
            start_time.elapsed().as_secs_f64(),
            summary
        );

        if summary.critical_count > 0 {
            warn!(
                "⚠️  {} critical vulnerabilities found in current evaluation!",
                summary.critical_count
            );
        }

        Ok(result)
    }

    /// Run vulnix on all garbage collection roots
    pub async fn scan_gc_roots(
        &self,
        evaluation_target_id: i32,
        scanner_version: Option<String>,
    ) -> Result<VulnixScanResult> {
        let start_time = Instant::now();

        info!("🔍 Starting vulnix scan for all GC roots");
        debug!("📋 Evaluation Target ID: {}", evaluation_target_id);

        // Build command arguments for GC roots scan
        let mut args = vec!["--json".to_string(), "--gc-roots".to_string()];

        if self.config.enable_whitelist {
            args.push("--whitelist".to_string());
        }

        args.extend(self.config.extra_args.clone());

        // Execute vulnix with retries
        let json_output = self
            .execute_vulnix_with_retries(args)
            .await
            .context("Failed to scan GC roots")?;

        debug!(
            "📄 vulnix GC roots scan JSON output: {} bytes",
            json_output.len()
        );

        // Parse the JSON output
        let mut result =
            VulnixParser::parse_and_convert(&json_output, evaluation_target_id, scanner_version)
                .context("Failed to parse vulnix GC roots scan JSON output")?;

        // Set scan timing
        result.scan.finish_timing(start_time);

        let summary = result.summary();
        info!(
            "✅ vulnix GC roots scan completed in {:.2}s: {}",
            start_time.elapsed().as_secs_f64(),
            summary
        );

        Ok(result)
    }

    /// Execute vulnix command with retry logic and timeout
    async fn execute_vulnix_with_retries(&self, args: Vec<String>) -> Result<String> {
        let mut last_error = None;

        for attempt in 1..=self.config.max_retries + 1 {
            debug!(
                "🔄 vulnix attempt {} of {}",
                attempt,
                self.config.max_retries + 1
            );

            match self.execute_vulnix_command(&args).await {
                Ok(output) => return Ok(output),
                Err(e) => {
                    last_error = Some(e);
                    if attempt <= self.config.max_retries {
                        warn!("⚠️  vulnix attempt {} failed, retrying...", attempt);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All vulnix attempts failed")))
    }

    /// Execute a single vulnix command with timeout
    async fn execute_vulnix_command(&self, args: &[String]) -> Result<String> {
        debug!("🚀 Executing: vulnix {}", args.join(" "));

        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(self.config.timeout_seconds),
            Command::new("vulnix").args(args).output(),
        )
        .await
        .context("vulnix command timed out")?
        .context("Failed to execute vulnix command")?;

        // Check if vulnix succeeded
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            error!(
                "❌ vulnix failed with exit code {}: {}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            );

            // Include stdout in error context if it contains useful info
            let error_msg = if !stdout.trim().is_empty() {
                format!(
                    "vulnix scan failed: {}\nStdout: {}",
                    stderr.trim(),
                    stdout.trim()
                )
            } else {
                format!("vulnix scan failed: {}", stderr.trim())
            };

            anyhow::bail!(error_msg);
        }

        // Convert stdout to string
        let json_output =
            String::from_utf8(output.stdout).context("vulnix output contains invalid UTF-8")?;

        // Validate JSON structure before returning
        VulnixParser::validate_json_structure(&json_output)
            .context("vulnix produced invalid JSON output")?;

        debug!(
            "📄 vulnix JSON output ({} bytes): {}",
            json_output.len(),
            if json_output.len() > 500 {
                format!("{}...", &json_output[..500])
            } else {
                json_output.clone()
            }
        );

        Ok(json_output)
    }

    /// Get vulnix version for metadata
    pub async fn get_vulnix_version() -> Result<String> {
        debug!("🔍 Getting vulnix version");

        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(10), // Short timeout for version check
            Command::new("vulnix").arg("--version").output(),
        )
        .await
        .context("vulnix version command timed out")?
        .context("Failed to get vulnix version")?;

        if output.status.success() {
            let version_output = String::from_utf8_lossy(&output.stdout);
            // Parse version from output like "vulnix 1.10.1"
            let version = version_output
                .split_whitespace()
                .nth(1) // Get the second word (version number)
                .or_else(|| {
                    // Fallback: try to extract version from anywhere in the output
                    version_output
                        .split_whitespace()
                        .find(|word| word.chars().next().map_or(false, |c| c.is_ascii_digit()))
                })
                .unwrap_or("unknown")
                .to_string();

            debug!("📋 vulnix version: {}", version);
            Ok(version)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("⚠️  Could not determine vulnix version: {}", stderr.trim());
            Ok("unknown".to_string())
        }
    }

    /// Check if vulnix is available on the system
    pub async fn check_vulnix_available() -> bool {
        debug!("🔍 Checking vulnix availability");

        match tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            Command::new("vulnix").arg("--version").output(),
        )
        .await
        {
            Ok(Ok(output)) => {
                let available = output.status.success();
                if available {
                    debug!("✅ vulnix is available");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!("⚠️  vulnix command failed: {}", stderr.trim());
                }
                available
            }
            Ok(Err(e)) => {
                warn!("⚠️  vulnix execution error: {}", e);
                false
            }
            Err(_) => {
                warn!("⚠️  vulnix version check timed out");
                false
            }
        }
    }

    /// Get vulnix help/usage information
    pub async fn get_vulnix_help(&self) -> Result<String> {
        let output = Command::new("vulnix")
            .arg("--help")
            .output()
            .await
            .context("Failed to get vulnix help")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("vulnix --help failed: {}", stderr.trim())
        }
    }

    /// Validate that a Nix path exists and is valid
    fn validate_nix_path(path: &str) -> bool {
        use std::path::Path;

        // Check if path exists
        if !Path::new(path).exists() {
            warn!("⚠️  Path does not exist: {}", path);
            return false;
        }

        // Check if it's a Nix store path or derivation
        if path.starts_with("/nix/store/") {
            debug!("✅ Valid Nix store path: {}", path);
            true
        } else if path.ends_with(".drv") {
            debug!("✅ Valid derivation path: {}", path);
            true
        } else {
            // Could be a symlink to Nix store (like result links)
            match std::fs::read_link(path) {
                Ok(target) => {
                    let target_str = target.to_string_lossy();
                    if target_str.starts_with("/nix/store/") {
                        debug!("✅ Valid symlink to Nix store: {} -> {}", path, target_str);
                        true
                    } else {
                        warn!(
                            "⚠️  Symlink does not point to Nix store: {} -> {}",
                            path, target_str
                        );
                        false
                    }
                }
                Err(_) => {
                    // Not a symlink, check if it's a regular file/directory in potential Nix context
                    if path.contains("/nix/") || path.starts_with("./result") || path == "result" {
                        debug!("🤔 Treating as potential Nix path: {}", path);
                        true // Let vulnix decide if it's valid
                    } else {
                        warn!("⚠️  Path doesn't appear to be Nix-related: {}", path);
                        false
                    }
                }
            }
        }
    }

    /// Get default vulnix configuration file path if it exists
    pub fn get_default_config_path() -> Option<String> {
        let possible_paths = [
            "/etc/vulnix.toml",
            "/etc/vulnix/config.toml",
            "~/.config/vulnix.toml",
            "./vulnix.toml",
        ];

        for path in &possible_paths {
            if std::path::Path::new(path).exists() {
                return Some(path.to_string());
            }
        }

        None
    }

    /// Scan multiple paths concurrently
    pub async fn scan_paths_concurrent(
        &self,
        paths: Vec<String>,
        evaluation_target_id: i32,
        scanner_version: Option<String>,
    ) -> Result<Vec<Result<VulnixScanResult>>> {
        info!(
            "🔍 Starting concurrent vulnix scans for {} paths",
            paths.len()
        );

        let tasks: Vec<_> = paths
            .into_iter()
            .map(|path| {
                let scanner_version = scanner_version.clone();
                async move {
                    self.scan_path(&path, evaluation_target_id, scanner_version)
                        .await
                }
            })
            .collect();

        let results = futures::future::join_all(tasks).await;

        let success_count = results.iter().filter(|r| r.is_ok()).count();
        info!(
            "✅ Completed {} of {} concurrent scans successfully",
            success_count,
            results.len()
        );

        Ok(results)
    }
}

/// Convenience functions for common scan scenarios
impl VulnixRunner {
    /// Scan a derivation path with automatic version detection
    pub async fn scan_derivation(
        &self,
        derivation_path: &str,
        evaluation_target_id: i32,
    ) -> Result<VulnixScanResult> {
        let version = Self::get_vulnix_version().await.ok();
        self.scan_path(derivation_path, evaluation_target_id, version)
            .await
    }

    /// Scan current system with automatic version detection
    pub async fn scan_system(&self, evaluation_target_id: i32) -> Result<VulnixScanResult> {
        let version = Self::get_vulnix_version().await.ok();
        self.scan_evaluation_target(evaluation_target_id, version)
            .await
    }

    /// Scan GC roots with automatic version detection
    pub async fn scan_gc_roots_auto(&self, evaluation_target_id: i32) -> Result<VulnixScanResult> {
        let version = Self::get_vulnix_version().await.ok();
        self.scan_gc_roots(evaluation_target_id, version).await
    }

    /// Create a quick scan runner with minimal configuration
    pub fn quick() -> Self {
        Self::with_config(VulnixConfig {
            timeout_seconds: 60,
            max_retries: 1,
            enable_whitelist: false,
            extra_args: vec!["--quick".to_string()],
        })
    }

    /// Create a thorough scan runner with extended timeouts
    pub fn thorough() -> Self {
        Self::with_config(VulnixConfig {
            timeout_seconds: 600, // 10 minutes
            max_retries: 3,
            enable_whitelist: true,
            extra_args: vec!["--verbose".to_string()],
        })
    }
}

impl Default for VulnixRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_vulnix_availability() {
        let available = VulnixRunner::check_vulnix_available().await;
        println!("Vulnix available: {}", available);
        // This test will only pass if vulnix is installed
    }

    #[tokio::test]
    async fn test_get_vulnix_version() {
        if VulnixRunner::check_vulnix_available().await {
            let version = VulnixRunner::get_vulnix_version().await;
            println!("Vulnix version result: {:?}", version);
            assert!(version.is_ok());
            assert!(!version.unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn test_get_vulnix_help() {
        if VulnixRunner::check_vulnix_available().await {
            let runner = VulnixRunner::new();
            let help = runner.get_vulnix_help().await;
            println!("Vulnix help result: {:?}", help);
            assert!(help.is_ok());
            assert!(help.unwrap().contains("vulnix"));
        }
    }

    #[test]
    fn test_validate_nix_path() {
        // Test derivation path
        assert!(!VulnixRunner::validate_nix_path(
            "/nix/store/nonexistent-test.drv"
        ));

        // Test Nix store path
        assert!(!VulnixRunner::validate_nix_path(
            "/nix/store/nonexistent-test"
        ));

        // Test invalid path
        assert!(!VulnixRunner::validate_nix_path("/tmp/not-nix"));

        // Test result link (common pattern)
        assert!(!VulnixRunner::validate_nix_path("./result")); // Would pass if result symlink existed

        // Note: These tests use nonexistent paths, so they'll fail validation
        // In real usage, you'd test with actual Nix store paths
    }

    #[test]
    fn test_config_creation() {
        let default_config = VulnixConfig::default();
        assert_eq!(default_config.timeout_seconds, 300);
        assert_eq!(default_config.max_retries, 2);
        assert!(default_config.enable_whitelist);

        let runner = VulnixRunner::quick();
        assert_eq!(runner.config.timeout_seconds, 60);

        let thorough_runner = VulnixRunner::thorough();
        assert_eq!(thorough_runner.config.timeout_seconds, 600);
    }

    #[tokio::test]
    async fn test_scan_empty_result() {
        // This test would need a real Nix environment to work properly
        // It's here as an example of how you'd test the scanning functionality

        if !VulnixRunner::check_vulnix_available().await {
            println!("Skipping scan test - vulnix not available");
            return;
        }

        // You could test with a known clean derivation or mock the output
        // For now, just ensure the runner can be created
        let runner = VulnixRunner::new();
        assert_eq!(runner.config.timeout_seconds, 300);
    }

    #[test]
    fn test_get_default_config_path() {
        let config_path = VulnixRunner::get_default_config_path();
        // This will be None unless vulnix config files exist
        println!("Default config path: {:?}", config_path);
    }
}
