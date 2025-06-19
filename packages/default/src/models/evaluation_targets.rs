use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct EvaluationTarget {
    pub id: i32,
    pub commit_id: i32,
    pub target_type: String,                    // e.g. "nixos", "home", "app"
    pub target_name: String,                    // e.g. system or profile name
    pub derivation_hash: Option<String>,        // populated post-build
    pub build_timestamp: Option<DateTime<Utc>>, // nullable until built
}

enum TargetType {
    NixOS,
    HomeManager,
}

impl EvaluationTarget {
    pub fn summary(&self) -> String {
        commit_id = get_by_id
        format!(
            "{}@{} ({})",
            flake_name,
            commit,
            self.target_name.as_deref().unwrap_or("unknown")
        )
    }
}
