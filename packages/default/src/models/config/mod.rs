mod agent;
mod build;
mod cache;
mod database;
mod environment;
mod flakes;
mod server;
mod system;
mod vulnix;

pub use agent::*;
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
use tokio_postgres::NoTls;
use tracing::{debug, info};

#[derive(Debug, Deserialize, Clone)]
pub struct CrystalForgeConfig {
    pub flakes: Option<FlakeConfig>,
    pub database: Option<DatabaseConfig>,
    pub server: Option<ServerConfig>,
    pub client: Option<AgentConfig>,
    pub environments: Option<Vec<EnvironmentConfig>>,
    pub systems: Option<Vec<SystemConfig>>,
    pub vulnix: Option<VulnixConfig>,
    pub build: Option<BuildConfig>,
    pub cache: Option<CacheConfig>,
}

impl CrystalForgeConfig {
    pub fn default() -> Self {
        Self {
            flakes: Some(FlakeConfig::default()),
            database: Some(DatabaseConfig::default()),
            server: Some(ServerConfig::default()),
            client: Some(AgentConfig::default()),
            environments: Some(vec![]),
            systems: Some(vec![]),
            vulnix: Some(VulnixConfig::default()),
            build: Some(BuildConfig::default()),
            cache: Some(CacheConfig::default()),
        }
    }
    /// Gets build config, returning owned value with defaults if missing
    pub fn get_build_config(&self) -> BuildConfig {
        self.build.clone().unwrap_or_default()
    }

    /// Gets vulnix config, returning owned value with defaults if missing  
    pub fn get_vulnix_config(&self) -> VulnixConfig {
        self.vulnix.clone().unwrap_or_default()
    }

    /// Gets cache config, returning owned value with defaults if missing
    pub fn get_cache_config(&self) -> CacheConfig {
        self.cache.clone().unwrap_or_default()
    }

    /// Gets build config as reference (for when you need to avoid cloning)
    pub fn build_config_ref(&self) -> Cow<'_, BuildConfig> {
        match &self.build {
            Some(config) => Cow::Borrowed(config),
            None => Cow::Owned(BuildConfig::default()),
        }
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

        settings
            .try_deserialize::<Self>()
            .context("parsing configuration")
    }

    pub async fn db_pool() -> Result<PgPool> {
        let cfg = Self::load()?;
        let db_url = cfg
            .database
            .as_ref()
            .context("missing [database] section in configuration")?
            .to_url();

        PgPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .context("connecting to database")
    }

    pub fn with_flakes(mut self, flakes: FlakeConfig) -> Self {
        self.flakes = Some(flakes);
        self
    }

    pub fn with_database(mut self, database: DatabaseConfig) -> Self {
        self.database = Some(database);
        self
    }

    pub fn with_server(mut self, server: ServerConfig) -> Self {
        self.server = Some(server);
        self
    }

    pub fn with_client(mut self, client: AgentConfig) -> Self {
        self.client = Some(client);
        self
    }

    pub fn with_environments<T>(mut self, environments: T) -> Self
    where
        T: Into<Vec<EnvironmentConfig>>,
    {
        self.environments = Some(environments.into());
        self
    }

    pub async fn validate_db_connection() -> anyhow::Result<()> {
        let cfg = Self::load()?;
        let db_url = cfg
            .database
            .as_ref()
            .context("missing [database] section in configuration")?
            .to_url();
        let (_client, connection) = tokio_postgres::connect(&db_url, NoTls).await?;
        tokio::spawn(connection);
        Ok(())
    }
    pub async fn sync_systems_to_db(&self, pool: &PgPool) -> anyhow::Result<()> {
        let cfg = Self::load()?;
        let systems = match &self.systems {
            Some(s) => s,
            None => {
                let config_path = env::var("CRYSTAL_FORGE_CONFIG")
                    .unwrap_or_else(|_| "/var/lib/crystal_forge/config.toml".to_string());
                tracing::info!(
                    "No systems defined in {}; skipping system sync.",
                    config_path
                );
                return Ok(());
            }
        };
        debug!("💡 Syncing Systems in Config to Database.");
        if let Some(environments) = &cfg.environments {
            for environment in environments {
                let _ = get_or_insert_environment_id_by_config(pool, environment).await?;
            }
        }

        for config in systems {
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
                    .as_ref()
                    .and_then(|f| f.watched.iter().find(|wf| &wf.name == flake_name))
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
