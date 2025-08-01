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
}
