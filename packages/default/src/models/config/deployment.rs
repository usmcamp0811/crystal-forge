use crate::models::config::{CacheType, duration_serde};
use crate::models::deployment_policies::DeploymentPolicy;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for deployment operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    #[serde(skip)] // Don't serialize the key
    pub server_public_key: Option<VerifyingKey>,
    pub max_deployment_age_minutes: u64,
    pub dry_run_first: bool,
    pub fallback_to_local_build: bool,
    pub deployment_timeout_minutes: u64,
    pub cache_url: Option<String>,
    pub cache_public_key: Option<String>,
    #[serde(with = "duration_serde")]
    pub deployment_poll_interval: Duration,

    /// Deployment policies that systems must satisfy
    #[serde(default)]
    pub policies: Vec<DeploymentPolicy>,
    pub require_sigs: bool,

    /// Cache type (Attic, S3, Nix, Http)
    #[serde(default)]
    pub cache_type: CacheType,
    /// Attic cache name (used when cache_type is Attic)
    pub attic_cache_name: Option<String>,
}

impl Default for DeploymentConfig {
    fn default() -> Self {
        Self {
            server_public_key: None,
            max_deployment_age_minutes: 30,
            dry_run_first: true,
            fallback_to_local_build: false,
            deployment_timeout_minutes: 60,
            cache_url: None,
            cache_public_key: None,
            deployment_poll_interval: Duration::from_secs(60),
            policies: vec![
                // Default: require CF agent
                DeploymentPolicy::RequireCrystalForgeAgent { strict: false },
            ],
            require_sigs: true,
            cache_type: CacheType::Nix,
            attic_cache_name: None,
        }
    }
}
