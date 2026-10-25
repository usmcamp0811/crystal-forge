//! Verifies deployment TOML compatibility with the server's existing serializer.
//!
//! The generated agent configuration uses the production cf-config root
//! deserializer. Serialization must retain deployed integer-second fields.

use cf_config::config::{CrystalForgeConfig, DeploymentConfig};
use std::time::Duration;

#[test]
fn generated_deployment_toml_roundtrips() {
    let text = match std::env::var("CF_TEST_GENERATED_DEPLOYMENT_CONFIG") {
        Ok(path) => std::fs::read_to_string(path).unwrap(),
        Err(_) => include_str!("../../cf-config/tests/fixtures/deployment-config.toml").to_owned(),
    };
    let config: CrystalForgeConfig = config::Config::builder()
        .add_source(config::File::from_str(&text, config::FileFormat::Toml))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    let encoded = toml::to_string(&config.deployment).unwrap();
    let table: toml::Table = toml::from_str(&encoded).unwrap();
    assert_eq!(table["deployment_poll_interval"].as_integer(), Some(900));
    assert_eq!(
        table["post_agent_start_deployment_delay"].as_integer(),
        Some(0)
    );
    let decoded: DeploymentConfig = toml::from_str(&encoded).unwrap();
    assert_eq!(decoded.deployment_poll_interval, Duration::from_secs(900));
    assert_eq!(decoded.post_agent_start_deployment_delay, Duration::ZERO);
}
