use serde::Deserialize;

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
    /// Interval in seconds between checking for new build jobs
    pub poll_interval: u64,
}

impl Default for BuildConfig {
    pub fn default() -> Self {
        Self {
            cores: 1,
            max_jobs: 1,
            use_substitutes: true,
            offline: false,
            poll_interval: 300, // 5 minutes
        }
    }
}

impl BuildConfig {
    /// Apply build configuration to a nix command
    pub fn apply_to_command(&self, cmd: &mut tokio::process::Command) {
        cmd.args([
            "--cores",
            &self.cores.to_string(),
            "--max-jobs",
            &self.max_jobs.to_string(),
        ]);

        if !self.use_substitutes {
            cmd.arg("--no-substitute");
        }

        if self.offline {
            cmd.arg("--offline");
        }
    }

    /// Get the poll interval as a Duration
    pub fn poll_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.poll_interval)
    }
}
