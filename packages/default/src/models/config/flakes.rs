use serde::Deserialize;
use std::time::Duration;

#[derive(Default, Debug, Deserialize, Clone)]
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
    pub auto_poll: bool,
    #[serde(default = "default_initial_commit_depth")]
    pub initial_commit_depth: usize,
}

fn default_initial_commit_depth() -> usize {
    5
}

impl WatchedFlake {
    pub fn branch(&self) -> String {
        parse_branch_from_url(&self.repo_url)
    }
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

pub fn parse_branch_from_url(url: &str) -> String {
    // Skip HTTP/HTTPS URLs for shorthand parsing
    if !url.starts_with("http://") && !url.starts_with("https://") {
        // GitHub/GitLab shorthand: github:owner/repo/branch
        if let Some(colon_pos) = url.find(':') {
            let after_colon = &url[colon_pos + 1..];
            let parts: Vec<&str> = after_colon.split('/').collect();
            if parts.len() >= 3 {
                return parts[2].to_string();
            }
        }
    }

    // Git URL with ref parameter: ?ref=branch
    if url.contains("?ref=") {
        if let Some(ref_part) = url.split("?ref=").nth(1) {
            return ref_part.split('&').next().unwrap_or("main").to_string();
        }
    }

    // GitHub web URL: /tree/branch
    if url.contains("/tree/") {
        if let Some(branch_part) = url.split("/tree/").nth(1) {
            return branch_part.split('/').next().unwrap_or("main").to_string();
        }
    }

    // Default to "main" for all other cases (including plain HTTP URLs)
    "main".to_string()
}
