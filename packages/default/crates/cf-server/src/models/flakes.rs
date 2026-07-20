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
    /// Timestamp when the branch-commit snapshot was first populated (TASK-397).
    /// NULL means this flake has never had a branch snapshot — readers fall
    /// back to timestamp-based ordering until the first successful sync.
    #[sqlx(default)]
    pub snapshot_ready_at: Option<DateTime<Utc>>,
}

/// A single row in the branch-commit snapshot table.
///
/// Stores the ordered position of a commit on a flake's tracked branch.
/// Populated atomically during successful sync and read by GET handlers.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BranchCommitSnapshot {
    pub flake_id: i32,
    pub commit_id: i32,
    pub position: i32,
    pub observed_at: DateTime<Utc>,
}
