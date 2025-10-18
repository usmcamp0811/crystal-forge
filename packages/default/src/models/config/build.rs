use serde::Deserialize;
use std::time::Duration;

/// Configuration for nix build resource limits and behavior
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BuildConfig {
    /// Maximum CPU cores to use per build job
    pub cores: u32,
    /// Maximum number of concurrent build jobs
    pub max_jobs: u32,
    /// Whether to use binary substitutes/caches
    pub use_substitutes: bool,
    /// Build in offline mode (no network access)
    pub offline: bool,
    /// Interval between checking for new build jobs
    #[serde(with = "humantime_serde")]
    pub poll_interval: Duration,
    /// Maximum time a build can be silent before timing out
    #[serde(with = "humantime_serde")]
    pub max_silent_time: Duration,
    /// Maximum total time for a build before timing out
    #[serde(with = "humantime_serde")]
    pub timeout: Duration,
    /// Enable sandbox for builds
    pub sandbox: bool,

    // Systemd resource controls
    /// Memory limit for systemd scope (e.g., "4G", "2048M")
    pub systemd_memory_max: Option<String>,
    /// CPU quota as percentage (e.g., 300 for 3 cores worth)
    pub systemd_cpu_quota: Option<u32>,
    /// Timeout for systemd scope stop operation in seconds
    pub systemd_timeout_stop_sec: Option<u32>,
    /// Whether to use systemd-run at all (fallback to direct execution if false)
    pub use_systemd_scope: bool,
    /// Additional systemd properties to set
    pub systemd_properties: Vec<String>,

    pub max_concurrent_derivations: Option<u32>,
    pub wait_for_cache_push: Option<bool>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            cores: 1,
            max_jobs: 1,
            use_substitutes: true,
            offline: false,
            poll_interval: Duration::from_secs(300), // 5 minutes
            max_silent_time: Duration::from_secs(3600), // 1 hour
            timeout: Duration::from_secs(7200),      // 2 hours
            sandbox: true,
            max_concurrent_derivations: Some(8),

            // Systemd defaults
            systemd_memory_max: Some("4G".to_string()),
            systemd_cpu_quota: Some(300),        // 3 cores worth
            systemd_timeout_stop_sec: Some(600), // 10 minutes
            use_systemd_scope: true,
            systemd_properties: Vec::new(),
            wait_for_cache_push: Some(false),
        }
    }
}

impl BuildConfig {
    /// Apply build configuration to a nix command
    pub fn apply_to_command(&self, cmd: &mut tokio::process::Command) {
        // Resource limits
        cmd.args([
            "--cores",
            &self.cores.to_string(),
            "--max-jobs",
            &self.max_jobs.to_string(),
        ]);

        // Timeout settings
        cmd.args([
            "--option",
            "max-silent-time",
            &self.max_silent_time.as_secs().to_string(),
        ]);
        cmd.args(["--option", "timeout", &self.timeout.as_secs().to_string()]);

        // Sandbox setting
        cmd.args(["--option", "sandbox", &self.sandbox.to_string()]);

        // Substitute settings
        if !self.use_substitutes {
            cmd.arg("--no-substitute");
        }

        // Offline mode
        if self.offline {
            cmd.arg("--offline");
        }
    }

    /// Create a systemd-run command with configured resource limits
    pub fn systemd_scoped_cmd_base(&self, derivation_id: i32) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new("systemd-run");

        // Base systemd-run arguments with unique unit name
        let unit_name = format!("crystal-forge-build-{}.scope", derivation_id);
        cmd.args([
            "--scope",
            "--collect",
            "--quiet",
            "--slice=crystal-forge-builds.slice", // Put under parent slice
            &format!("--unit={}", unit_name),     // Unique name per build
        ]);

        // Memory limit
        if let Some(ref memory_max) = self.systemd_memory_max {
            cmd.args(["--property", &format!("MemoryMax={}", memory_max)]);
        }

        // CPU quota
        if let Some(cpu_quota) = self.systemd_cpu_quota {
            cmd.args(["--property", &format!("CPUQuota={}%", cpu_quota)]);
        }

        // Timeout for stopping the scope
        if let Some(timeout_stop) = self.systemd_timeout_stop_sec {
            cmd.args(["--property", &format!("TimeoutStopSec={}", timeout_stop)]);
        }

        // Additional custom properties
        for property in &self.systemd_properties {
            cmd.args(["--property", property]);
        }

        // Add the actual command to run
        cmd.args(["--", "nix", "build"]);
        cmd
    }

    /// Get timeout for build process (use the shorter of the two timeouts)
    pub fn process_timeout(&self) -> Duration {
        // Add some buffer time for process cleanup
        self.timeout + Duration::from_secs(60)
    }

    /// Get timeout in seconds for max-silent-time
    pub fn max_silent_time_seconds(&self) -> u64 {
        self.max_silent_time.as_secs()
    }

    /// Get timeout in seconds for total timeout
    pub fn timeout_seconds(&self) -> u64 {
        self.timeout.as_secs()
    }

    /// Check if systemd should be used for this build
    pub fn should_use_systemd(&self) -> bool {
        self.use_systemd_scope
    }
}
