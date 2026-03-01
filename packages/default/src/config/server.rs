use serde::Deserialize;

/// Configuration for the server itself.
///
/// This section is loaded from `[server]` in `config.toml`.
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,

    /// Number of worker threads for nix-eval-jobs parallel evaluation.
    /// Default: 2 (conservative to avoid hosing the system)
    #[serde(default = "default_eval_workers")]
    pub eval_workers: usize,

    /// Maximum memory size per worker in MB for nix-eval-jobs.
    /// Total eval memory = eval_workers × eval_max_memory_mb
    /// Default: 4096 MB (4 GB) per worker
    #[serde(default = "default_eval_max_memory_mb")]
    pub eval_max_memory_mb: usize,

    /// Whether to check cache status during evaluation.
    /// Adds --check-cache-status flag to nix-eval-jobs.
    /// Default: true
    #[serde(default = "default_eval_check_cache")]
    pub eval_check_cache: bool,

    /// Authentication mode: "dev" or "oidc"
    /// Default: "oidc" (read from AUTH_MODE env var)
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,

    /// Whether to allow new user registration (for local auth mode).
    /// When false, only the initial admin can be registered (when no users exist).
    /// Default: false
    #[serde(default)]
    pub allow_registration: bool,

    /// Maximum total log size stored per build job in MB.
    /// Default: 10 MB.
    #[serde(default = "default_max_build_log_size_mb")]
    pub max_build_log_size_mb: usize,

    /// Maximum size per append logs request payload in MB.
    /// Default: 1 MB.
    #[serde(default = "default_max_build_log_chunk_mb")]
    pub max_build_log_chunk_mb: usize,

    /// Retention period for successful build job logs in days.
    /// Older logs are cleared by background retention task.
    /// Default: 30 days.
    #[serde(default = "default_build_log_retention_days")]
    pub build_log_retention_days: i32,

    /// Retention period for failed build job logs in days.
    /// Older logs are cleared by background retention task.
    /// Default: 90 days.
    #[serde(default = "default_failed_build_log_retention_days")]
    pub failed_build_log_retention_days: i32,
}

// Default value functions for serde
fn default_eval_workers() -> usize {
    2 // Conservative: don't hose the system by default
}

fn default_eval_max_memory_mb() -> usize {
    4096 // 4 GB per worker
}

fn default_eval_check_cache() -> bool {
    true // Usually helpful for build planning
}

fn default_auth_mode() -> String {
    std::env::var("AUTH_MODE").unwrap_or_else(|_| "oidc".to_string())
}

fn default_max_build_log_size_mb() -> usize {
    10
}

fn default_max_build_log_chunk_mb() -> usize {
    1
}

fn default_build_log_retention_days() -> i32 {
    30
}

fn default_failed_build_log_retention_days() -> i32 {
    90
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            eval_workers: default_eval_workers(),
            eval_max_memory_mb: default_eval_max_memory_mb(),
            eval_check_cache: default_eval_check_cache(),
            auth_mode: default_auth_mode(),
            allow_registration: false,
            max_build_log_size_mb: default_max_build_log_size_mb(),
            max_build_log_chunk_mb: default_max_build_log_chunk_mb(),
            build_log_retention_days: default_build_log_retention_days(),
            failed_build_log_retention_days: default_failed_build_log_retention_days(),
        }
    }
}

impl ServerConfig {
    /// Returns the full socket address to bind to.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Get arguments for nix-eval-jobs based on config.
    pub fn nix_eval_jobs_args(&self) -> Vec<String> {
        let mut args = vec![
            "--workers".to_string(),
            self.eval_workers.to_string(),
            "--max-memory-size".to_string(),
            self.eval_max_memory_mb.to_string(),
        ];

        if self.eval_check_cache {
            args.push("--check-cache-status".to_string());
        }

        args
    }

    /// Validate configuration and warn about potential issues.
    pub fn validate(&self) -> Result<(), String> {
        // Check for excessive memory allocation
        let total_eval_memory_mb = self.eval_workers * self.eval_max_memory_mb;
        if total_eval_memory_mb > 32768 {
            // 32 GB
            return Err(format!(
                "Evaluation memory too high: {} workers × {} MB = {} MB total ({}GB). \
                 This may exhaust system memory.",
                self.eval_workers,
                self.eval_max_memory_mb,
                total_eval_memory_mb,
                total_eval_memory_mb / 1024
            ));
        }

        // Warn if eval workers seems excessive
        if self.eval_workers > 16 {
            eprintln!(
                "⚠️  Warning: {} evaluation workers is very high. \
                 Consider reducing to 4-8 for most systems.",
                self.eval_workers
            );
        }

        if self.max_build_log_chunk_mb == 0 {
            return Err("max_build_log_chunk_mb must be greater than 0".to_string());
        }

        if self.max_build_log_size_mb < self.max_build_log_chunk_mb {
            return Err(format!(
                "max_build_log_size_mb ({}) must be >= max_build_log_chunk_mb ({})",
                self.max_build_log_size_mb, self.max_build_log_chunk_mb
            ));
        }

        if self.build_log_retention_days <= 0 || self.failed_build_log_retention_days <= 0 {
            return Err("build log retention days must be greater than 0".to_string());
        }

        Ok(())
    }
}
