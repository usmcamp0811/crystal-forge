//! Persists owner access tokens, their immutable grants and their audit events.
//!
//! Only the SHA-256 of a token is stored; no function here accepts or returns
//! plaintext. Writers run inside a caller-owned transaction. The caller MUST
//! call [`lock_owner_tx`] first: it takes a per-owner advisory lock and then a
//! shared lock on the owner row. That order serializes every create and rotate
//! for one owner, so count, name uniqueness, rate and revoke-then-issue
//! decisions see each other's committed rows.
//!
//! Queries use the runtime SQLx API. They do not need offline metadata.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{Executor, Postgres, Transaction};
use uuid::Uuid;

use crate::models::auth_identity::AuthRole;

/// Selects one token with its derived lifecycle status.
///
/// `statement_timestamp()` keeps the status of every row in one statement
/// consistent. Precedence: revoked, expired, owner suspended, expiring within
/// 7 days, active.
const TOKEN_SELECT: &str = "SELECT t.id, t.user_id, t.name, t.audience, t.display_hint,
        t.created_at, t.expires_at, t.revoked_at, t.last_used_at,
        CASE WHEN t.revoked_at IS NOT NULL THEN 'revoked'
             WHEN t.expires_at <= statement_timestamp() THEN 'expired'
             WHEN NOT u.is_active THEN 'owner_suspended'
             WHEN t.expires_at <= statement_timestamp() + interval '7 days' THEN 'expiring_soon'
             ELSE 'active' END AS status
    FROM api_access_tokens t JOIN users u ON u.id = t.user_id";

/// Holds one token row without secret material.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TokenRow {
    /// Public token id.
    pub id: Uuid,
    /// Owner user id.
    pub user_id: Uuid,
    /// Display name.
    pub name: String,
    /// Audience; `mcp` is the only value the schema allows.
    pub audience: String,
    /// Masked hint.
    pub display_hint: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Expiry time.
    pub expires_at: DateTime<Utc>,
    /// Revocation time.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Coarse last-use time.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Derived status string, see `TOKEN_SELECT`.
    pub status: String,
}

/// Holds one granted environment with its current display data.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TokenEnvironmentRow {
    /// Token the grant belongs to.
    pub token_id: Uuid,
    /// Environment id.
    pub id: Uuid,
    /// Environment name.
    pub name: String,
    /// Whether the environment is active now.
    pub is_active: bool,
}

/// Holds the owner's current account state.
#[derive(Debug, Clone)]
pub struct OwnerAccess {
    /// Whether the owner account is enabled.
    pub is_active: bool,
    /// Current roles.
    pub roles: Vec<AuthRole>,
    /// Current environment memberships, sorted.
    pub environment_ids: Vec<Uuid>,
}

impl OwnerAccess {
    /// Reports whether the owner currently holds the Admin role.
    pub fn is_admin(&self) -> bool {
        self.roles.contains(&AuthRole::Admin)
    }

    /// Reports whether the owner currently holds Operator or Admin.
    pub fn is_operator_or_admin(&self) -> bool {
        self.roles
            .iter()
            .any(|role| matches!(role, AuthRole::Admin | AuthRole::Operator))
    }

    /// Reports whether the owner holds at least one role.
    pub fn has_any_role(&self) -> bool {
        !self.roles.is_empty()
    }
}

#[derive(sqlx::FromRow)]
struct OwnerAccessRow {
    is_active: bool,
    roles: Vec<String>,
    environment_ids: Vec<Uuid>,
}

/// Reads the owner's active flag, roles and environment memberships in one
/// statement.
///
/// Returns `None` when the user does not exist. Inside a transaction that
/// holds [`lock_owner_tx`], the result is current for the whole transaction
/// because concurrent role and membership writers lock the user row.
///
/// # Errors
/// Returns database errors.
pub async fn owner_access<'e, E>(executor: E, user_id: Uuid) -> Result<Option<OwnerAccess>>
where
    E: Executor<'e, Database = Postgres>,
{
    let row = sqlx::query_as::<_, OwnerAccessRow>(
        "SELECT u.is_active,
                ARRAY(SELECT role::text FROM user_role_assignments
                      WHERE user_id = u.id ORDER BY role) AS roles,
                ARRAY(SELECT environment_id FROM user_environment_memberships
                      WHERE user_id = u.id ORDER BY environment_id) AS environment_ids
         FROM users u WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_optional(executor)
    .await?;
    Ok(row.map(|row| OwnerAccess {
        is_active: row.is_active,
        roles: row
            .roles
            .iter()
            .filter_map(|role| match role.as_str() {
                "admin" => Some(AuthRole::Admin),
                "operator" => Some(AuthRole::Operator),
                "viewer" => Some(AuthRole::Viewer),
                _ => None,
            })
            .collect(),
        environment_ids: row.environment_ids,
    }))
}

/// Serializes issuing for one owner and locks the owner row.
///
/// CONCURRENCY: The advisory lock uses key namespace 534 with
/// `hashtext(user id)`, which is distinct from the grant lock `(526, 2)`.
/// This function never takes `(526, 2)`. Lock order is: owner advisory lock,
/// owner row `FOR SHARE`, token rows `FOR UPDATE`, environment rows
/// `FOR SHARE`. Grant writers lock the user row `FOR UPDATE`, so they wait for
/// this transaction and role or membership changes cannot commit between the
/// checks and the insert. Returns the owner's `is_active`, or `None` when the
/// user does not exist.
///
/// # Errors
/// Returns database errors.
pub async fn lock_owner_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Option<bool>> {
    sqlx::query("SELECT pg_advisory_xact_lock(534, hashtext($1::text))")
        .bind(user_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(
        sqlx::query_scalar::<_, bool>("SELECT is_active FROM users WHERE id = $1 FOR SHARE")
            .bind(user_id)
            .fetch_optional(&mut **tx)
            .await?,
    )
}

/// Returns the active environments the owner may select, sorted by name.
///
/// An Admin sees every active environment. Other roles see active
/// environments they are assigned to.
///
/// # Errors
/// Returns database errors.
pub async fn selectable_environments<'e, E>(
    executor: E,
    user_id: Uuid,
    is_admin: bool,
) -> Result<Vec<(Uuid, String)>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as::<_, (Uuid, String)>(
        "SELECT e.id, e.name FROM environments e
         WHERE COALESCE(e.is_active, false)
           AND ($2 OR EXISTS (SELECT 1 FROM user_environment_memberships m
                              WHERE m.user_id = $1 AND m.environment_id = e.id))
         ORDER BY e.name, e.id",
    )
    .bind(user_id)
    .bind(is_admin)
    .fetch_all(executor)
    .await?)
}

/// Share-locks the named environments and returns `(id, name, is_active)`.
///
/// Missing ids are absent from the result. The share lock stops a concurrent
/// deactivation or deletion from committing before this transaction ends.
///
/// # Errors
/// Returns database errors.
pub async fn lock_environments_tx(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, String, bool)>> {
    Ok(sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT e.id, e.name, COALESCE(e.is_active, false) FROM environments e
         WHERE e.id = ANY($1) ORDER BY e.id FOR SHARE OF e",
    )
    .bind(ids)
    .fetch_all(&mut **tx)
    .await?)
}

/// Counts the owner's non-revoked, unexpired tokens.
///
/// # Errors
/// Returns database errors.
pub async fn count_live_tokens_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM api_access_tokens
         WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > clock_timestamp()",
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?)
}

/// Counts tokens the owner created in the last hour, including revoked ones.
///
/// A rotation inserts a token, so create and rotate share this count.
///
/// # Errors
/// Returns database errors.
pub async fn count_recent_issues_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM api_access_tokens
         WHERE user_id = $1 AND created_at > clock_timestamp() - interval '1 hour'",
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?)
}

/// Reports whether the owner has a non-revoked token with this name,
/// ignoring case.
///
/// # Errors
/// Returns database errors.
pub async fn name_in_use_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    name: &str,
) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM api_access_tokens
                        WHERE user_id = $1 AND revoked_at IS NULL
                          AND lower(name) = lower($2))",
    )
    .bind(user_id)
    .bind(name)
    .fetch_one(&mut **tx)
    .await?)
}

/// Inserts a token row and returns its `expires_at`.
///
/// `created_at` and `expires_at` come from the database clock.
/// `lifetime_seconds` is positive, so `expires_at > created_at`. The caller
/// stores `secret_hash` only; the plaintext never reaches this function.
///
/// # Errors
/// Returns database errors, including a unique violation when a non-revoked
/// token with the same owner and name exists.
pub async fn insert_token_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    user_id: Uuid,
    name: &str,
    secret_hash: &[u8],
    display_hint: &str,
    lifetime_seconds: f64,
) -> Result<DateTime<Utc>> {
    Ok(sqlx::query_scalar::<_, DateTime<Utc>>(
        "INSERT INTO api_access_tokens
             (id, user_id, name, audience, secret_hash, display_hint, created_at, expires_at)
         VALUES ($1, $2, $3, 'mcp', $4, $5, clock_timestamp(),
                 clock_timestamp() + make_interval(secs => $6))
         RETURNING expires_at",
    )
    .bind(id)
    .bind(user_id)
    .bind(name)
    .bind(secret_hash)
    .bind(display_hint)
    .bind(lifetime_seconds)
    .fetch_one(&mut **tx)
    .await?)
}

/// Inserts the immutable scope and environment grants of a new token.
///
/// # Errors
/// Returns database errors, including a check violation for a scope outside
/// the database allowlist.
pub async fn insert_grants_tx(
    tx: &mut Transaction<'_, Postgres>,
    token_id: Uuid,
    scopes: &[String],
    environment_ids: &[Uuid],
) -> Result<()> {
    sqlx::query(
        "INSERT INTO api_access_token_scopes (token_id, scope)
         SELECT $1, unnest($2::text[])",
    )
    .bind(token_id)
    .bind(scopes)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO api_access_token_environments (token_id, environment_id)
         SELECT $1, unnest($2::uuid[])",
    )
    .bind(token_id)
    .bind(environment_ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Holds a token row locked by [`lock_token_tx`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LockedToken {
    /// Display name.
    pub name: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Expiry time.
    pub expires_at: DateTime<Utc>,
    /// Revocation time.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Whether `expires_at` has passed on the database clock.
    pub expired: bool,
}

/// Locks one of the owner's token rows `FOR UPDATE`.
///
/// Returns `None` when the token does not exist or belongs to another user,
/// so the caller cannot distinguish the two cases.
///
/// # Errors
/// Returns database errors.
pub async fn lock_token_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    token_id: Uuid,
) -> Result<Option<LockedToken>> {
    Ok(sqlx::query_as::<_, LockedToken>(
        "SELECT name, created_at, expires_at, revoked_at,
                expires_at <= clock_timestamp() AS expired
         FROM api_access_tokens WHERE id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(token_id)
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?)
}

/// Marks a token revoked once. Returns `true` only for the transition.
///
/// INVARIANT: Revocation is irreversible. The `revoked_at IS NULL` guard keeps
/// the first revocation time and actor.
///
/// # Errors
/// Returns database errors.
pub async fn revoke_token_tx(
    tx: &mut Transaction<'_, Postgres>,
    token_id: Uuid,
    revoked_by: Uuid,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE api_access_tokens
         SET revoked_at = clock_timestamp(), revoked_by_user_id = $2
         WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(token_id)
    .bind(revoked_by)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Writes an access-token audit event in the caller's transaction.
///
/// `target` is the public token id in simple (32 hex) form and the actor
/// identifier is the actor's user id. The metadata MUST NOT contain a secret
/// or a hash.
///
/// # Errors
/// Returns database errors; the caller MUST roll back.
pub async fn audit_access_token_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    action: &str,
    token_id: Uuid,
    metadata: serde_json::Value,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO admin_audit_events
             (actor_user_id, actor_identifier, action, target, metadata)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(actor)
    .bind(actor.to_string())
    .bind(action)
    .bind(token_id.simple().to_string())
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Returns one of the owner's tokens, or `None` for a missing token or another
/// user's token.
///
/// # Errors
/// Returns database errors.
pub async fn get_token<'e, E>(
    executor: E,
    user_id: Uuid,
    token_id: Uuid,
) -> Result<Option<TokenRow>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as::<_, TokenRow>(&format!(
        "{TOKEN_SELECT} WHERE t.id = $1 AND t.user_id = $2"
    ))
    .bind(token_id)
    .bind(user_id)
    .fetch_optional(executor)
    .await?)
}

/// Returns one page of the owner's tokens, newest first.
///
/// `lifecycle` is `all`, `active` (not revoked and not expired), `expired`
/// (not revoked, expired) or `revoked`. It filters on the raw columns, so
/// `active` includes tokens whose derived status is `expiring_soon`.
/// `name_query` matches as a case-insensitive substring without wildcard
/// syntax.
///
/// # Errors
/// Returns database errors.
pub async fn list_tokens<'e, E>(
    executor: E,
    user_id: Uuid,
    lifecycle: &str,
    name_query: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<TokenRow>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as::<_, TokenRow>(&format!(
        "{TOKEN_SELECT}
         WHERE t.user_id = $1
           AND ($3::text IS NULL OR position(lower($3) in lower(t.name)) > 0)
           AND ($2 = 'all'
                OR ($2 = 'active' AND t.revoked_at IS NULL
                    AND t.expires_at > statement_timestamp())
                OR ($2 = 'expired' AND t.revoked_at IS NULL
                    AND t.expires_at <= statement_timestamp())
                OR ($2 = 'revoked' AND t.revoked_at IS NOT NULL))
         ORDER BY t.created_at DESC, t.id DESC LIMIT $4 OFFSET $5"
    ))
    .bind(user_id)
    .bind(lifecycle)
    .bind(name_query)
    .bind(limit)
    .bind(offset)
    .fetch_all(executor)
    .await?)
}

/// Returns `(token id, scope)` pairs for the given tokens, sorted by scope.
///
/// # Errors
/// Returns database errors.
pub async fn token_scopes<'e, E>(executor: E, token_ids: &[Uuid]) -> Result<Vec<(Uuid, String)>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as::<_, (Uuid, String)>(
        "SELECT token_id, scope FROM api_access_token_scopes
         WHERE token_id = ANY($1) ORDER BY token_id, scope",
    )
    .bind(token_ids)
    .fetch_all(executor)
    .await?)
}

/// Returns granted environments for the given tokens, sorted by name.
///
/// # Errors
/// Returns database errors.
pub async fn token_environments<'e, E>(
    executor: E,
    token_ids: &[Uuid],
) -> Result<Vec<TokenEnvironmentRow>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as::<_, TokenEnvironmentRow>(
        "SELECT te.token_id, e.id, e.name, COALESCE(e.is_active, false) AS is_active
         FROM api_access_token_environments te
         JOIN environments e ON e.id = te.environment_id
         WHERE te.token_id = ANY($1) ORDER BY e.name, e.id",
    )
    .bind(token_ids)
    .fetch_all(executor)
    .await?)
}

/// Groups scope pairs by token id.
pub fn group_scopes(pairs: Vec<(Uuid, String)>) -> HashMap<Uuid, Vec<String>> {
    let mut grouped: HashMap<Uuid, Vec<String>> = HashMap::new();
    for (token_id, scope) in pairs {
        grouped.entry(token_id).or_default().push(scope);
    }
    grouped
}

/// Returns up to `limit` access-token audit events that concern a token,
/// newest first.
///
/// An event concerns a token when its target is the token's public id or its
/// metadata names the token as `replaced_token_id`. Only the action and time
/// are returned.
///
/// # Errors
/// Returns database errors.
pub async fn recent_events<'e, E>(
    executor: E,
    token_id: Uuid,
    limit: i64,
) -> Result<Vec<(String, DateTime<Utc>)>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT action, created_at FROM admin_audit_events
         WHERE action IN ('access_token_created', 'access_token_rotated', 'access_token_revoked')
           AND (target = $1 OR metadata ->> 'replaced_token_id' = $1)
         ORDER BY created_at DESC, id DESC LIMIT $2",
    )
    .bind(token_id.simple().to_string())
    .bind(limit)
    .fetch_all(executor)
    .await?)
}

/// Holds the columns that token validation needs.
///
/// SECURITY: `secret_hash` is a credential verifier. Compare it only with
/// `crate::auth::access_token::hashes_equal` and never log or serialize it.
#[derive(sqlx::FromRow)]
pub struct TokenAuthRow {
    /// Public token id.
    pub id: Uuid,
    /// Owner user id.
    pub user_id: Uuid,
    /// SHA-256 of the canonical token.
    pub secret_hash: Vec<u8>,
    /// Audience.
    pub audience: String,
    /// Expiry time.
    pub expires_at: DateTime<Utc>,
    /// Revocation time.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Whether the owner account is enabled.
    pub owner_active: bool,
}

/// Looks up the validation columns by public token id.
///
/// # Errors
/// Returns database errors.
pub async fn find_token_for_validation<'e, E>(
    executor: E,
    token_id: Uuid,
) -> Result<Option<TokenAuthRow>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as::<_, TokenAuthRow>(
        "SELECT t.id, t.user_id, t.secret_hash, t.audience, t.expires_at, t.revoked_at,
                u.is_active AS owner_active
         FROM api_access_tokens t JOIN users u ON u.id = t.user_id
         WHERE t.id = $1",
    )
    .bind(token_id)
    .fetch_optional(executor)
    .await?)
}

/// Returns the granted environment ids of one token.
///
/// # Errors
/// Returns database errors.
pub async fn token_environment_ids<'e, E>(executor: E, token_id: Uuid) -> Result<Vec<Uuid>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar::<_, Uuid>(
        "SELECT environment_id FROM api_access_token_environments
         WHERE token_id = $1 ORDER BY environment_id",
    )
    .bind(token_id)
    .fetch_all(executor)
    .await?)
}

/// Returns the granted scopes of one token.
///
/// # Errors
/// Returns database errors.
pub async fn token_scope_names<'e, E>(executor: E, token_id: Uuid) -> Result<Vec<String>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT scope FROM api_access_token_scopes WHERE token_id = $1 ORDER BY scope",
    )
    .bind(token_id)
    .fetch_all(executor)
    .await?)
}

/// Holds one linked external identity of an owner.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OwnerIdentity {
    /// Identity row id.
    pub id: Uuid,
    /// Provider key, for example `oidc`.
    pub provider_key: String,
    /// Entitlement sync state, for example `valid` or `legacy_unresolved`.
    pub entitlement_sync_state: String,
}

/// Lists every external identity linked to the owner.
///
/// # Errors
/// Returns database errors.
pub async fn owner_identities<'e, E>(executor: E, user_id: Uuid) -> Result<Vec<OwnerIdentity>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as::<_, OwnerIdentity>(
        "SELECT id, provider_key::text AS provider_key, entitlement_sync_state
         FROM external_identities WHERE user_id = $1 ORDER BY id",
    )
    .bind(user_id)
    .fetch_all(executor)
    .await?)
}

/// Records a coarse last-use time when the stored value is older than the
/// throttle. Returns `true` when the row was updated.
///
/// PERFORMANCE: The `WHERE` guard makes most calls update nothing, so
/// concurrent requests do not contend for a hot row. `last_used_at` is
/// informational and MUST NOT influence an authorization decision.
///
/// # Errors
/// Returns database errors.
pub async fn touch_last_used<'e, E>(
    executor: E,
    token_id: Uuid,
    throttle_seconds: f64,
) -> Result<bool>
where
    E: Executor<'e, Database = Postgres>,
{
    let result = sqlx::query(
        "UPDATE api_access_tokens SET last_used_at = clock_timestamp()
         WHERE id = $1
           AND (last_used_at IS NULL
                OR last_used_at < clock_timestamp() - make_interval(secs => $2))",
    )
    .bind(token_id)
    .bind(throttle_seconds)
    .execute(executor)
    .await?;
    Ok(result.rows_affected() == 1)
}
