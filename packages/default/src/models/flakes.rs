use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Flake {
    pub id: i32,
    pub name: String,
    pub repo_url: String,
    pub branch: String,
    pub build_scope: String,
    pub deleted_at: Option<DateTime<Utc>>,
    /// Sync state: "unknown" | "synced" | "syncing" | "error" (TASK-385 / migration 0157).
    /// Added as Option to allow reading rows from before the migration with sqlx offline mode.
    #[sqlx(default)]
    pub sync_status: Option<String>,
    /// Timestamp of the last sync attempt.
    #[sqlx(default)]
    pub last_sync_at: Option<DateTime<Utc>>,
    /// Error text from the last failed sync, if any.
    #[sqlx(default)]
    pub last_sync_error: Option<String>,
}
