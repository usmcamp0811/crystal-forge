//! Crystal Forge configuration loading.
//!
//! This module provides pure deserialization and loading of the Crystal Forge
//! configuration. No database connections, no server-side synchronization, and
//! no external service calls are made here.
//!
//! # Crate boundary rules
//!
//! - No `sqlx`, `axum`, `reqwest`, PostgreSQL, OIDC, or server module imports.
//! - Only `cf-protocol` is permitted as a Crystal Forge workspace dependency.
//! - Configuration loading uses TOML files and environment variables only.

mod agent;
mod auth;
mod build;
mod builder;
mod cache;
mod database;
pub mod deployment;
mod environment;
mod flakes;
mod oidc;
mod server;
mod system;
mod vulnix;

pub use agent::*;
pub use auth::*;
pub use build::*;
pub use builder::*;
pub use cache::*;
pub use database::*;
pub use deployment::*;
pub use environment::*;
pub use flakes::*;
pub use oidc::*;
pub use server::*;
pub use system::*;
pub use vulnix::*;

use anyhow::{Context, Result};
use config::Config;
use serde::Deserialize;
use std::env;
use tracing::debug;

pub(crate) mod duration_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }
}

/// Root configuration for Crystal Forge.
///
/// Loaded from a TOML file and environment variables. Database connectivity,
/// pool creation, and DB synchronization are NOT part of this struct — they
/// live in `cf-server` which has a dependency on SQLx.
#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct CrystalForgeConfig {
    #[serde(default)]
    pub flakes: FlakeConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub client: AgentConfig,
    #[serde(default)]
    pub builder: BuilderConfig,
    #[serde(default)]
    pub environments: Vec<EnvironmentConfig>,
    #[serde(default)]
    pub systems: Vec<SystemConfig>,
    #[serde(default)]
    pub vulnix: VulnixConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub deployment: DeploymentConfig,
}

impl Default for CrystalForgeConfig {
    fn default() -> Self {
        Self {
            flakes: FlakeConfig::default(),
            database: DatabaseConfig::default(),
            server: ServerConfig::default(),
            client: AgentConfig::default(),
            builder: BuilderConfig::default(),
            environments: vec![],
            systems: vec![],
            vulnix: VulnixConfig::default(),
            build: BuildConfig::default(),
            cache: CacheConfig::default(),
            auth: AuthConfig::default(),
            deployment: DeploymentConfig::default(),
        }
    }
}

impl CrystalForgeConfig {
    pub fn get_server_config(&self) -> &ServerConfig {
        &self.server
    }

    pub fn get_build_config(&self) -> &BuildConfig {
        &self.build
    }

    pub fn get_vulnix_config(&self) -> &VulnixConfig {
        &self.vulnix
    }

    pub fn get_deployment_config(&self) -> &DeploymentConfig {
        &self.deployment
    }

    pub fn get_cache_config(&self) -> &CacheConfig {
        &self.cache
    }

    pub fn build_config_ref(&self) -> &BuildConfig {
        &self.build
    }

    pub fn get_auth_config(&self) -> &AuthConfig {
        &self.auth
    }

    pub fn get_builder_config(&self) -> &BuilderConfig {
        &self.builder
    }

    /// Load configuration from TOML file and environment variables.
    ///
    /// No database connection is made by this call.
    pub fn load() -> Result<Self> {
        let config_path = env::var("CRYSTAL_FORGE_CONFIG")
            .unwrap_or_else(|_| "/var/lib/crystal_forge/config.toml".to_string());

        debug!("CRYSTAL_FORGE_CONFIG => {}", config_path);

        let settings = Config::builder()
            .add_source(config::File::with_name(&config_path).required(false))
            .add_source(config::Environment::with_prefix("CRYSTAL_FORGE").separator("__"))
            .build()
            .context("loading configuration")?;

        let config: Self = settings
            .try_deserialize()
            .context("parsing configuration")?;

        Ok(config)
    }

    pub fn with_flakes(mut self, flakes: FlakeConfig) -> Self {
        self.flakes = flakes;
        self
    }

    pub fn with_database(mut self, database: DatabaseConfig) -> Self {
        self.database = database;
        self
    }

    pub fn with_server(mut self, server: ServerConfig) -> Self {
        self.server = server;
        self
    }

    pub fn with_client(mut self, client: AgentConfig) -> Self {
        self.client = client;
        self
    }

    pub fn with_environments<T>(mut self, environments: T) -> Self
    where
        T: Into<Vec<EnvironmentConfig>>,
    {
        self.environments = environments.into();
        self
    }
}
