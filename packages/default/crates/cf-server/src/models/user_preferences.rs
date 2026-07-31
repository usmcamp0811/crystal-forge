use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserPreferences {
    pub user_id: Uuid,
    pub theme: String,
    pub density: String,
    pub sidebar_collapsed: bool,
    pub default_systems_view: String,
    pub updated_at: DateTime<Utc>,
}
