use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug)]
pub struct PendingTarget {
    pub flake_name: String,
    pub repo_url: String,
    pub commit_hash: String,
    pub target: EvaluationTarget,
}
