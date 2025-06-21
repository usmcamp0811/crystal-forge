use crate::models::flakes::Flake;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::PgPool;
use std::fmt;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Commit {
    pub id: i32,
    pub flake_id: i32,
    pub git_commit_hash: String,
    pub commit_timestamp: DateTime<Utc>,
}

impl Commit {
    pub async fn get_flake(&self, pool: &PgPool) -> Result<Flake> {
        crate::queries::flakes::get_flake_by_id(pool, self.flake_id).await
    }
}

impl fmt::Display for Commit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Commit(id={}, flake_id={}, hash={}, timestamp={})",
            self.id, self.flake_id, self.git_commit_hash, self.commit_timestamp
        )
    }
}
