use serde::Deserialize;
use std::time::Duration;

/// Configuration for nix build resource limits and behavior
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BuildConfig {
    /// Maximum CPU cores to use per build job
    pub cores: u32,
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

    /// Maximum number of concurrent nix-store --realise processes.
    /// This is how many builds Crystal Forge runs in parallel.
    /// Default: 1 (conservative - one build at a time)
    #[serde(default = "default_max_concurrent_derivations")]
    pub max_concurrent_derivations: usize,

    /// Passed as --max-jobs to Nix.
    /// Number of parallel derivations within each nix-store process.
    /// NOT cores per build - it's parallel derivations.
    /// Default: 1 (sequential builds within each process)
    #[serde(default = "default_max_jobs")]
    pub max_jobs: usize,

    /// Passed as --cores to Nix.
    /// Number of CPU cores each derivation can use.
    /// Special value 0 means unrestricted (Nix will use all cores).
    /// Default: 0 (let single builds use all cores)
    #[serde(default = "default_cores_per_job")]
    pub cores_per_job: usize,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            cores: 1,
            use_substitutes: true,
            offline: false,
            poll_interval: Duration::from_secs(300), // 5 minutes
            max_silent_time: Duration::from_secs(3600), // 1 hour
            timeout: Duration::from_secs(7200),      // 2 hours
            sandbox: true,
            max_concurrent_derivations: default_max_concurrent_derivations(),
            max_jobs: default_max_jobs(),
            cores_per_job: default_cores_per_job(),

            // Systemd defaults
            systemd_memory_max: Some("4G".to_string()),
            systemd_cpu_quota: Some(300),        // 3 cores worth
            systemd_timeout_stop_sec: Some(600), // 10 minutes
            use_systemd_scope: true,
            systemd_properties: Vec::new(),
        }
    }
}

// Build defaults
fn default_max_concurrent_derivations() -> usize {
    1 // Very conservative: one build at a time
}

fn default_max_jobs() -> usize {
    1 // Sequential derivations within that build
}

fn default_cores_per_job() -> usize {
    0 // Unrestricted - let single build use all cores
}

impl BuildConfig {
    /// Apply build configuration to a nix command
    pub fn apply_to_command(&self, cmd: &mut tokio::process::Command) {
        // Resource limits - use new config fields
        cmd.args([
            "--cores",
            &self.cores_per_job.to_string(),
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

    /// Get the Nix build arguments based on configuration.
    pub fn nix_build_args(&self) -> Vec<String> {
        vec![
            "--max-jobs".to_string(),
            self.max_jobs.to_string(),
            "--cores".to_string(),
            self.cores_per_job.to_string(),
        ]
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

    /// Validate configuration and warn about potential issues.
    pub fn validate(&self) -> Result<(), String> {
        // Try to get CPU count
        let cpu_count = num_cpus::get();

        // Calculate potential max CPU usage (if cores_per_job > 0)
        if self.cores_per_job > 0 {
            let max_cpu_usage =
                self.max_concurrent_derivations * self.max_jobs * self.cores_per_job;

            // Error if we'd use more than 2× available cores
            if max_cpu_usage > cpu_count * 2 {
                return Err(format!(
                    "Build configuration would severely oversubscribe CPUs:\n\
                     Calculated max CPU usage: {} cores\n\
                     System CPUs: {} cores\n\
                     \n\
                     Current settings:\n\
                       max_concurrent_derivations = {}\n\
                       max_jobs = {}\n\
                       cores_per_job = {}\n\
                     \n\
                     Formula: {} × {} × {} = {} cores\n\
                     \n\
                     Recommendation: Reduce one of these values.",
                    max_cpu_usage,
                    cpu_count,
                    self.max_concurrent_derivations,
                    self.max_jobs,
                    self.cores_per_job,
                    self.max_concurrent_derivations,
                    self.max_jobs,
                    self.cores_per_job,
                    max_cpu_usage
                ));
            }

            // Warn if using more cores than available
            if max_cpu_usage > cpu_count {
                eprintln!(
                    "⚠️  Warning: Build config may oversubscribe CPUs:\n\
                     Max usage: {} cores (system has {})\n\
                     This is OK if builds are I/O bound, but may cause slowdown.",
                    max_cpu_usage, cpu_count
                );
            }
        } else {
            // cores_per_job = 0 means unrestricted
            if self.max_concurrent_derivations > 1 {
                eprintln!(
                    "⚠️  Warning: cores_per_job = 0 with {} concurrent derivations.\n\
                     Each build can use ALL {} cores, leading to oversubscription.\n\
                     Consider setting cores_per_job = {} to limit per-build usage.",
                    self.max_concurrent_derivations,
                    cpu_count,
                    cpu_count / self.max_concurrent_derivations
                );
            }
        }

        Ok(())
    }

    /// Get a human-readable summary of the build configuration.
    pub fn summary(&self) -> String {
        let cpu_count = num_cpus::get();
        let cores_desc = if self.cores_per_job == 0 {
            format!("unrestricted (up to {} per build)", cpu_count)
        } else {
            format!("{} per derivation", self.cores_per_job)
        };

        let max_parallel = self.max_concurrent_derivations * self.max_jobs;

        format!(
            "Build Configuration:\n\
             - Concurrent builds: {}\n\
             - Parallel derivations per build: {}\n\
             - Cores per derivation: {}\n\
             - Max parallel derivations: {}\n\
             - System CPUs: {}",
            self.max_concurrent_derivations, self.max_jobs, cores_desc, max_parallel, cpu_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_defaults() {
        let build = BuildConfig::default();
        assert_eq!(build.max_concurrent_derivations, 1); // Very conservative
        assert_eq!(build.max_jobs, 1);
        assert_eq!(build.cores_per_job, 0); // Unrestricted for single build
        assert!(build.validate().is_ok());
    }

    #[test]
    fn test_validation_catches_oversubscription() {
        let build = BuildConfig {
            max_concurrent_derivations: 8,
            max_jobs: 4,
            cores_per_job: 4, // 8 × 4 × 4 = 128 cores!
            ..Default::default()
        };

        // Should fail validation unless system has 64+ cores
        let result = build.validate();
        if num_cpus::get() < 64 {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_nix_build_args() {
        let config = BuildConfig {
            max_jobs: 2,
            cores_per_job: 4,
            ..Default::default()
        };

        let args = config.nix_build_args();
        assert_eq!(args, vec!["--max-jobs", "2", "--cores", "4"]);
    }
}
