mod agent;
mod auth;
mod build;
mod cache;
mod database;
mod environment;
mod flakes;
mod server;
mod system;
mod vulnix;

pub use agent::*;
pub use auth::*;
pub use build::*;
pub use cache::*;
pub use database::*;
pub use environment::*;
pub use flakes::*;
pub use server::*;
pub use system::*;
pub use vulnix::*;

use crate::models::systems::System;
use crate::queries::environments::{
    get_environment_id_by_name, get_or_insert_environment_id_by_config,
};
use crate::queries::flakes::{get_flake_id_by_repo_url, insert_flake};
use crate::queries::systems::insert_system;
use anyhow::{Context, Result};
use config::Config;
use serde::Deserialize;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::time::Duration;
use tokio_postgres::NoTls;
use tracing::{debug, info};

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
}

impl Default for CrystalForgeConfig {
    fn default() -> Self {
        Self {
            flakes: FlakeConfig::default(),
            database: DatabaseConfig::default(),
            server: ServerConfig::default(),
            client: AgentConfig::default(),
            environments: vec![],
            systems: vec![],
            vulnix: VulnixConfig::default(),
            build: BuildConfig::default(),
            cache: CacheConfig::default(),
            auth: AuthConfig::default(),
        }
    }
}

impl CrystalForgeConfig {
    /// Gets build config as reference
    pub fn get_build_config(&self) -> &BuildConfig {
        &self.build
    }

    /// Gets vulnix config as reference
    pub fn get_vulnix_config(&self) -> &VulnixConfig {
        &self.vulnix
    }

    /// Gets cache config as reference
    pub fn get_cache_config(&self) -> &CacheConfig {
        &self.cache
    }

    /// Gets build config as reference (legacy method, same as get_build_config)
    pub fn build_config_ref(&self) -> &BuildConfig {
        &self.build
    }

    pub fn get_auth_config(&self) -> &AuthConfig {
        &self.auth
    }

    pub fn load() -> Result<Self> {
        let config_path = env::var("CRYSTAL_FORGE_CONFIG")
            .unwrap_or_else(|_| "/var/lib/crystal_forge/config.toml".to_string());

        debug!("CRYSTAL_FORGE_CONFIG => {}", config_path);

        let settings = Config::builder()
            .add_source(config::File::with_name(&config_path).required(false))
            .add_source(config::Environment::with_prefix("CRYSTAL_FORGE").separator("__"))
            .build()
            .context("loading configuration")?;

        let mut config: Self = settings
            .try_deserialize()
            .context("parsing configuration")?;

        // Post-process to set branch fields
        for watched_flake in &mut config.flakes.watched {
            watched_flake.set_branch_from_url();
        }

        Ok(config)
    }

    pub async fn db_pool() -> Result<PgPool> {
        let cfg = Self::load()?;
        let db_url = cfg.database.to_url();
        PgPoolOptions::new()
            .max_connections(5)
            .min_connections(1) // Keep minimum connections alive
            .acquire_timeout(Duration::from_secs(30)) // Timeout acquiring connections
            .idle_timeout(Some(Duration::from_secs(600))) // Close idle connections after 10min
            .max_lifetime(Some(Duration::from_secs(1800))) // Rotate connections after 30min
            .test_before_acquire(true) // Test connections before use
            .connect(&db_url)
            .await
            .context("connecting to database")
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

    pub async fn validate_db_connection() -> anyhow::Result<()> {
        let cfg = Self::load()?;
        let db_url = cfg.database.to_url();
        let (_client, connection) = tokio_postgres::connect(&db_url, NoTls).await?;
        tokio::spawn(connection);
        Ok(())
    }

    pub async fn sync_systems_to_db(&self, pool: &PgPool) -> anyhow::Result<()> {
        let cfg = Self::load()?;

        if self.systems.is_empty() {
            let config_path = env::var("CRYSTAL_FORGE_CONFIG")
                .unwrap_or_else(|_| "/var/lib/crystal_forge/config.toml".to_string());
            tracing::info!(
                "No systems defined in {}; skipping system sync.",
                config_path
            );
            return Ok(());
        }

        debug!("💡 Syncing Systems in Config to Database.");

        // Sync environments first
        for environment in &cfg.environments {
            let _ = get_or_insert_environment_id_by_config(pool, environment).await?;
        }

        for config in &self.systems {
            tracing::info!("📥 Syncing system {}...", config.hostname);

            // Fetch environment ID
            let environment_id = get_environment_id_by_name(pool, &config.environment)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Environment '{}' not found in database for system '{}'",
                        config.environment,
                        config.hostname
                    )
                })?;

            // Fetch flake ID by repo URL if provided in your config (optional)
            let flake_id = if let Some(flake_name) = &config.flake_name {
                // Find watched flake in config by name
                let watched_flake = self
                    .flakes
                    .watched
                    .iter()
                    .find(|wf| &wf.name == flake_name)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Flake '{}' referenced by system '{}' not found in flakes.watched",
                            flake_name,
                            config.hostname
                        )
                    })?;

                // Lookup or insert in DB
                let id = match get_flake_id_by_repo_url(pool, &watched_flake.repo_url).await? {
                    Some(id) => id,
                    None => {
                        insert_flake(pool, &watched_flake.name, &watched_flake.repo_url)
                            .await?
                            .id
                    }
                };
                Some(id) // wrap in Some() to match Option<i32>
            } else {
                None
            };

            let system = System::new(
                pool,
                config.hostname.clone(),
                Some(environment_id),
                true,
                config.public_key.clone(),
                flake_id,
            )
            .await?;
        }

        Ok(())
    }
}
