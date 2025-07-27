use serde::Deserialize;
#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub server_host: String,
    pub server_port: u16,
    pub private_key: String,
}

impl AgentConfig {
    pub fn default() -> Self {
        Self {
            server_host: "127.0.0.1".to_string(),
            server_port: 3000,
            private_key: "/var/lib/crystal-forge/private.key".to_string(),
        }
    }
    /// Returns the full HTTP URL to the configured server.
    pub fn endpoint(&self) -> String {
        format!("http://{}:{}", self.server_host, self.server_port)
    }
}
