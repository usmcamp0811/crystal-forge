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

/// Carries one effective role or environment and its independent source flags.
///
/// Exactly one of `role` and `environment_name` is populated. Identity IDs are
/// internal ownership evidence and must not be included in the API projection.
#[derive(Debug, sqlx::FromRow)]
pub struct AdminUserGrantSourceRow {
    /// Identifies the account receiving the grant.
    pub user_id: Uuid,
    /// Contains the effective role for a role-assignment row.
    pub role: Option<AuthRole>,
    /// Contains the environment name for a membership row.
    pub environment_name: Option<String>,
    /// Indicates an independent explicit local contribution.
    pub local_grant: bool,
    /// Identifies the managed contribution, independently of local ownership.
    pub oidc_identity_id: Option<Uuid>,
    /// Indicates unresolved legacy ownership, independently of other sources.
    pub legacy_ownership_unresolved: bool,
}

/// Loads grant contributions for all requested accounts in one MVCC snapshot.
///
/// Callers derive both effective authority and source metadata from these rows
/// to avoid inconsistent reads and per-account role/membership queries. Accounts
/// without grants produce no rows. No credentials or identity claims are read.
///
/// # Errors
/// Returns an error if the database cannot load the grant projection.
pub async fn list_admin_user_grant_sources(
    pool: &PgPool,
    user_ids: &[Uuid],
) -> Result<Vec<AdminUserGrantSourceRow>> {
    Ok(sqlx::query_as::<_, AdminUserGrantSourceRow>(
        "SELECT user_id, role, NULL::text AS environment_name,
                local_grant, oidc_identity_id, legacy_ownership_unresolved
         FROM user_role_assignments WHERE user_id = ANY($1::uuid[])
         UNION ALL
         SELECT m.user_id, NULL::auth_role AS role, e.name AS environment_name,
                m.local_grant, m.oidc_identity_id, m.legacy_ownership_unresolved
         FROM user_environment_memberships m
         JOIN environments e ON e.id = m.environment_id
         WHERE m.user_id = ANY($1::uuid[])",
    )
    .bind(user_ids)
    .fetch_all(pool)
    .await?)
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

/// Updates account status and resets notification eligibility on reactivation.
///
/// Does not enforce the last-admin guard. Administrator PATCH callers use
/// [`patch_admin_user`] to commit status with grants and audits atomically.
///
/// # Errors
/// Returns an error on database failure; status and preferences roll back together.
pub async fn update_user_active(pool: &PgPool, user_id: Uuid, enabled: bool) -> Result<()> {
    let mut tx = pool.begin().await?;
    let was_active: Option<bool> =
        sqlx::query_scalar("SELECT is_active FROM users WHERE id = $1 FOR UPDATE")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;

    if let Some(was_active) = was_active {
        update_user_active_tx(&mut tx, user_id, was_active, enabled).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn update_user_active_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    was_active: bool,
    enabled: bool,
) -> Result<()> {
    sqlx::query("UPDATE users SET is_active = $1 WHERE id = $2")
        .bind(enabled)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;

    if !was_active && enabled {
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
        .execute(&mut **tx)
        .await?;
    }

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

// Clearing an override is not a legacy ownership decision. Delete only rows
// whose final source would disappear, then retain all managed/legacy sources.
async fn remove_local_role_contributions(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM user_role_assignments
         WHERE user_id = $1 AND local_grant AND oidc_identity_id IS NULL
           AND NOT legacy_ownership_unresolved",
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE user_role_assignments SET local_grant = false
         WHERE user_id = $1 AND local_grant",
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
    replace_local_role_tx(&mut tx, user_id, role, granted_by_user_id).await?;

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

async fn replace_local_role_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    role: AuthRole,
    granted_by_user_id: Uuid,
) -> Result<()> {
    clear_local_roles(tx, user_id).await?;

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
    .execute(&mut **tx)
    .await?;
    Ok(())
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

    let resolved = resolve_patch_environments(&mut tx, &normalized)
        .await
        .map_err(|_| "Failed to validate environments".to_string())?;
    if resolved.len() != normalized.len() {
        let missing = normalized
            .iter()
            .filter(|name| !resolved.iter().any(|env| &env.name == *name))
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!("Unknown environment(s): {}", missing.join(", ")));
    }
    replace_local_environments_tx(&mut tx, user_id, assigned_by_user_id, &resolved)
        .await
        .map_err(|_| "Failed to update environment memberships".to_string())?;
    tx.commit()
        .await
        .map_err(|_| "Failed to commit environment memberships".to_string())?;
    Ok(())
}

async fn resolve_patch_environments(
    tx: &mut Transaction<'_, Postgres>,
    names: &[String],
) -> Result<Vec<EnvironmentLookupRow>> {
    // Keep exact-name validation stable until commit, including deletion/rename.
    Ok(sqlx::query_as::<_, EnvironmentLookupRow>(
        "SELECT id, name FROM environments WHERE name = ANY($1) ORDER BY id FOR SHARE",
    )
    .bind(names)
    .fetch_all(&mut **tx)
    .await?)
}

async fn replace_local_environments_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    assigned_by_user_id: Uuid,
    resolved: &[EnvironmentLookupRow],
) -> Result<()> {
    sqlx::query(
        "DELETE FROM user_environment_memberships WHERE user_id = $1 AND oidc_identity_id IS NULL",
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE user_environment_memberships
         SET local_grant = false, legacy_ownership_unresolved = false WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await?;

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
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

/// Carries a prepared administrator PATCH and its authenticated audit context.
///
/// Omitted grants preserve their category unless `clear_local_role` is true.
/// Supplied replacement grants classify local/unresolved sources; clearing a
/// local role instead preserves all managed and unresolved legacy sources.
/// Callers validate DTO syntax and hash passwords before opening a transaction.
pub struct AdminUserPatch {
    /// Sets account status when supplied.
    pub enabled: Option<bool>,
    /// Replaces local role sources when supplied.
    pub role: Option<AuthRole>,
    /// Removes only local role contributions; mutually exclusive with `role`.
    pub clear_local_role: bool,
    /// Replaces local environment sources with exact database names.
    pub environments: Option<Vec<String>>,
    /// Sets an already prepared password hash; never contains plaintext.
    pub password_hash: Option<String>,
    /// Identifies the authenticated administrator assigning local grants.
    pub actor_user_id: Uuid,
    /// Identifies the session that authenticated `actor_user_id`.
    ///
    /// SECURITY: [`patch_admin_user`] rechecks this session, the actor's
    /// active status and the actor's current Admin role inside its transaction.
    /// The caller's earlier authorization is not sufficient because it is not
    /// serialized with concurrent demotion, disablement or logout.
    pub actor_session_id: Uuid,
    /// Retains the actor identifier in durable audit records.
    pub actor_identifier: String,
    /// Retains best-effort request origin metadata.
    pub request_origin: Option<String>,
}

/// Describes an atomic PATCH result without exposing database details.
#[derive(Debug, PartialEq, Eq)]
pub enum AdminUserPatchOutcome {
    /// Commits all requested mutations and audit records.
    Applied,
    /// Leaves all state unchanged because the target does not exist.
    NotFound,
    /// Leaves all state unchanged because an exact environment name is unknown.
    InvalidEnvironments,
    /// Leaves all state unchanged to retain the final enabled effective Admin.
    LastAdmin,
    /// Leaves all state unchanged and writes no audit because the acting
    /// administrator was no longer authorized when the transaction held its
    /// locks.
    ///
    /// The actor was disabled, lost every Admin role assignment, or the
    /// authenticating session was missing, invalidated, expired or owned by
    /// another user. The check precedes target lookup, so this outcome does
    /// not reveal whether the target exists.
    ActorNotAuthorized,
}

/// Locks the distinct actor and target user rows in ascending id order.
///
/// CONCURRENCY: A single `ORDER BY id FOR UPDATE` statement locks both rows in
/// a deterministic order, so two mutations that name the same pair in opposite
/// roles cannot deadlock. Missing rows are skipped; callers read the rows
/// afterwards in new statements to learn which of them exist.
async fn lock_actor_and_target(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    target_user_id: Uuid,
) -> Result<()> {
    let mut ids = [actor_user_id, target_user_id];
    ids.sort();
    let ids = if ids[0] == ids[1] {
        &ids[..1]
    } else {
        &ids[..]
    };
    sqlx::query("SELECT id FROM users WHERE id = ANY($1) ORDER BY id FOR UPDATE")
        .bind(ids)
        .fetch_all(&mut **tx)
        .await?;
    Ok(())
}

/// Returns whether the actor is still an active Admin with a live session.
///
/// SECURITY: Callers MUST hold the grant advisory lock and the actor row lock.
/// Each statement starts a new READ COMMITTED snapshot, so it sees changes
/// committed while the caller waited for those locks. The session row is
/// share-locked because session invalidation does not take the advisory lock;
/// the lock holds the session valid until this transaction ends. The expiry
/// test uses `clock_timestamp()` because `NOW()` is the transaction start,
/// which precedes any lock wait.
async fn actor_still_authorized(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    actor_session_id: Uuid,
) -> Result<bool> {
    let active_admin = sqlx::query_scalar::<_, bool>(
        "SELECT u.is_active AND EXISTS(SELECT 1 FROM user_role_assignments ura
                                       WHERE ura.user_id = u.id AND ura.role = 'admin')
         FROM users u WHERE u.id = $1",
    )
    .bind(actor_user_id)
    .fetch_optional(&mut **tx)
    .await?;
    if active_admin != Some(true) {
        return Ok(false);
    }
    Ok(sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM user_sessions
         WHERE id = $1 AND user_id = $2 AND invalidated_at IS NULL
           AND expires_at > clock_timestamp()
         FOR SHARE",
    )
    .bind(actor_session_id)
    .bind(actor_user_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

/// Applies all PATCH categories and audit records in one transaction.
///
/// Acquires the shared grant advisory lock first, then the actor and target
/// user rows in ascending id order, then rechecks that the actor is still an
/// active Admin with a valid session. Validates all environment names before
/// writes and checks the final effective Admin union before commit.
/// Rejections and audit failures roll back every category, source flag and
/// notification-eligibility change.
///
/// SECURITY: The caller's authorization happened before this transaction and
/// can be stale when it takes the locks. The in-transaction recheck is the
/// authoritative authorization; it runs before target lookup, validation and
/// every write.
///
/// # Errors
/// Returns an error for conflicting role edits or database failure, with no
/// partial committed mutation. An unauthorized actor is reported as
/// [`AdminUserPatchOutcome::ActorNotAuthorized`], not as an error.
pub async fn patch_admin_user(
    pool: &PgPool,
    user_id: Uuid,
    patch: AdminUserPatch,
) -> Result<AdminUserPatchOutcome> {
    anyhow::ensure!(
        patch.role.is_none() || !patch.clear_local_role,
        "conflicting local role edits"
    );
    let mut tx = pool.begin().await?;
    // CONCURRENCY: The advisory lock precedes every user row lock, as in all
    // other guarded mutations. The actor and target rows are then locked in
    // id order, so the actor check below waits for any in-flight change to
    // either row and reads only committed state.
    lock_admin_grant_changes(&mut tx).await?;
    lock_actor_and_target(&mut tx, patch.actor_user_id, user_id).await?;
    if !actor_still_authorized(&mut tx, patch.actor_user_id, patch.actor_session_id).await? {
        tx.rollback().await?;
        return Ok(AdminUserPatchOutcome::ActorNotAuthorized);
    }
    let Some(prior) = lock_user_guard(&mut tx, user_id).await? else {
        return Ok(AdminUserPatchOutcome::NotFound);
    };
    let environments = if let Some(names) = &patch.environments {
        let resolved = resolve_patch_environments(&mut tx, names).await?;
        if names
            .iter()
            .any(|name| !resolved.iter().any(|env| &env.name == name))
        {
            return Ok(AdminUserPatchOutcome::InvalidEnvironments);
        }
        Some(resolved)
    } else {
        None
    };
    let email: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
    let mut events = Vec::new();
    if let Some(enabled) = patch.enabled {
        update_user_active_tx(&mut tx, user_id, prior.is_active, enabled).await?;
        if enabled != prior.is_active {
            events.push((
                if enabled {
                    "user_enabled"
                } else {
                    "user_disabled"
                },
                serde_json::json!({ "enabled": enabled }),
            ));
        }
    }
    if patch.role.is_some() || patch.clear_local_role {
        let roles: Vec<AuthRole> =
            sqlx::query_scalar("SELECT role FROM user_role_assignments WHERE user_id = $1")
                .bind(user_id)
                .fetch_all(&mut *tx)
                .await?;
        let from = [AuthRole::Admin, AuthRole::Operator, AuthRole::Viewer]
            .into_iter()
            .find(|role| roles.contains(role));
        let changes = if let Some(role) = patch.role {
            replace_local_role_tx(&mut tx, user_id, role, patch.actor_user_id).await?;
            serde_json::json!({
                "from": from, "local_role": role, "classifies_legacy_roles": true,
            })
        } else {
            remove_local_role_contributions(&mut tx, user_id).await?;
            serde_json::json!({
                "from": from, "clear_local_role": true, "classifies_legacy_roles": false,
            })
        };
        // Explicit role intent is audited even if effective authority or local
        // sources are unchanged, including repeated clears with no local role.
        events.push(("user_role_assigned", changes));
    }
    if let Some(resolved) = environments {
        let from: Vec<String> = sqlx::query_scalar(
            "SELECT e.name FROM user_environment_memberships m
             JOIN environments e ON e.id = m.environment_id
             WHERE m.user_id = $1 ORDER BY e.name",
        )
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await?;
        replace_local_environments_tx(&mut tx, user_id, patch.actor_user_id, &resolved).await?;
        events.push((
            "user_environment_membership_updated",
            serde_json::json!({
                "from": from, "local_environments": patch.environments,
                "classifies_legacy_environments": true,
            }),
        ));
    }
    if let Some(hash) = &patch.password_hash {
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(hash)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        events.push((
            "user_updated",
            serde_json::json!({ "password_reset": true }),
        ));
    }
    // Check the combined final state: a role edit cannot mask disabling the
    // final Admin, and an overlapping managed Admin still satisfies the guard.
    if prior.is_active && prior.has_admin_role && enabled_admin_count(&mut tx).await? == 0 {
        tx.rollback().await?;
        return Ok(AdminUserPatchOutcome::LastAdmin);
    }
    write_patch_audits(&mut tx, &patch, &format!("{email} ({user_id})"), events).await?;
    tx.commit().await?;
    Ok(AdminUserPatchOutcome::Applied)
}

async fn write_patch_audits(
    tx: &mut Transaction<'_, Postgres>,
    patch: &AdminUserPatch,
    target: &str,
    mut events: Vec<(&str, serde_json::Value)>,
) -> Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    let count = events.len();
    events.insert(
        0,
        (
            "user_updated",
            serde_json::json!({
                "actor": patch.actor_identifier, "change_count": count,
            }),
        ),
    );
    for (index, (action, changes)) in events.into_iter().enumerate() {
        let metadata = if index == 0 {
            changes
        } else {
            serde_json::json!({ "actor": patch.actor_identifier, "changes": changes })
        };
        sqlx::query(
            "INSERT INTO admin_audit_events
             (actor_user_id, actor_identifier, action, target, request_origin, metadata)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(patch.actor_user_id)
        .bind(&patch.actor_identifier)
        .bind(action)
        .bind(target)
        .bind(&patch.request_origin)
        .bind(metadata)
        .execute(&mut **tx)
        .await?;
    }
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
