use anyhow::{Context, Result};
use config::Config;
use serde::Deserialize;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::collections::HashMap;
use std::env;
use tokio_postgres::NoTls;

#[derive(Debug, Deserialize)]
pub struct CrystalForgeConfig {
    pub flakes: Option<FlakeConfig>,
    pub database: Option<DatabaseConfig>,
    pub server: Option<ServerConfig>,
    pub client: Option<AgentConfig>,
    pub environments: Option<EnvironmentConfig>,
}

impl CrystalForgeConfig {
    pub fn load() -> Result<Self> {
        let config_path = env::var("CRYSTAL_FORGE_CONFIG")
            .unwrap_or_else(|_| "/var/lib/crystal_forge/config.toml".to_string());

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

    pub fn with_environments(mut self, environments: EnvironmentConfig) -> Self {
        self.environments = Some(environments);
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
}

#[derive(Debug, Deserialize)]
pub struct FlakeConfig {
    pub watched: HashMap<String, String>,
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
    pub authorized_keys: HashMap<String, String>,
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
