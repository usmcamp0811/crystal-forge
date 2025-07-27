use serde::Deserialize;
/// PostgreSQL database connection configuration.
///
/// This section is loaded from `[database]` in `config.toml`.
#[derive(Debug, Deserialize, Clone)]
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
    pub fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            user: "crystal_forge".to_string(),
            password: "password".to_string(),
            name: "crystal_forge".to_string(),
        }
    }
    /// Returns a PostgreSQL connection string.
    pub fn to_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.name
        )
    }
}
