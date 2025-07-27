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
    /// Interval in seconds between checking for new build jobs
    #[serde(with = "humantime_serde")]
    pub poll_interval: Duration,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            cores: 1,
            max_jobs: 1,
            use_substitutes: true,
            offline: false,
            poll_interval: Duration::from_secs(300), // 5 minutes
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
}
