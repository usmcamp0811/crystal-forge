use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// Builder-specific configuration for API-based multi-builder mode
#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct BuilderConfig {
    /// Optional builder UUID. If omitted in API mode, the builder resolves its
    /// server-assigned ID from its local public key.
    pub builder_id: Option<Uuid>,

    /// Path to Ed25519 private key file for authentication
    pub private_key_path: Option<PathBuf>,

    /// Server API base URL (e.g., "https://crystal-forge.example.com")
    pub server_url: Option<String>,

    /// How often to poll for new jobs (in seconds)
    #[serde(with = "super::duration_serde")]
    pub poll_interval: Duration,

    /// How often to send heartbeat and metrics (in seconds)
    #[serde(with = "super::duration_serde")]
    pub heartbeat_interval: Duration,

    /// Maximum concurrent jobs this builder will execute
    /// (overrides the server-side setting if lower)
    pub max_concurrent_jobs: Option<i32>,

    /// Enable API mode (if false, use legacy direct-database mode)
    pub enable_api_mode: bool,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self {
            builder_id: None,
            private_key_path: None,
            server_url: None,
            poll_interval: Duration::from_secs(5),
            heartbeat_interval: Duration::from_secs(30),
            max_concurrent_jobs: None,
            enable_api_mode: false, // Default to legacy mode for backward compatibility
        }
    }
}

impl BuilderConfig {
    /// Check if API mode is configured and enabled
    pub fn is_api_mode_ready(&self) -> bool {
        self.enable_api_mode && self.private_key_path.is_some() && self.server_url.is_some()
    }

    /// Get the builder ID, or error if not configured
    pub fn require_builder_id(&self) -> anyhow::Result<Uuid> {
        self.builder_id
            .ok_or_else(|| anyhow::anyhow!("builder.builder_id not configured"))
    }

    /// Get the private key path, or error if not configured
    pub fn require_private_key_path(&self) -> anyhow::Result<PathBuf> {
        self.private_key_path
            .clone()
            .ok_or_else(|| anyhow::anyhow!("builder.private_key_path not configured"))
    }

    /// Get the server URL, or error if not configured
    pub fn require_server_url(&self) -> anyhow::Result<String> {
        self.server_url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("builder.server_url not configured"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_mode_ready_does_not_require_local_builder_id() {
        let config = BuilderConfig {
            enable_api_mode: true,
            private_key_path: Some(PathBuf::from("/var/lib/crystal-forge/builder-api.key")),
            server_url: Some("https://crystal-forge.example.com".to_string()),
            ..BuilderConfig::default()
        };

        assert!(config.is_api_mode_ready());
    }
}
