use anyhow::{Context, Result};
use config::Config;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use tokio_postgres::NoTls;

/// Top-level application configuration structure.
///
/// This groups all config sections from the TOML file:
/// - `[database]` → `DbConfig`
/// - `[server]`   → `ServerConfig`
/// - `[client]`   → `AgentConfig`
#[derive(Debug, Deserialize)]
pub struct CrystalForgeConfig {
    pub database: DbConfig,
    pub server: ServerConfig,
    pub client: AgentConfig,
}

/// Configuration for the agent/client.
///
/// This section is loaded from `[client]` in `config.toml`.
#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    /// Hostname or IP address of the server to connect to.
    pub server_host: String,

    /// Port the server is listening on.
    pub server_port: u16,

    /// Path to the Ed25519 private key file used for signing.
    pub private_key: String,
}

impl AgentConfig {
    /// Returns the full HTTP URL to the configured server.
    pub fn endpoint(&self) -> String {
        format!("http://{}:{}", self.server_host, self.server_port)
    }
}

/// Configuration for the server itself.
///
/// This section is loaded from `[server]` in `config.toml`.
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    /// Address to bind to (e.g., `0.0.0.0` or `127.0.0.1`).
    pub host: String,

    /// Port to bind the HTTP server to.
    pub port: u16,

    /// List of base64-encoded Ed25519 public keys authorized to post.
    pub authorized_keys: HashMap<String, String>,
}

impl ServerConfig {
    /// Returns the full socket address to bind to.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// PostgreSQL database connection configuration.
///
/// This section is loaded from `[database]` in `config.toml`.
#[derive(Debug, Deserialize)]
pub struct DbConfig {
    /// Hostname of the database server.
    pub host: String,

    /// Database user.
    pub user: String,

    /// Password for the user.
    pub password: String,

    /// Name of the database.
    pub dbname: String,
}

impl DbConfig {
    /// Returns a PostgreSQL connection string.
    ///
    /// Example: `"host=localhost user=app password=secret dbname=mydb"`
    pub fn to_url(&self) -> String {
        format!(
            "host={} user={} password={} dbname={}",
            self.host, self.user, self.password, self.dbname
        )
    }
}

/// Loads the full application configuration from `config.toml`
/// and optionally from `CRYSTAL_FORGE_*` environment variables.
///
/// The default path is `/var/lib/crystal_forge/config.toml`,
/// unless overridden by the `CRYSTAL_FORGE_CONFIG` environment variable.
pub fn load_config() -> Result<CrystalForgeConfig> {
    let config_path = env::var("CRYSTAL_FORGE_CONFIG")
        .unwrap_or_else(|_| "/var/lib/crystal_forge/config.toml".to_string());

    let settings = Config::builder()
        .add_source(config::File::with_name(&config_path).required(false))
        .add_source(config::Environment::with_prefix("CRYSTAL_FORGE").separator("__"))
        .build()
        .context("loading configuration")?;

    settings
        .try_deserialize::<CrystalForgeConfig>()
        .context("parsing full config into CrystalForgeConfig")
}

/// Attempts to connect to the PostgreSQL database.
///
/// This runs a test connection using the given URL,
/// and spawns the connection in the background to handle async messages.
///
/// # Errors
///
/// Returns an error if the connection fails.
pub async fn validate_db_connection(db_url: &str) -> anyhow::Result<()> {
    let (_client, connection) = tokio_postgres::connect(db_url, NoTls).await?;
    tokio::spawn(connection);
    Ok(())
}
