use crate::auth::repository::normalize_tenant_discriminator;
use crate::models::auth_identity::{AuthRole, ExternalIdentity, UserRoleAssignment, UserSession};
use crate::models::users::User;
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

    pub async fn find_active_session_by_token_hash(
        &self,
        session_token_hash: &str,
    ) -> Result<Option<UserSession>, AuthRepositoryError> {
        let session = sqlx::query_as::<_, UserSession>(
            "SELECT id, user_id, session_token_hash, issued_at, expires_at, last_seen_at, invalidated_at, user_agent, ip_address
             FROM user_sessions
             WHERE session_token_hash = $1
               AND invalidated_at IS NULL
               AND expires_at > NOW()",
        )
        .bind(session_token_hash)
        .fetch_optional(self.pool)
        .await?;

        Ok(session)
    }

    pub async fn invalidate_session_by_token_hash(
        &self,
        session_token_hash: &str,
    ) -> Result<bool, AuthRepositoryError> {
        let result = sqlx::query(
            "UPDATE user_sessions
             SET invalidated_at = NOW()
             WHERE session_token_hash = $1
               AND invalidated_at IS NULL",
        )
        .bind(session_token_hash)
        .execute(self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

// Convenience wrapper functions for common operations

/// Find user roles by user ID.
pub async fn find_user_roles(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<UserRoleAssignment>, AuthRepositoryError> {
    let repo = AuthIdentityRepository::new(pool);
    repo.list_roles(user_id).await
}

/// Assign a role to a user.
pub async fn assign_role_to_user(
    pool: &PgPool,
    user_id: Uuid,
    role: AuthRole,
    granted_by_user_id: Option<Uuid>,
) -> Result<UserRoleAssignment, AuthRepositoryError> {
    let repo = AuthIdentityRepository::new(pool);
    let assignment = NewRoleAssignment {
        user_id,
        role,
        granted_by_user_id,
    };
    repo.assign_role(&assignment).await
}

/// Synchronize a user's role assignment to exactly one role.
///
/// Ensures the user has exactly the specified role and removes any others.
/// Uses a transaction so delete+insert is atomic.
pub async fn sync_user_role(
    pool: &PgPool,
    user_id: Uuid,
    role: AuthRole,
) -> Result<(), AuthRepositoryError> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM user_role_assignments WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT INTO user_role_assignments (user_id, role, granted_by_user_id)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, role) DO NOTHING",
    )
    .bind(user_id)
    .bind(role)
    .bind(Option::<Uuid>::None)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Create a new user session.
pub async fn create_user_session(
    pool: &PgPool,
    user_id: Uuid,
    session_token_hash: String,
    expires_at: DateTime<Utc>,
    user_agent: Option<String>,
    ip_address: Option<String>,
) -> Result<UserSession, AuthRepositoryError> {
    let repo = AuthIdentityRepository::new(pool);
    let session = NewUserSession {
        user_id,
        session_token_hash,
        expires_at,
        user_agent,
        ip_address,
    };
    repo.create_session(&session).await
}

pub async fn find_active_session_by_token_hash(
    pool: &PgPool,
    session_token_hash: &str,
) -> Result<Option<UserSession>, AuthRepositoryError> {
    let repo = AuthIdentityRepository::new(pool);
    repo.find_active_session_by_token_hash(session_token_hash)
        .await
}

pub async fn invalidate_session_by_token_hash(
    pool: &PgPool,
    session_token_hash: &str,
) -> Result<bool, AuthRepositoryError> {
    let repo = AuthIdentityRepository::new(pool);
    repo.invalidate_session_by_token_hash(session_token_hash)
        .await
}

/// Get a session by token hash (including expired/invalidated sessions).
pub async fn get_session_by_token_hash(
    pool: &PgPool,
    session_token_hash: &str,
) -> Result<Option<UserSession>, AuthRepositoryError> {
    let session = sqlx::query_as::<_, UserSession>(
        "SELECT id, user_id, session_token_hash, issued_at, expires_at, last_seen_at, invalidated_at, user_agent, ip_address
         FROM user_sessions
         WHERE session_token_hash = $1",
    )
    .bind(session_token_hash)
    .fetch_optional(pool)
    .await?;

    Ok(session)
}

/// Get all roles for a user.
pub async fn get_user_roles(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<UserRoleAssignment>, AuthRepositoryError> {
    find_user_roles(pool, user_id).await
}

#[derive(Debug, sqlx::FromRow)]
pub struct OidcMappingMatchRow {
    pub role: Option<AuthRole>,
    pub environments: Vec<String>,
}

pub async fn get_user_by_id(pool: &PgPool, user_id: Uuid) -> Result<User, AuthRepositoryError> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, first_name, last_name, user_type, is_active, created_at, updated_at
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(user)
}

pub async fn is_user_active(pool: &PgPool, user_id: Uuid) -> Result<bool, AuthRepositoryError> {
    let value = sqlx::query_scalar::<_, bool>("SELECT is_active FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(value.unwrap_or(false))
}

pub async fn create_user_and_bind_external_identity(
    pool: &PgPool,
    email: &str,
    display_name: Option<&str>,
    provider_key: &str,
    subject: &str,
    tenant_discriminator: Option<&str>,
    claims: serde_json::Value,
) -> Result<User, AuthRepositoryError> {
    let mut tx = pool.begin().await?;

    let user_id = Uuid::new_v4();
    let username = email.split('@').next().unwrap_or(email);
    let (first_name, last_name) = match display_name {
        Some(name) => {
            let parts: Vec<&str> = name.splitn(2, ' ').collect();
            (
                Some(parts[0].to_string()),
                Some(parts.get(1).copied().unwrap_or("").to_string()),
            )
        }
        None => (Some(String::new()), Some(String::new())),
    };

    sqlx::query(
        "INSERT INTO users (id, username, first_name, last_name, email)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(username)
    .bind(&first_name)
    .bind(&last_name)
    .bind(email)
    .execute(&mut *tx)
    .await?;

    let tenant_key = normalize_tenant_discriminator(tenant_discriminator);
    sqlx::query(
        "INSERT INTO external_identities (user_id, provider_key, subject, tenant_discriminator, claims)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (provider_key, subject, tenant_discriminator)
         DO UPDATE SET
             user_id = EXCLUDED.user_id,
             claims = EXCLUDED.claims,
             updated_at = NOW()",
    )
    .bind(user_id)
    .bind(provider_key)
    .bind(subject)
    .bind(&tenant_key)
    .bind(claims)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    get_user_by_id(pool, user_id).await
}

pub async fn get_oidc_mapping_matches(
    pool: &PgPool,
    groups: &[String],
) -> Result<Vec<OidcMappingMatchRow>, AuthRepositoryError> {
    if groups.is_empty() {
        return Ok(vec![]);
    }

    let rows = sqlx::query_as::<_, OidcMappingMatchRow>(
        "SELECT role, environments FROM oidc_group_mappings WHERE group_name = ANY($1)",
    )
    .bind(groups)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn clear_user_role_assignments(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<(), AuthRepositoryError> {
    sqlx::query("DELETE FROM user_role_assignments WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn clear_user_environment_memberships(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<(), AuthRepositoryError> {
    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_environment_ids_by_names(
    pool: &PgPool,
    names: &[String],
) -> Result<Vec<Uuid>, AuthRepositoryError> {
    let ids = sqlx::query_scalar::<_, Uuid>("SELECT id FROM environments WHERE name = ANY($1)")
        .bind(names)
        .fetch_all(pool)
        .await?;
    Ok(ids)
}

pub async fn insert_user_environment_membership(
    pool: &PgPool,
    user_id: Uuid,
    environment_id: Uuid,
) -> Result<(), AuthRepositoryError> {
    sqlx::query(
        "INSERT INTO user_environment_memberships (user_id, environment_id, assigned_by_user_id)
         VALUES ($1, $2, NULL)",
    )
    .bind(user_id)
    .bind(environment_id)
    .execute(pool)
    .await?;
    Ok(())
}
