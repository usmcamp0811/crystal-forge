use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::option::Option;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct System {
    pub id: Uuid,
    pub hostname: String,
    pub environment_id: Option<Uuid>,
    pub is_active: bool,
    pub public_key: String,
    pub flake_id: Option<i32>,
    pub derivation: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
