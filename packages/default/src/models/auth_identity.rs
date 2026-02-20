use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "auth_role", rename_all = "lowercase")]
pub enum AuthRole {
    Admin,
    Operator,
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ExternalIdentity {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider_key: String,
    pub subject: String,
    pub tenant_discriminator: String,
    pub claims: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserRoleAssignment {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role: AuthRole,
    pub granted_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub session_token_hash: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub invalidated_at: Option<DateTime<Utc>>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
}
