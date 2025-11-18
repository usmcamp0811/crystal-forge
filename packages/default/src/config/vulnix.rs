use serde::Deserialize;
use std::time::Duration;
use tracing::warn;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VulnixConfig {
    #[serde(with = "humantime_serde")]
    pub timeout: Duration,
    pub max_retries: u32,
    pub enable_whitelist: bool,
    pub extra_args: Vec<String>,
    pub whitelist_path: Option<String>,
    /// Interval in seconds between checking for new build jobs
    #[serde(with = "humantime_serde")]
    pub poll_interval: Duration,
}

impl Default for VulnixConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_retries: 5,
            enable_whitelist: false,
            extra_args: vec![],
            whitelist_path: None,
            poll_interval: Duration::from_secs(60),
        }
    }
}

impl VulnixConfig {
    /// Get timeout in seconds for compatibility/logging
    pub fn timeout_seconds(&self) -> u64 {
        self.timeout.as_secs()
    }
    /// Get vulnix command args
    pub fn get_vulnix_args(&self) -> Vec<String> {
        let mut args = self.extra_args.clone();

        // Only add whitelist if enabled and path exists
        if self.enable_whitelist {
            if let Some(whitelist_path) = &self.whitelist_path {
                if std::path::Path::new(whitelist_path).exists() {
                    args.extend_from_slice(&["--whitelist".to_string(), whitelist_path.clone()]);
                } else {
                    warn!(
                        "Warning: Whitelist enabled but file {} not found",
                        whitelist_path
                    );
                }
            }
        }

        args
    }
}
