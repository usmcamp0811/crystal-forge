use serde::Deserialize;
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
