use serde::Deserialize;

fn default_deployment_policy() -> String {
    "manual".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct SystemConfig {
    pub hostname: String,
    pub public_key: String,
    pub environment: String,
    pub flake_name: Option<String>, // just the flake name reference
    #[serde(default = "default_deployment_policy")]
    pub deployment_policy: String, // Will be converted to/from DeploymentPolicy enum
    pub desired_target: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::SystemConfig;
    use serde_json::json;

    #[test]
    fn defaults_deployment_policy_when_missing() {
        let value = json!({
            "hostname": "node-1",
            "public_key": "ssh-ed25519 AAAA",
            "environment": "dev",
            "flake_name": null,
            "desired_target": null
        });

        let cfg: SystemConfig = serde_json::from_value(value).expect("system config should parse");
        assert_eq!(cfg.deployment_policy, "manual");
    }

    #[test]
    fn keeps_explicit_deployment_policy() {
        let value = json!({
            "hostname": "node-1",
            "public_key": "ssh-ed25519 AAAA",
            "environment": "dev",
            "flake_name": null,
            "deployment_policy": "auto_latest",
            "desired_target": null
        });

        let cfg: SystemConfig = serde_json::from_value(value).expect("system config should parse");
        assert_eq!(cfg.deployment_policy, "auto_latest");
    }
}
