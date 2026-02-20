use crate::config::{CacheType, duration_serde};
use crate::models::deployment_policies::DeploymentPolicy;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Deployment strategy for activating configurations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStrategy {
    /// Create generation + activate immediately (default)
    ImmediatePersist,
    /// Create generation, activate on next boot only
    BootOnly,
}

impl Default for DeploymentStrategy {
    fn default() -> Self {
        Self::ImmediatePersist
    }
}

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

    /// Deployment strategy (immediate_persist or boot_only)
    #[serde(default)]
    pub strategy: DeploymentStrategy,
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
            strategy: DeploymentStrategy::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_strategy_default() {
        let strategy = DeploymentStrategy::default();
        assert_eq!(strategy, DeploymentStrategy::ImmediatePersist);
    }

    #[test]
    fn test_deployment_strategy_serde_immediate_persist() {
        let json = r#"{"strategy": "immediate_persist"}"#;
        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let strategy: DeploymentStrategy =
            serde_json::from_value(parsed["strategy"].clone()).unwrap();
        assert_eq!(strategy, DeploymentStrategy::ImmediatePersist);
    }

    #[test]
    fn test_deployment_strategy_serde_boot_only() {
        let json = r#"{"strategy": "boot_only"}"#;
        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let strategy: DeploymentStrategy =
            serde_json::from_value(parsed["strategy"].clone()).unwrap();
        assert_eq!(strategy, DeploymentStrategy::BootOnly);
    }

    #[test]
    fn test_deployment_config_default_strategy() {
        let config = DeploymentConfig::default();
        assert_eq!(config.strategy, DeploymentStrategy::ImmediatePersist);
    }
}
