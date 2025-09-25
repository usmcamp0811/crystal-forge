use serde::Deserialize;
#[derive(Debug, Deserialize, Clone)]
pub struct SystemConfig {
    pub hostname: String,
    pub public_key: String,
    pub environment: String,
    pub flake_name: Option<String>, // just the flake name reference
    pub desired_derivation: Option<String>,
    pub deployment_policy: String, // Will be converted to/from DeploymentPolicy enum
    pub server_public_key: Option<String>,
}
