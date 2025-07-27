use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VulnixConfig {
    #[serde(with = "humantime_serde")]
    pub timeout: Duration,
    pub max_retries: u32,
    pub enable_whitelist: bool,
    pub extra_args: Vec<String>,
}

impl Default for VulnixConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300), // 5 minutes
            max_retries: 2,
            enable_whitelist: true,
            extra_args: vec![],
        }
    }
}

impl VulnixConfig {
    /// Get timeout in seconds for compatibility/logging
    pub fn timeout_seconds(&self) -> u64 {
        self.timeout.as_secs()
    }
}
