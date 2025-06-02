use anyhow::{Context, Result};
use config::{Config, ConfigError};
use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize)]
pub struct DbConfig {
    pub host: String,
    pub user: String,
    pub password: String,
    pub dbname: String,
}

impl DbConfig {
    pub fn to_url(&self) -> String {
        format!(
            "host={} user={} password={} dbname={}",
            self.host, self.user, self.password, self.dbname
        )
    }
}

pub fn load_config() -> Result<DbConfig> {
    let config_path = env::var("CRYSTAL_FORGE_CONFIG")
        .unwrap_or_else(|_| "/var/lib/crystal_forge/config.toml".to_string());

    let settings = Config::builder()
        .add_source(config::File::with_name(&config_path).required(false))
        .add_source(config::Environment::with_prefix("CRYSTAL_FORGE").separator("_"))
        .build()
        .context("loading configuration")?;

    settings
        .get::<DbConfig>("database")
        .context("parsing [database] section")
}
