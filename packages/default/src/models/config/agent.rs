use serde::Deserialize;
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
