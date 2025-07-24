use serde::Deserialize;

/// Configuration for nix build resource limits and behavior
#[derive(Debug, Deserialize)]
pub struct BuildConfig {
    /// Maximum CPU cores to use per build job
    #[serde(default = "BuildConfig::default_cores")]
    pub cores: u32,

    /// Maximum number of concurrent build jobs
    #[serde(default = "BuildConfig::default_max_jobs")]
    pub max_jobs: u32,

    /// Whether to use binary substitutes/caches
    #[serde(default = "BuildConfig::default_use_substitutes")]
    pub use_substitutes: bool,

    /// Build in offline mode (no network access)
    #[serde(default)]
    pub offline: bool,
}

impl BuildConfig {
    fn default_cores() -> u32 {
        1
    }
    fn default_max_jobs() -> u32 {
        1
    }
    fn default_use_substitutes() -> bool {
        true
    }

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

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            cores: 1,
            max_jobs: 1,
            use_substitutes: true,
            offline: false,
        }
    }
}
