use crate::auth::repository::normalize_tenant_discriminator;
use crate::models::auth_identity::{AuthRole, ExternalIdentity, UserRoleAssignment, UserSession};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::error::Error;
use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Debug)]
pub enum AuthRepositoryError {
    Database(sqlx::Error),
}

impl Display for AuthRepositoryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
        }
    }
}

impl Error for AuthRepositoryError {}

impl From<sqlx::Error> for AuthRepositoryError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

#[derive(Debug, Clone)]
pub struct NewExternalIdentity {
    pub user_id: Uuid,
    pub provider_key: String,
    pub subject: String,
    pub tenant_discriminator: Option<String>,
    pub claims: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct NewRoleAssignment {
    pub user_id: Uuid,
    pub role: AuthRole,
    pub granted_by_user_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct NewUserSession {
    pub user_id: Uuid,
    pub session_token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
}

pub struct AuthIdentityRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> AuthIdentityRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn find_external_identity(
        &self,
        provider_key: &str,
        subject: &str,
        tenant_discriminator: Option<&str>,
    ) -> Result<Option<ExternalIdentity>, AuthRepositoryError> {
        let tenant_key = normalize_tenant_discriminator(tenant_discriminator);
        let record = sqlx::query_as::<_, ExternalIdentity>(
            "SELECT id, user_id, provider_key, subject, tenant_discriminator, claims, created_at, updated_at
             FROM external_identities
             WHERE provider_key = $1
               AND subject = $2
               AND tenant_discriminator = $3",
        )
        .bind(provider_key)
        .bind(subject)
        .bind(&tenant_key)
        .fetch_optional(self.pool)
        .await?;

        Ok(record)
    }

    pub async fn upsert_external_identity(
        &self,
        record: &NewExternalIdentity,
    ) -> Result<ExternalIdentity, AuthRepositoryError> {
        let tenant_key = normalize_tenant_discriminator(record.tenant_discriminator.as_deref());
        let external_identity = sqlx::query_as::<_, ExternalIdentity>(
            "INSERT INTO external_identities (user_id, provider_key, subject, tenant_discriminator, claims)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (provider_key, subject, tenant_discriminator)
             DO UPDATE SET
                 user_id = EXCLUDED.user_id,
                 claims = EXCLUDED.claims,
                 updated_at = NOW()
             RETURNING id, user_id, provider_key, subject, tenant_discriminator, claims, created_at, updated_at",
        )
        .bind(record.user_id)
        .bind(&record.provider_key)
        .bind(&record.subject)
        .bind(&tenant_key)
        .bind(&record.claims)
        .fetch_one(self.pool)
        .await?;

        Ok(external_identity)
    }

    pub async fn assign_role(
        &self,
        assignment: &NewRoleAssignment,
    ) -> Result<UserRoleAssignment, AuthRepositoryError> {
        let role_assignment = sqlx::query_as::<_, UserRoleAssignment>(
            "INSERT INTO user_role_assignments (user_id, role, granted_by_user_id)
             VALUES ($1, $2, $3)
             ON CONFLICT (user_id, role)
             DO UPDATE SET granted_by_user_id = EXCLUDED.granted_by_user_id
             RETURNING id, user_id, role, granted_by_user_id, created_at",
        )
        .bind(assignment.user_id)
        .bind(assignment.role)
        .bind(assignment.granted_by_user_id)
        .fetch_one(self.pool)
        .await?;

        Ok(role_assignment)
    }

    pub async fn list_roles(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<UserRoleAssignment>, AuthRepositoryError> {
        let roles = sqlx::query_as::<_, UserRoleAssignment>(
            "SELECT id, user_id, role, granted_by_user_id, created_at
             FROM user_role_assignments
             WHERE user_id = $1
             ORDER BY created_at ASC",
        )
        .bind(user_id)
        .fetch_all(self.pool)
        .await?;

        Ok(roles)
    }

    pub async fn create_session(
        &self,
        session: &NewUserSession,
    ) -> Result<UserSession, AuthRepositoryError> {
        let created_session = sqlx::query_as::<_, UserSession>(
            "INSERT INTO user_sessions (user_id, session_token_hash, expires_at, user_agent, ip_address)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, user_id, session_token_hash, issued_at, expires_at, last_seen_at, invalidated_at, user_agent, ip_address",
        )
        .bind(session.user_id)
        .bind(&session.session_token_hash)
        .bind(session.expires_at)
        .bind(&session.user_agent)
        .bind(&session.ip_address)
        .fetch_one(self.pool)
        .await?;

        Ok(created_session)
    }
}
