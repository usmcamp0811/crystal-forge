use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::option::Option;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Environment {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub compliance_level_id: Option<i32>,
    pub risk_profile_id: Option<i32>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct RiskProfile {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct ComplianceLevel {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
}
