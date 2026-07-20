use cf_protocol::builder::RemoteBuildExecutionStrategy;
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

    /// Explicit remote build strategies this builder is willing to execute.
    /// The default preserves the existing production behavior and prevents
    /// accidental source re-evaluation jobs from silently falling back.
    pub supported_execution_strategies: Vec<RemoteBuildExecutionStrategy>,

    /// Root containing local bare Git mirrors used by verified source builds.
    pub source_mirror_root: PathBuf,

    /// Root containing detached worktrees at authorized commit SHAs.
    pub source_worktree_root: PathBuf,

    /// Whether to remove detached source worktrees after the build/reporting
    /// path finishes. Defaults to true to avoid unbounded growth.
    pub cleanup_source_worktrees: bool,
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
            resolve_retry_interval: Duration::from_secs(10),
            resolve_retry_max_interval: Duration::from_secs(300),
            resolve_max_attempts: 0, // retry forever by default
            supported_execution_strategies: vec![RemoteBuildExecutionStrategy::ServerDerivation],
            source_mirror_root: PathBuf::from("/var/lib/crystal-forge/flake-mirrors"),
            source_worktree_root: PathBuf::from("/var/lib/crystal-forge/flake-worktrees"),
            cleanup_source_worktrees: true,
        }
    }
}

impl BuilderConfig {
    /// Check if API mode is configured and ready.
    ///
    /// Requires a private key path and server URL. builder_id is NOT required
    /// here — it is resolved dynamically from the server via the public key on
    /// first connection. The builder is API-only; these fields are required.
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

    pub fn supports_execution_strategy(&self, strategy: RemoteBuildExecutionStrategy) -> bool {
        self.supported_execution_strategies.contains(&strategy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_mode_ready_does_not_require_local_builder_id() {
        let config = BuilderConfig {
            private_key_path: Some(PathBuf::from("/var/lib/crystal-forge/builder-api.key")),
            server_url: Some("https://crystal-forge.example.com".to_string()),
            ..BuilderConfig::default()
        };

        assert!(config.is_api_mode_ready());
    }

    #[test]
    fn default_builder_supports_only_server_derivation() {
        let config = BuilderConfig::default();

        assert!(config.supports_execution_strategy(RemoteBuildExecutionStrategy::ServerDerivation));
        assert!(
            !config.supports_execution_strategy(
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified
            )
        );
    }
}
