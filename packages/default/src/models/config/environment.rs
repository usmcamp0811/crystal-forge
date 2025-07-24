use serde::Deserialize;
#[derive(Debug, Deserialize)]
pub struct EnvironmentConfig {
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub risk_profile: String,
    pub compliance_level: String,
}
