use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::models::auth_identity::AuthRole;

#[derive(Debug, sqlx::FromRow)]
pub struct AdminUserRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub is_active: bool,
    pub has_external_identity: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct OidcMappingRow {
    pub id: Uuid,
    pub group_name: String,
    pub role: Option<AuthRole>,
    pub environments: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct AdminAuditEventRow {
    pub created_at: DateTime<Utc>,
    pub actor_identifier: Option<String>,
    pub action: String,
    pub target: String,
    pub request_origin: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct AuditRoleRow {
    pub created_at: DateTime<Utc>,
    pub actor_email: Option<String>,
    pub role: AuthRole,
    pub target_email: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct AuditSessionRow {
    pub invalidated_at: DateTime<Utc>,
    pub user_email: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct EnvironmentLookupRow {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardedMutationOutcome {
    Applied,
    GuardrailViolation,
    NotFound,
}

#[derive(Debug, sqlx::FromRow)]
struct UserDeleteGuardRow {
    is_active: bool,
    has_admin_role: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct EnvironmentMembershipRow {
    environment_name: String,
}

pub async fn list_admin_users(pool: &PgPool) -> Result<Vec<AdminUserRow>> {
    let rows = sqlx::query_as::<_, AdminUserRow>(
        "SELECT u.id,
                u.username,
                u.email,
                u.is_active,
                EXISTS(SELECT 1 FROM external_identities ei WHERE ei.user_id = u.id) AS has_external_identity,
                u.updated_at
         FROM users u
         ORDER BY u.updated_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn find_admin_user(pool: &PgPool, user_id: Uuid) -> Result<Option<AdminUserRow>> {
    let row = sqlx::query_as::<_, AdminUserRow>(
        "SELECT u.id,
                u.username,
                u.email,
                u.is_active,
                EXISTS(SELECT 1 FROM external_identities ei WHERE ei.user_id = u.id) AS has_external_identity,
                u.updated_at
         FROM users u
         WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn find_admin_user_required(pool: &PgPool, user_id: Uuid) -> Result<AdminUserRow> {
    let row = sqlx::query_as::<_, AdminUserRow>(
        "SELECT u.id,
                u.username,
                u.email,
                u.is_active,
                EXISTS(SELECT 1 FROM external_identities ei WHERE ei.user_id = u.id) AS has_external_identity,
                u.updated_at
         FROM users u
         WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn count_users_by_email(pool: &PgPool, email: &str) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

pub async fn update_user_active(pool: &PgPool, user_id: Uuid, enabled: bool) -> Result<()> {
    sqlx::query("UPDATE users SET is_active = $1 WHERE id = $2")
        .bind(enabled)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_oidc_mappings(pool: &PgPool) -> Result<Vec<OidcMappingRow>> {
    let rows = sqlx::query_as::<_, OidcMappingRow>(
        "SELECT id, group_name, role, environments, updated_at
         FROM oidc_group_mappings
         ORDER BY group_name ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert_oidc_mapping(
    pool: &PgPool,
    group_name: &str,
    role: Option<AuthRole>,
    environments: &[String],
) -> Result<OidcMappingRow> {
    let row = sqlx::query_as::<_, OidcMappingRow>(
        "INSERT INTO oidc_group_mappings (group_name, role, environments)
         VALUES ($1, $2, $3)
         ON CONFLICT (group_name)
         DO UPDATE SET role = EXCLUDED.role, environments = EXCLUDED.environments
         RETURNING id, group_name, role, environments, updated_at",
    )
    .bind(group_name)
    .bind(role)
    .bind(environments)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn delete_oidc_mapping(
    pool: &PgPool,
    mapping_id: Uuid,
) -> Result<Option<OidcMappingRow>> {
    let row = sqlx::query_as::<_, OidcMappingRow>(
        "DELETE FROM oidc_group_mappings
         WHERE id = $1
         RETURNING id, group_name, role, environments, updated_at",
    )
    .bind(mapping_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn list_admin_audit_events(pool: &PgPool) -> Result<Vec<AdminAuditEventRow>> {
    let rows = sqlx::query_as::<_, AdminAuditEventRow>(
        "SELECT created_at, actor_identifier, action, target, request_origin
         FROM admin_audit_events
         ORDER BY created_at DESC
         LIMIT 300",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_role_audit_events(pool: &PgPool) -> Result<Vec<AuditRoleRow>> {
    let rows = sqlx::query_as::<_, AuditRoleRow>(
        "SELECT ura.created_at,
                actor.email AS actor_email,
                ura.role,
                target.email AS target_email
         FROM user_role_assignments ura
         LEFT JOIN users actor ON actor.id = ura.granted_by_user_id
         JOIN users target ON target.id = ura.user_id
         ORDER BY ura.created_at DESC
         LIMIT 100",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_session_audit_events(pool: &PgPool) -> Result<Vec<AuditSessionRow>> {
    let rows = sqlx::query_as::<_, AuditSessionRow>(
        "SELECT us.invalidated_at, u.email AS user_email
         FROM user_sessions us
         LEFT JOIN users u ON u.id = us.user_id
         WHERE us.invalidated_at IS NOT NULL
         ORDER BY us.invalidated_at DESC
         LIMIT 100",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_user_environment_names(pool: &PgPool, user_id: Uuid) -> Result<Vec<String>> {
    let rows = sqlx::query_as::<_, EnvironmentMembershipRow>(
        "SELECT e.name AS environment_name
         FROM user_environment_memberships uem
         JOIN environments e ON e.id = uem.environment_id
         WHERE uem.user_id = $1
         ORDER BY e.name ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|row| row.environment_name).collect())
}

pub async fn count_enabled_admins(pool: &PgPool) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT u.id)
         FROM users u
         JOIN user_role_assignments ura ON ura.user_id = u.id
         WHERE u.is_active = TRUE AND ura.role = 'admin'",
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

pub async fn clear_user_roles(pool: &PgPool, user_id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM user_role_assignments WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn disable_user_with_admin_guard(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<GuardedMutationOutcome> {
    let mut tx = pool.begin().await?;

    let active_admin_ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT u.id
         FROM users u
         JOIN user_role_assignments ura ON ura.user_id = u.id
         WHERE u.is_active = TRUE AND ura.role = 'admin'
         FOR UPDATE",
    )
    .fetch_all(&mut *tx)
    .await?;

    if active_admin_ids.len() <= 1 {
        tx.rollback().await?;
        return Ok(GuardedMutationOutcome::GuardrailViolation);
    }

    sqlx::query("UPDATE users SET is_active = FALSE WHERE id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(GuardedMutationOutcome::Applied)
}

pub async fn replace_user_primary_role_with_admin_guard(
    pool: &PgPool,
    user_id: Uuid,
    role: AuthRole,
    granted_by_user_id: Uuid,
    enforce_last_admin_guard: bool,
) -> Result<GuardedMutationOutcome> {
    let mut tx = pool.begin().await?;

    if enforce_last_admin_guard {
        let active_admin_ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT u.id
             FROM users u
             JOIN user_role_assignments ura ON ura.user_id = u.id
             WHERE u.is_active = TRUE AND ura.role = 'admin'
             FOR UPDATE",
        )
        .fetch_all(&mut *tx)
        .await?;

        if active_admin_ids.len() <= 1 {
            tx.rollback().await?;
            return Ok(GuardedMutationOutcome::GuardrailViolation);
        }
    }

    sqlx::query("DELETE FROM user_role_assignments WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT INTO user_role_assignments (user_id, role, granted_by_user_id)
         VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(role)
    .bind(granted_by_user_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(GuardedMutationOutcome::Applied)
}

pub async fn delete_user_with_admin_guard(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<GuardedMutationOutcome> {
    let mut tx = pool.begin().await?;

    let active_admin_ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT u.id
         FROM users u
         JOIN user_role_assignments ura ON ura.user_id = u.id
         WHERE u.is_active = TRUE AND ura.role = 'admin'
         FOR UPDATE",
    )
    .fetch_all(&mut *tx)
    .await?;

    let guard_row = sqlx::query_as::<_, UserDeleteGuardRow>(
        "SELECT u.is_active,
                EXISTS(
                    SELECT 1 FROM user_role_assignments ura
                    WHERE ura.user_id = u.id AND ura.role = 'admin'
                ) AS has_admin_role
         FROM users u
         WHERE u.id = $1
         FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(guard_row) = guard_row else {
        tx.rollback().await?;
        return Ok(GuardedMutationOutcome::NotFound);
    };

    if guard_row.is_active && guard_row.has_admin_role && active_admin_ids.len() <= 1 {
        tx.rollback().await?;
        return Ok(GuardedMutationOutcome::GuardrailViolation);
    }

    sqlx::query("UPDATE admin_audit_events SET actor_user_id = NULL WHERE actor_user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "UPDATE user_role_assignments SET granted_by_user_id = NULL WHERE granted_by_user_id = $1",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE user_environment_memberships SET assigned_by_user_id = NULL WHERE assigned_by_user_id = $1",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(GuardedMutationOutcome::Applied)
}

pub async fn find_user_email(pool: &PgPool, user_id: Uuid) -> Result<Option<String>> {
    let value = sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(value)
}

pub async fn insert_admin_audit_event(
    pool: &PgPool,
    actor_user_id: Uuid,
    actor_identifier: &str,
    action: &str,
    target: &str,
    request_origin: Option<String>,
    metadata: serde_json::Value,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO admin_audit_events (actor_user_id, actor_identifier, action, target, request_origin, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor_user_id)
    .bind(actor_identifier)
    .bind(action)
    .bind(target)
    .bind(request_origin)
    .bind(metadata)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn validate_environment_names_exist(
    pool: &PgPool,
    names: &[String],
) -> Result<(), String> {
    if names.is_empty() {
        return Ok(());
    }

    let resolved = sqlx::query_as::<_, EnvironmentLookupRow>(
        "SELECT id, name FROM environments WHERE name = ANY($1)",
    )
    .bind(names)
    .fetch_all(pool)
    .await
    .map_err(|_| "Failed to validate environments".to_string())?;

    if resolved.len() == names.len() {
        return Ok(());
    }

    let known = resolved
        .into_iter()
        .map(|row| row.name)
        .collect::<BTreeSet<_>>();
    let missing = names
        .iter()
        .filter(|name| !known.contains((*name).as_str()))
        .cloned()
        .collect::<Vec<_>>();

    Err(format!("Unknown environment(s): {}", missing.join(", ")))
}

pub async fn replace_user_environment_memberships(
    pool: &PgPool,
    user_id: Uuid,
    assigned_by_user_id: Uuid,
    environments: &[String],
) -> Result<(), String> {
    let normalized: Vec<String> = environments
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "Failed to start membership update".to_string())?;

    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| "Failed to update environment memberships".to_string())?;

    if normalized.is_empty() {
        tx.commit()
            .await
            .map_err(|_| "Failed to commit environment memberships".to_string())?;
        return Ok(());
    }

    let resolved = sqlx::query_as::<_, EnvironmentLookupRow>(
        "SELECT id, name FROM environments WHERE name = ANY($1)",
    )
    .bind(&normalized)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| "Failed to validate environments".to_string())?;

    if resolved.len() != normalized.len() {
        let known = resolved
            .iter()
            .map(|row| row.name.clone())
            .collect::<BTreeSet<_>>();
        let missing = normalized
            .iter()
            .filter(|name| !known.contains((*name).as_str()))
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!("Unknown environment(s): {}", missing.join(", ")));
    }

    for env in resolved {
        sqlx::query(
            "INSERT INTO user_environment_memberships (user_id, environment_id, assigned_by_user_id)
             VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(env.id)
        .bind(assigned_by_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| "Failed to insert environment membership".to_string())?;
    }

    tx.commit()
        .await
        .map_err(|_| "Failed to commit environment memberships".to_string())?;
    Ok(())
}
