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
use std::collections::HashMap;
use std::env;
use tokio_postgres::NoTls;
use tracing::{debug, info};

#[derive(Debug, Deserialize)]
pub struct CrystalForgeConfig {
    pub flakes: Option<FlakeConfig>,
    pub database: Option<DatabaseConfig>,
    pub server: Option<ServerConfig>,
    pub client: Option<AgentConfig>,
    pub environments: Option<Vec<EnvironmentConfig>>,
    pub systems: Option<Vec<SystemConfig>>,
}

impl CrystalForgeConfig {
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

#[derive(Debug, Deserialize)]
pub struct FlakeConfig {
    pub watched: Vec<WatchedFlake>,
}

#[derive(Debug, Deserialize)]
pub struct WatchedFlake {
    pub name: String,
    pub repo_url: String,
}

/// PostgreSQL database connection configuration.
///
/// This section is loaded from `[database]` in `config.toml`.
#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    #[serde(default = "default_pg_port")]
    pub port: u16,
    pub user: String,
    pub password: String,
    pub name: String,
}

fn default_pg_port() -> u16 {
    5432
}

impl DatabaseConfig {
    /// Returns a PostgreSQL connection string.
    pub fn to_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.name
        )
    }
}

/// Configuration for the server itself.
///
/// This section is loaded from `[server]` in `config.toml`.
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl ServerConfig {
    /// Returns the full socket address to bind to.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    pub server_host: String,
    pub server_port: u16,
    pub private_key: String,
}

impl AgentConfig {
    /// Returns the full HTTP URL to the configured server.
    pub fn endpoint(&self) -> String {
        format!("http://{}:{}", self.server_host, self.server_port)
    }
}

#[derive(Debug, Deserialize)]
pub struct EnvironmentConfig {
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub risk_profile: String,
    pub compliance_level: String,
}

#[derive(Debug, Deserialize)]
pub struct SystemConfig {
    pub hostname: String,
    pub public_key: String,
    pub environment: String,
    pub flake_name: Option<String>, // just the flake name reference
}
