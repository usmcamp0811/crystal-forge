use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct AgentHeartbeat {
    pub id: i64,
    pub system_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub agent_version: Option<String>,
    pub agent_build_hash: Option<String>,
}
