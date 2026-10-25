use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
/// PostgreSQL database connection configuration.
///
/// This section is loaded from `[database]` in `config.toml`.
#[derive(Default, Debug, Deserialize, Clone)]
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
    /// Returns a PostgreSQL connection string for TCP hosts or Unix sockets.
    ///
    /// The field values are percent-encoded so reserved URI characters and
    /// spaces retain their values in both `tokio_postgres` and SQLx parsers.
    /// A host beginning with `/` is encoded as the URI authority host. Both
    /// PostgreSQL clients interpret that decoded path as a Unix socket
    /// directory. Keep this representation shared because connection
    /// validation and the SQLx pool consume the same string.
    pub fn to_url(&self) -> String {
        let encode = |value: &str| utf8_percent_encode(value, NON_ALPHANUMERIC).to_string();
        let host = if self.host.starts_with('/') {
            encode(&self.host)
        } else {
            self.host.clone()
        };

        format!(
            "postgres://{}:{}@{}:{}/{}",
            encode(&self.user),
            encode(&self.password),
            host,
            self.port,
            encode(&self.name)
        )
    }
}
