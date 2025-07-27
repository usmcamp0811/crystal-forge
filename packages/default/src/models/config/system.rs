use serde::Deserialize;
#[derive(Debug, Deserialize, Clone)]
pub struct SystemConfig {
    pub hostname: String,
    pub public_key: String,
    pub environment: String,
    pub flake_name: Option<String>, // just the flake name reference
}
