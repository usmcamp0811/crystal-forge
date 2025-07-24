use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct VulnixConfig {
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub enable_whitelist: bool,
    pub extra_args: Vec<String>,
}

impl Default for VulnixConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 300,
            max_retries: 2,
            enable_whitelist: true,
            extra_args: vec![],
        }
    }
}
