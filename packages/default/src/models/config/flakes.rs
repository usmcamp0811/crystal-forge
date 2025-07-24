use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct FlakeConfig {
    pub watched: Vec<WatchedFlake>,
}

#[derive(Debug, Deserialize)]
pub struct WatchedFlake {
    pub name: String,
    pub repo_url: String,
}
