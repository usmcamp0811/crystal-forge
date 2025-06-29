use anyhow::{Context, Result};
use config::Config;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use tokio_postgres::NoTls;
use tracing::debug;

#[derive(Debug, Deserialize)]
pub struct CrystalForgeConfig {
    pub flakes: Option<FlakeConfig>,
    pub database: Option<DatabaseConfig>,
    pub server: Option<ServerConfig>,
    pub client: Option<AgentConfig>,
    pub environments: Option<EnvironmentConfig>,
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
