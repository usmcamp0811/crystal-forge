use crate::models::config::duration_serde;
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
    #[serde(with = "duration_serde")]
    pub deployment_poll_interval: Duration,
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
            deployment_poll_interval: Duration::from_secs(15 * 60),
        }
    }
}
