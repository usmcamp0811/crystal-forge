use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Commit {
    pub id: i32,
    pub flake_id: i32,
    pub git_commit_hash: String,
    pub commit_timestamp: DateTime<Utc>,
}

pub struct PendingCommit {
    pub commit_hash: String,
    pub repo_url: String,
    pub flake_name: String,
}
