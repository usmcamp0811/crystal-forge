use crate::models::config::CrystalForgeConfig;
use anyhow::{Context, Result};
use config::Config;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use tokio_postgres::NoTls;
use tracing::debug;

pub fn debug_print_config(cfg: &CrystalForgeConfig) {
    debug!("🔧 Loaded Configuration:");

    if let Some(db) = &cfg.database {
        let masked: String = "*".repeat(db.password.chars().count());
        debug!("  [database]");
        debug!("    host = {}", db.host);
        debug!("    user = {}", db.user);
        debug!("    password = {}", masked);
        debug!("    name = {}", db.name);
    }

    if let Some(server) = &cfg.server {
        debug!("  [server]");
        debug!("    host = {}", server.host);
        debug!("    port = {}", server.port);
        for (k, v) in &server.authorized_keys {
            debug!("    authorized_keys[{}] = {}", k, v);
        }
    }

    if let Some(client) = &cfg.client {
        debug!("  [client]");
        debug!("    server_host = {}", client.server_host);
        debug!("    server_port = {}", client.server_port);
        debug!("    private_key = ***");
    }

    if let Some(flakes) = &cfg.flakes {
        debug!("  [flakes]");
        for (name, url) in &flakes.watched {
            debug!("    {} = {}", name, url);
        }
    }
}

/// Top-level application configuration structure.
///
/// This groups all config sections from the TOML file:
/// - `[database]` → `DatabaseConfig`
/// - `[server]`   → `ServerConfig`
/// - `[client]`   → `AgentConfig`

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
