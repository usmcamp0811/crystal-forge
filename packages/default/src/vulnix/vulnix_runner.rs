use crate::vulnix::vulnix_parser::{VulnixParser, VulnixScanResult};
use anyhow::{Context, Result};
use std::time::Instant;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

/// Async runner for vulnix scans
pub struct VulnixRunner;

impl VulnixRunner {
    /// Run vulnix on a specific Nix path and return parsed results
    ///
    /// # Arguments
    /// * `nix_path` - Path to scan (derivation, store path, or gc root)
    /// * `evaluation_target` - Target to scan
    /// * `scanner_version` - Optional vulnix version for metadata
    ///
    /// # Examples
    /// ```rust
    /// let result = VulnixRunner::scan_path(
    ///     "/nix/store/abc123-openssl-1.1.1w.drv",
    ///     42,
    ///     None
    /// ).await?;
    /// ```
    pub async fn scan_path(
        nix_path: &str,
        evaluation_target: &str,
        scanner_version: Option<String>,
    ) -> Result<VulnixScanResult> {
        let start_time = Instant::now();

        info!("🔍 Starting vulnix scan for: {}", nix_path);
        debug!("📋 Evaluation Target: {}", evaluation_target);

        // Validate the path exists (basic check)
        if !Self::validate_nix_path(nix_path) {
            anyhow::bail!("Invalid or non-existent Nix path: {}", nix_path);
        }

        // Run vulnix with JSON output
        // TODO: make sure this runs on the evaluation target path
        let output = Command::new("vulnix")
            .args(["--json", nix_path])
            .output()
            .await
            .context("Failed to execute vulnix command")?;

        // Check if vulnix succeeded
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "❌ vulnix failed with exit code {}: {}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            );
            anyhow::bail!("vulnix scan failed: {}", stderr.trim());
        }

        // Convert stdout to string
        let json_output =
            String::from_utf8(output.stdout).context("vulnix output contains invalid UTF-8")?;

        debug!(
            "📄 vulnix JSON output ({} bytes): {}",
            json_output.len(),
            if json_output.len() > 500 {
                format!("{}...", &json_output[..500])
            } else {
                json_output.clone()
            }
        );

        // Parse the JSON output
        let mut result =
            VulnixParser::parse_and_convert(&json_output, evaluation_target, scanner_version)
                .context("Failed to parse vulnix JSON output")?;

        // Set scan timing
        result.scan.finish_timing(start_time);

        let summary = result.summary();
        info!(
            "✅ vulnix scan completed in {:.2}s: {} packages, {} CVEs ({} critical, {} high)",
            start_time.elapsed().as_secs_f64(),
            summary.total_packages,
            summary.total_cves,
            summary.critical_count,
            summary.high_count
        );

        if summary.critical_count > 0 {
            warn!(
                "⚠️  {} critical vulnerabilities found!",
                summary.critical_count
            );
        }

        Ok(result)
    }

    /// Run vulnix on the current system (uses --system flag)
    pub async fn scan_current_system(
        evaluation_target: i32,
        scanner_version: Option<String>,
    ) -> Result<VulnixScanResult> {
        let start_time = Instant::now();

        info!("🔍 Starting vulnix scan for current system");
        debug!("📋 Evaluation Target: {}", evaluation_target);

        // Run vulnix on current system
        let output = Command::new("vulnix")
            .args(["--json", "--system"])
            .output()
            .await
            .context("Failed to execute vulnix system scan")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "❌ vulnix system scan failed with exit code {}: {}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            );
            anyhow::bail!("vulnix system scan failed: {}", stderr.trim());
        }

        let json_output = String::from_utf8(output.stdout)
            .context("vulnix system scan output contains invalid UTF-8")?;

        debug!(
            "📄 vulnix system scan JSON output: {} bytes",
            json_output.len()
        );

        // Parse the JSON output
        let mut result =
            VulnixParser::parse_and_convert(&json_output, evaluation_target, scanner_version)
                .context("Failed to parse vulnix scan JSON output")?;

        // Set scan timing
        result.scan.finish_timing(start_time);

        let summary = result.summary();
        info!(
            "✅ vulnix scan completed in {:.2}s: {} packages, {} CVEs ({} critical, {} high)",
            start_time.elapsed().as_secs_f64(),
            summary.total_packages,
            summary.total_cves,
            summary.critical_count,
            summary.high_count
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
        evaluation_target: i32,
        scanner_version: Option<String>,
    ) -> Result<VulnixScanResult> {
        let start_time = Instant::now();

        info!("🔍 Starting vulnix scan for all GC roots");
        debug!("📋 Evaluation Target: {}", evaluation_target);

        // Run vulnix on GC roots
        let output = Command::new("vulnix")
            .args(["--json", "--gc-roots"])
            .output()
            .await
            .context("Failed to execute vulnix GC roots scan")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "❌ vulnix GC roots scan failed with exit code {}: {}",
                output.status.code().unwrap_or(-1),
                stderr.trim()
            );
            anyhow::bail!("vulnix GC roots scan failed: {}", stderr.trim());
        }

        let json_output = String::from_utf8(output.stdout)
            .context("vulnix GC roots scan output contains invalid UTF-8")?;

        debug!(
            "📄 vulnix GC roots scan JSON output: {} bytes",
            json_output.len()
        );

        // Parse the JSON output
        let mut result =
            VulnixParser::parse_and_convert(&json_output, evaluation_target, scanner_version)
                .context("Failed to parse vulnix GC roots scan JSON output")?;

        // Set scan timing
        result.scan.finish_timing(start_time);

        let summary = result.summary();
        info!(
            "✅ vulnix GC roots scan completed in {:.2}s: {} packages, {} CVEs ({} critical, {} high)",
            start_time.elapsed().as_secs_f64(),
            summary.total_packages,
            summary.total_cves,
            summary.critical_count,
            summary.high_count
        );

        Ok(result)
    }

    /// Get vulnix version for metadata
    pub async fn get_vulnix_version() -> Result<String> {
        let output = Command::new("vulnix")
            .arg("--version")
            .output()
            .await
            .context("Failed to get vulnix version")?;

        if output.status.success() {
            let version_output = String::from_utf8_lossy(&output.stdout);
            // Parse version from output like "vulnix 1.10.1"
            let version = version_output
                .split_whitespace()
                .last()
                .unwrap_or("unknown")
                .to_string();

            debug!("📋 vulnix version: {}", version);
            Ok(version)
        } else {
            warn!("⚠️  Could not determine vulnix version");
            Ok("unknown".to_string())
        }
    }

    /// Check if vulnix is available on the system
    pub async fn check_vulnix_available() -> bool {
        match Command::new("vulnix").arg("--version").output().await {
            Ok(output) => {
                let available = output.status.success();
                if available {
                    debug!("✅ vulnix is available");
                } else {
                    warn!("⚠️  vulnix command failed");
                }
                available
            }
            Err(e) => {
                warn!("⚠️  vulnix not found: {}", e);
                false
            }
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
                    // Not a symlink, check if it's a regular file/directory in Nix store context
                    debug!("🤔 Treating as potential Nix path: {}", path);
                    true // Let vulnix decide if it's valid
                }
            }
        }
    }
}

/// Convenience functions for common scan scenarios
impl VulnixRunner {
    /// Scan a derivation path with automatic version detection
    pub async fn scan_derivation(
        derivation_path: &str,
        evaluation_target: i32,
    ) -> Result<VulnixScanResult> {
        let version = Self::get_vulnix_version().await.ok();
        Self::scan_path(derivation_path, evaluation_target, version).await
    }

    /// Scan current system with automatic version detection
    pub async fn scan_system(evaluation_target: i32) -> Result<VulnixScanResult> {
        let version = Self::get_vulnix_version().await.ok();
        Self::scan_current_system(evaluation_target, version).await
    }

    /// Scan GC roots with automatic version detection
    pub async fn scan_gc_roots_auto(evaluation_target: i32) -> Result<VulnixScanResult> {
        let version = Self::get_vulnix_version().await.ok();
        Self::scan_gc_roots(evaluation_target, version).await
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

        // Note: These tests use nonexistent paths, so they'll fail validation
        // In real usage, you'd test with actual Nix store paths
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
        // For now, just ensure the function signature is correct
    }
}
