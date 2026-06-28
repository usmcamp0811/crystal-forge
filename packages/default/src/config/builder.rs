use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// Builder-specific configuration for API-based multi-builder mode
#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct BuilderConfig {
    /// Builder UUID (must match a builder registered in the server)
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

    /// Initial delay between builder-ID resolution retries when the server
    /// rejects the builder (e.g. the public key has not been registered yet or
    /// the builder is disabled). The delay grows exponentially up to
    /// `resolve_retry_max_interval`.
    #[serde(with = "super::duration_serde")]
    pub resolve_retry_interval: Duration,

    /// Maximum delay between builder-ID resolution retries.
    #[serde(with = "super::duration_serde")]
    pub resolve_retry_max_interval: Duration,

    /// Maximum number of builder-ID resolution attempts before the process
    /// gives up and exits. `0` means retry forever (recommended) so a 401 at
    /// startup never crashes the service or blocks a NixOS switch — it just
    /// keeps logging until an admin registers/enables the builder.
    pub resolve_max_attempts: u32,
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
            resolve_retry_interval: Duration::from_secs(10),
            resolve_retry_max_interval: Duration::from_secs(300),
            resolve_max_attempts: 0, // retry forever by default
        }
    }
}

impl BuilderConfig {
    /// Check if API mode is configured and ready.
    ///
    /// Requires a private key path and server URL. builder_id is NOT required
    /// here — it is resolved dynamically from the server via the public key on
    /// first connection. enable_api_mode is also not required: if the private
    /// key path and server URL are set (via env or config) API mode is used.
    pub fn is_api_mode_ready(&self) -> bool {
        self.private_key_path.is_some() && self.server_url.is_some()
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
