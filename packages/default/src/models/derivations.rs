use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct SystemBuild {
    pub id: i32,
    pub commit_id: i32,
    pub system_name: String,
    pub derivation_hash: Option<String>,
    pub build_timestamp: DateTime<Utc>,
}
