use serde::Deserialize;

/// Configuration for the server itself.
///
/// This section is loaded from `[server]` in `config.toml`.
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,

    /// Number of worker threads for nix-eval-jobs parallel evaluation
    #[serde(default = "default_eval_workers")]
    pub eval_workers: usize,

    /// Maximum memory size per worker in MB for nix-eval-jobs
    #[serde(default = "default_eval_max_memory_mb")]
    pub eval_max_memory_mb: usize,

    /// Whether to check cache status during evaluation
    #[serde(default = "default_eval_check_cache")]
    pub eval_check_cache: bool,
}

// Default value functions for serde
fn default_eval_workers() -> usize {
    4
}

fn default_eval_max_memory_mb() -> usize {
    4096
}

fn default_eval_check_cache() -> bool {
    true
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            eval_workers: default_eval_workers(),
            eval_max_memory_mb: default_eval_max_memory_mb(),
            eval_check_cache: default_eval_check_cache(),
        }
    }
}

impl ServerConfig {
    /// Returns the full socket address to bind to.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
    pub fn get_eval_workers(&self) -> usize {
        if self.eval_workers == 0 {
            num_cpus::get()
        } else {
            self.eval_workers
        }
    }
}
