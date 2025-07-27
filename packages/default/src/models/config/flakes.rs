use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone)]
pub struct FlakeConfig {
    pub watched: Vec<WatchedFlake>,
    #[serde(with = "humantime_serde")]
    pub flake_polling_interval: Duration,
    #[serde(with = "humantime_serde")]
    pub commit_evaluation_interval: Duration,
    #[serde(with = "humantime_serde")]
    pub build_processing_interval: Duration,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WatchedFlake {
    pub name: String,
    pub repo_url: String,
    pub auto_poll: bool, // true = server polls git directly, false = webhook-only
}

impl FlakeConfig {
    pub fn default() -> Self {
        Self {
            watched: vec![],
            flake_polling_interval: Duration::from_secs(600),
            commit_evaluation_interval: Duration::from_secs(60),
            build_processing_interval: Duration::from_secs(60),
        }
    }
}
