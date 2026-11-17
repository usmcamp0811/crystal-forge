use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::models::auth_identity::AuthRole;
use crate::models::retry_policy::AutomaticRetryPolicy;

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

#[derive(Debug, sqlx::FromRow)]
pub struct DatabaseRuntimeInfoRow {
    pub database_name: String,
    pub database_size: String,
    pub server_version: String,
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

pub async fn database_runtime_info(pool: &PgPool) -> Result<DatabaseRuntimeInfoRow> {
    let row = sqlx::query_as::<_, DatabaseRuntimeInfoRow>(
        "SELECT current_database() AS database_name,
                pg_size_pretty(pg_database_size(current_database())) AS database_size,
                current_setting('server_version') AS server_version",
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn active_session_count(pool: &PgPool) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint
         FROM user_sessions
         WHERE invalidated_at IS NULL
           AND expires_at > NOW()",
    )
    .fetch_one(pool)
    .await?;

    Ok(count)
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
    let mut tx = pool.begin().await?;
    let was_active: Option<bool> =
        sqlx::query_scalar("SELECT is_active FROM users WHERE id = $1 FOR UPDATE")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;

    sqlx::query("UPDATE users SET is_active = $1 WHERE id = $2")
        .bind(enabled)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    if was_active == Some(false) && enabled {
        // SECURITY: A reactivated account starts a new notification eligibility
        // interval. Events created while the account was inactive must not enter
        // its inbox or external-email queue.
        sqlx::query(
            "INSERT INTO user_notification_preferences (
                 user_id, deploy_failures_in_app_enabled_at,
                 build_failures_in_app_enabled_at,
                 critical_cves_in_app_enabled_at,
                 policy_violations_in_app_enabled_at, initialized_at
             ) VALUES ($1, NOW(), NOW(), NOW(), NOW(), NOW())
             ON CONFLICT (user_id) DO UPDATE SET
                 deploy_failures_email_enabled_at = CASE
                     WHEN user_notification_preferences.deploy_failures
                      AND user_notification_preferences.delivery_channel IN ('email', 'both') THEN NOW()
                 END,
                 build_failures_email_enabled_at = CASE
                     WHEN user_notification_preferences.build_failures
                      AND user_notification_preferences.delivery_channel IN ('email', 'both') THEN NOW()
                 END,
                 critical_cves_email_enabled_at = CASE
                     WHEN user_notification_preferences.critical_cves
                      AND user_notification_preferences.delivery_channel IN ('email', 'both') THEN NOW()
                 END,
                 policy_violations_email_enabled_at = CASE
                     WHEN user_notification_preferences.policy_violations
                      AND user_notification_preferences.delivery_channel IN ('email', 'both') THEN NOW()
                 END,
                 heartbeat_lost_email_enabled_at = CASE
                     WHEN user_notification_preferences.heartbeat_lost
                      AND user_notification_preferences.delivery_channel IN ('email', 'both') THEN NOW()
                 END,
                 deploy_failures_in_app_enabled_at = CASE
                     WHEN user_notification_preferences.deploy_failures
                      AND user_notification_preferences.delivery_channel IN ('in_app', 'both') THEN NOW()
                 END,
                 build_failures_in_app_enabled_at = CASE
                     WHEN user_notification_preferences.build_failures
                      AND user_notification_preferences.delivery_channel IN ('in_app', 'both') THEN NOW()
                 END,
                 critical_cves_in_app_enabled_at = CASE
                     WHEN user_notification_preferences.critical_cves
                      AND user_notification_preferences.delivery_channel IN ('in_app', 'both') THEN NOW()
                 END,
                 policy_violations_in_app_enabled_at = CASE
                     WHEN user_notification_preferences.policy_violations
                      AND user_notification_preferences.delivery_channel IN ('in_app', 'both') THEN NOW()
                 END,
                 heartbeat_lost_in_app_enabled_at = CASE
                     WHEN user_notification_preferences.heartbeat_lost
                      AND user_notification_preferences.delivery_channel IN ('in_app', 'both') THEN NOW()
                 END,
                 weekly_digest_enabled_at = CASE
                     WHEN user_notification_preferences.weekly_digest
                      AND user_notification_preferences.delivery_channel IN ('email', 'both') THEN NOW()
                 END,
                 initialized_at = NOW(),
                 updated_at = NOW()",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
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
         VALUES ($1, $2::auth_role, $3::text[])
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

/// Serializes OIDC grant reconciliation and guarded administrator mutations.
///
/// All callers MUST acquire shared advisory lock `(526, 2)` before locking a
/// user, identity or assignment. Guard counts then see committed grant changes
/// from the preceding holder without locking other users or assignment rows.
/// Authoritative IdP revocation may remove the final managed Admin; this lock
/// orders guard decisions but MUST NOT preserve revoked managed authority.
///
/// # Errors
/// Returns an error if the transaction cannot acquire the advisory lock.
pub(crate) async fn lock_admin_grant_changes(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(526, 2)")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn lock_user_guard(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Option<UserDeleteGuardRow>> {
    // Read assignments in a new statement after any wait on the user lock.
    let exists = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await?;
    if exists.is_none() {
        return Ok(None);
    }
    Ok(sqlx::query_as::<_, UserDeleteGuardRow>(
        "SELECT u.is_active,
                EXISTS(SELECT 1 FROM user_role_assignments ura
                       WHERE ura.user_id = u.id AND ura.role = 'admin') AS has_admin_role
         FROM users u WHERE u.id = $1 FOR UPDATE OF u",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?)
}

async fn enabled_admin_count(tx: &mut Transaction<'_, Postgres>) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM users u WHERE u.is_active
         AND EXISTS(SELECT 1 FROM user_role_assignments ura
                    WHERE ura.user_id = u.id AND ura.role = 'admin')",
    )
    .fetch_one(&mut **tx)
    .await?)
}

// Explicit replacement classifies only this category. Delete source-less rows
// before clearing flags so the source CHECK holds at every statement boundary.
async fn clear_local_roles(tx: &mut Transaction<'_, Postgres>, user_id: Uuid) -> Result<()> {
    sqlx::query(
        "DELETE FROM user_role_assignments WHERE user_id = $1 AND oidc_identity_id IS NULL",
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE user_role_assignments SET local_grant = false, legacy_ownership_unresolved = false
         WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Removes local and unresolved legacy role sources, retaining managed roles.
///
/// Does not classify environment ownership or enforce the last-admin guard.
/// Administrator role replacement should use the guarded atomic replacement.
///
/// # Errors
/// Returns an error on database failure; all changes roll back together.
pub async fn clear_user_roles(pool: &PgPool, user_id: Uuid) -> Result<()> {
    let mut tx = pool.begin().await?;
    lock_user_guard(&mut tx, user_id).await?;
    clear_local_roles(&mut tx, user_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Disables a user unless this removes the last enabled effective Admin.
///
/// # Errors
/// Returns an error on database failure without committing a partial mutation.
pub async fn disable_user_with_admin_guard(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<GuardedMutationOutcome> {
    let mut tx = pool.begin().await?;
    lock_admin_grant_changes(&mut tx).await?;
    let Some(target) = lock_user_guard(&mut tx, user_id).await? else {
        return Ok(GuardedMutationOutcome::NotFound);
    };
    if target.is_active && target.has_admin_role && enabled_admin_count(&mut tx).await? <= 1 {
        return Ok(GuardedMutationOutcome::GuardrailViolation);
    }
    sqlx::query("UPDATE users SET is_active = FALSE WHERE id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(GuardedMutationOutcome::Applied)
}

/// Replaces local and unresolved role sources with one explicit local role.
///
/// Retains IdP contributions and their actor attribution on overlapping rows.
/// Classifies roles only, leaving unresolved environment ownership unchanged.
/// The optional guard checks the final effective union, so a managed Admin
/// survives a local Viewer replacement. Production admin callers enable it.
///
/// # Errors
/// Returns an error on database failure; removal and insertion roll back together.
pub async fn replace_user_primary_role_with_admin_guard(
    pool: &PgPool,
    user_id: Uuid,
    role: AuthRole,
    granted_by_user_id: Uuid,
    enforce_last_admin_guard: bool,
) -> Result<GuardedMutationOutcome> {
    let mut tx = pool.begin().await?;

    lock_admin_grant_changes(&mut tx).await?;
    let Some(target) = lock_user_guard(&mut tx, user_id).await? else {
        return Ok(GuardedMutationOutcome::NotFound);
    };
    clear_local_roles(&mut tx, user_id).await?;

    sqlx::query(
        "INSERT INTO user_role_assignments
         (user_id, role, granted_by_user_id, local_grant, legacy_ownership_unresolved)
         VALUES ($1, $2, $3, true, false)
         ON CONFLICT (user_id, role) DO UPDATE SET
             local_grant = true, legacy_ownership_unresolved = false,
             granted_by_user_id = CASE WHEN user_role_assignments.oidc_identity_id IS NULL
                 THEN EXCLUDED.granted_by_user_id ELSE user_role_assignments.granted_by_user_id END",
    )
    .bind(user_id)
    .bind(role)
    .bind(granted_by_user_id)
    .execute(&mut *tx)
    .await?;

    if enforce_last_admin_guard
        && target.is_active
        && target.has_admin_role
        && enabled_admin_count(&mut tx).await? == 0
    {
        tx.rollback().await?;
        return Ok(GuardedMutationOutcome::GuardrailViolation);
    }

    tx.commit().await?;
    Ok(GuardedMutationOutcome::Applied)
}

/// Deletes a user and owned grants before deleting linked identities.
///
/// Preserves audit event identifiers and clears references from surviving grants.
/// Rejects deletion of the last enabled effective Admin.
///
/// # Errors
/// Returns an error on database failure; deletion rolls back atomically.
pub async fn delete_user_with_admin_guard(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<GuardedMutationOutcome> {
    let mut tx = pool.begin().await?;

    lock_admin_grant_changes(&mut tx).await?;
    let guard_row = lock_user_guard(&mut tx, user_id).await?;

    let Some(guard_row) = guard_row else {
        tx.rollback().await?;
        return Ok(GuardedMutationOutcome::NotFound);
    };

    if guard_row.is_active && guard_row.has_admin_role && enabled_admin_count(&mut tx).await? <= 1 {
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

    // INVARIANT: Source foreign keys restrict identity removal. Remove owned
    // effective grants first, even though user deletion also cascades them.
    for table in [
        "user_role_assignments",
        "user_environment_memberships",
        "external_identities",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE user_id = $1"))
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    }

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

/// Replaces local and unresolved environment sources, retaining IdP membership.
///
/// An empty list explicitly resolves only environment ownership. Role sources
/// remain unchanged. Retained managed rows keep their original actor attribution.
///
/// # Errors
/// Returns an error for unknown environments, missing users or database failures.
/// All membership changes roll back together on failure.
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

    if lock_user_guard(&mut tx, user_id)
        .await
        .map_err(|_| "Failed to lock membership owner".to_string())?
        .is_none()
    {
        return Err("User not found".to_string());
    }

    sqlx::query(
        "DELETE FROM user_environment_memberships WHERE user_id = $1 AND oidc_identity_id IS NULL",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| "Failed to update environment memberships".to_string())?;

    sqlx::query(
        "UPDATE user_environment_memberships
         SET local_grant = false, legacy_ownership_unresolved = false WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| "Failed to classify environment memberships".to_string())?;

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
            "INSERT INTO user_environment_memberships
             (user_id, environment_id, assigned_by_user_id, local_grant, legacy_ownership_unresolved)
             VALUES ($1, $2, $3, true, false)
             ON CONFLICT (user_id, environment_id) DO UPDATE SET
                 local_grant = true, legacy_ownership_unresolved = false,
                 assigned_by_user_id = CASE WHEN user_environment_memberships.oidc_identity_id IS NULL
                     THEN EXCLUDED.assigned_by_user_id ELSE user_environment_memberships.assigned_by_user_id END",
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

// ── Classification banner config ──────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
pub struct ClassificationBannerConfigRow {
    pub enabled: bool,
    pub level: String,
    pub custom_text: String,
}

pub async fn get_classification_banner_config(
    pool: &PgPool,
) -> Result<ClassificationBannerConfigRow> {
    let row = sqlx::query_as::<_, ClassificationBannerConfigRow>(
        "SELECT enabled, level, custom_text FROM classification_banner_config WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn upsert_classification_banner_config(
    pool: &PgPool,
    enabled: bool,
    level: &str,
    custom_text: &str,
) -> Result<ClassificationBannerConfigRow> {
    let row = sqlx::query_as::<_, ClassificationBannerConfigRow>(
        "INSERT INTO classification_banner_config (id, enabled, level, custom_text)
         VALUES (1, $1, $2, $3)
         ON CONFLICT (id) DO UPDATE
             SET enabled = EXCLUDED.enabled,
                 level = EXCLUDED.level,
                 custom_text = EXCLUDED.custom_text,
                 updated_at = NOW()
         RETURNING enabled, level, custom_text",
    )
    .bind(enabled)
    .bind(level)
    .bind(custom_text)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get_automatic_retry_policy(pool: &PgPool) -> Result<AutomaticRetryPolicy> {
    let policy = sqlx::query_as::<_, AutomaticRetryPolicy>(
        "SELECT max_build_retries, max_evaluation_retries, backoff_seconds, transient_only
         FROM automatic_retry_policy
         WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(policy.unwrap_or_default())
}

pub async fn upsert_automatic_retry_policy(
    pool: &PgPool,
    policy: AutomaticRetryPolicy,
) -> Result<AutomaticRetryPolicy> {
    let policy = sqlx::query_as::<_, AutomaticRetryPolicy>(
        "INSERT INTO automatic_retry_policy (
             id, max_build_retries, max_evaluation_retries, backoff_seconds, transient_only
         )
         VALUES (1, $1, $2, $3, $4)
         ON CONFLICT (id) DO UPDATE SET
             max_build_retries = EXCLUDED.max_build_retries,
             max_evaluation_retries = EXCLUDED.max_evaluation_retries,
             backoff_seconds = EXCLUDED.backoff_seconds,
             transient_only = EXCLUDED.transient_only,
             updated_at = NOW()
         RETURNING max_build_retries, max_evaluation_retries, backoff_seconds, transient_only",
    )
    .bind(policy.max_build_retries)
    .bind(policy.max_evaluation_retries)
    .bind(policy.backoff_seconds)
    .bind(policy.transient_only)
    .fetch_one(pool)
    .await?;

    Ok(policy)
}

#[cfg(test)]
mod retry_policy_tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn automatic_retry_policy_round_trips(pool: PgPool) {
        let expected = AutomaticRetryPolicy {
            max_build_retries: 5,
            max_evaluation_retries: 0,
            backoff_seconds: 120,
            transient_only: false,
        };

        let saved = upsert_automatic_retry_policy(&pool, expected)
            .await
            .expect("policy should save");
        let loaded = get_automatic_retry_policy(&pool)
            .await
            .expect("policy should load");

        assert_eq!(saved, expected);
        assert_eq!(loaded, expected);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn automatic_retry_policy_defaults_when_row_is_absent(pool: PgPool) {
        sqlx::query("DELETE FROM automatic_retry_policy")
            .execute(&pool)
            .await
            .expect("seed policy should delete");

        let loaded = get_automatic_retry_policy(&pool)
            .await
            .expect("fallback policy should load");

        assert_eq!(loaded, AutomaticRetryPolicy::default());
    }
}
