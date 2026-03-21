use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Flake {
    pub id: i32,
    pub name: String,
    pub repo_url: String,
    pub branch: String,
    pub deleted_at: Option<DateTime<Utc>>,
}
