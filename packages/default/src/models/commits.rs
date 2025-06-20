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
    pub git_commit_hash: String,
    pub repo_url: String,
    pub flake_name: String,
}

impl Commit {
    pub async fn get_flake<'a>(&self, pool: &'a PgPool) -> Result<Flake> {
        crate::queries::flakes::get_flake_by_id(pool, self.flake_id).await
    }
}
