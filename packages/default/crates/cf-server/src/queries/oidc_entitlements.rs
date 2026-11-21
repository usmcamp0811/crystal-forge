//! Reconciles validated OIDC evidence without replacing independent local grants.
//!
//! Writers lock the user before identities and assignments. A committed pending
//! generation denies freshness even if reconciliation is cancelled or rolls back.
//! Legacy ownership requires explicit classification before managed cutover.
//! Multiple linked identities, including non-OIDC providers, fail closed because
//! effective assignment rows can represent only one external ownership source.
//! Login initiation reserves a database-global generation before redirect. A
//! delayed callback cannot supersede evidence from a newer accepted login.

use crate::models::auth_identity::AuthRole;
use crate::queries::auth_identity::OidcMappingMatchRow;
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Describes whether authoritative evidence could replace the managed source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationOutcome {
    /// Grants, claims and successful freshness committed atomically.
    Reconciled,
    /// Existing grants remain intact; login may continue, but PAT eligibility
    /// remains blocked until an administrator classifies legacy ownership.
    LegacyUnresolved,
}

/// Reserves a globally ordered positive generation before an OIDC redirect.
///
/// The caller MUST retain this value only in the server-side OIDC flow session
/// and pass it unchanged to [`reconcile`]. Never reserve a replacement generation
/// in a callback: that would let delayed evidence overwrite newer evidence.
/// Sequence gaps from failed or abandoned logins are intentional.
///
/// # Errors
/// Returns an error if the database cannot reserve a generation, including
/// sequence exhaustion. Login must fail without redirecting in that case.
pub async fn reserve_entitlement_generation(pool: &PgPool) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT nextval('oidc_entitlement_generation_seq')")
            .fetch_one(pool)
            .await?,
    )
}

async fn lock_owner(tx: &mut Transaction<'_, Postgres>, user_id: Uuid) -> Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT is_active FROM users WHERE id = $1 FOR UPDATE")
            .bind(user_id)
            .fetch_one(&mut **tx)
            .await?,
    )
}

async fn unresolved(tx: &mut Transaction<'_, Postgres>, user_id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_role_assignments
                        WHERE user_id = $1 AND legacy_ownership_unresolved)
             OR EXISTS (SELECT 1 FROM user_environment_memberships
                        WHERE user_id = $1 AND legacy_ownership_unresolved)",
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?)
}

/// Replaces one bound identity's managed grants using validated group evidence.
///
/// The caller MUST authenticate the provider, subject and tenant before passing
/// the bound identity and validated groups. `Err` groups means missing or invalid
/// evidence, not an empty authoritative claim. Claims are stored only on success.
/// Valid evidence with no mapped role grants an OIDC-owned Viewer role; it never
/// retains a previous managed role. Empty mappings remove managed memberships.
///
/// Pending commits before work begins. Success replaces only this identity's
/// source and advances freshness in the same transaction. Legacy grants block
/// replacement without changing rights or the last successful timestamp.
/// Legacy ownership returns `LegacyUnresolved` even when group evidence is
/// missing or invalid, preserving existing login while PAT eligibility denies.
/// The identity must use provider key `oidc`. Multiple linked identities of any
/// provider fail closed to prevent replacement of another identity's source.
/// `generation` MUST be reserved at login initiation. Only a strictly newer,
/// positive generation can register pending; stale evidence, including invalid
/// evidence, cannot change identity state, claims, grants or freshness.
///
/// # Errors
/// Returns an error for invalid evidence after legacy cutover, inactive or
/// incorrectly bound users or providers,
/// multiple linked identities (unsupported by the single-source row model), a
/// superseding attempt, unknown/inactive mapped environments, or database errors.
/// Failure leaves pending or generation-fenced invalid state, never false fresh
/// state. An old failure cannot invalidate a newer successful generation.
///
/// # Examples
/// ```no_run
/// # async fn callback(pool: &sqlx::PgPool, user: uuid::Uuid, identity: uuid::Uuid,
/// # generation: i64, groups: Result<Vec<String>, String>, claims: &serde_json::Value) -> anyhow::Result<()> {
/// use crystal_forge::queries::oidc_entitlements::{reconcile, ReconciliationOutcome};
/// match reconcile(pool, user, identity, generation, groups, claims).await? {
///     ReconciliationOutcome::Reconciled => { /* Current mappings committed. */ }
///     ReconciliationOutcome::LegacyUnresolved => { /* Preserve login; deny PAT. */ }
/// }
/// # Ok(())
/// # }
/// ```
pub async fn reconcile(
    pool: &PgPool,
    user_id: Uuid,
    identity_id: Uuid,
    generation: i64,
    groups: Result<Vec<String>, String>,
    claims: &Value,
) -> Result<ReconciliationOutcome> {
    ensure!(
        generation > 0,
        "OIDC entitlement generation must be positive"
    );
    let attempt_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, user_id).await?;
    // SECURITY: Bind before any state write, including invalid evidence.
    let bound = sqlx::query(
        "UPDATE external_identities
         SET entitlement_sync_state = 'pending', entitlement_sync_attempt_id = $3,
             highest_accepted_entitlement_generation = $4
         WHERE id = $1 AND user_id = $2 AND provider_key = 'oidc'
           AND $4 > 0 AND highest_accepted_entitlement_generation < $4",
    )
    .bind(identity_id)
    .bind(user_id)
    .bind(attempt_id)
    .bind(generation)
    .execute(&mut *tx)
    .await?;
    // A rejected generation exits before cleanup too: stale invalid evidence
    // must not invalidate a newer attempt or its successful freshness stamp.
    ensure!(
        bound.rows_affected() == 1,
        "OIDC identity binding or generation rejected"
    );
    tx.commit().await?;

    let result = reconcile_pending(pool, user_id, identity_id, attempt_id, groups, claims).await;
    if result.is_err() {
        // CONCURRENCY: Serialize cleanup with the same user-first lock order.
        // Cancellation or cleanup failure leaves durable pending, which denies.
        let mut tx = pool.begin().await?;
        lock_owner(&mut tx, user_id).await?;
        sqlx::query(
            "UPDATE external_identities SET entitlement_sync_state = 'invalid'
             WHERE id = $1 AND user_id = $2 AND entitlement_sync_attempt_id = $3
               AND provider_key = 'oidc' AND entitlement_sync_state = 'pending'",
        )
        .bind(identity_id)
        .bind(user_id)
        .bind(attempt_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    result
}

async fn reconcile_pending(
    pool: &PgPool,
    user_id: Uuid,
    identity_id: Uuid,
    attempt_id: Uuid,
    groups: Result<Vec<String>, String>,
    claims: &Value,
) -> Result<ReconciliationOutcome> {
    let mut tx = pool.begin().await?;
    // CONCURRENCY: Match guarded admin mutations: advisory lock before user.
    // Pending registration and cleanup never change roles and need only user.
    crate::queries::admin::lock_admin_grant_changes(&mut tx).await?;
    ensure!(lock_owner(&mut tx, user_id).await?, "OIDC user is inactive");
    let current: Option<Uuid> = sqlx::query_scalar(
        "SELECT entitlement_sync_attempt_id FROM external_identities
         WHERE id = $1 AND user_id = $2 AND provider_key = 'oidc' FOR UPDATE",
    )
    .bind(identity_id)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        current == Some(attempt_id),
        "OIDC reconciliation was superseded"
    );
    // SECURITY: Count every linked provider, not only OIDC. The row model cannot
    // safely replace a second external identity's overlapping managed source.
    let identities: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM external_identities WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    ensure!(
        identities == 1,
        "multiple linked external identities are unsupported"
    );
    // COMPATIBILITY: Legacy login preserves unclassified grants even without
    // strict group evidence. It never certifies those grants as fresh for PATs.
    if unresolved(&mut tx, user_id).await? {
        sqlx::query(
            "UPDATE external_identities SET entitlement_sync_state = 'legacy_unresolved'
             WHERE id = $1 AND user_id = $2 AND provider_key = 'oidc'",
        )
        .bind(identity_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(ReconciliationOutcome::LegacyUnresolved);
    }
    let groups = match groups {
        Ok(groups) => groups,
        Err(_) => bail!("missing or invalid OIDC group evidence"),
    };

    let groups: Vec<String> = groups
        .iter()
        .map(|g| g.trim().to_ascii_lowercase())
        .collect();
    // Lock mapping rows while resolving their environment names. Normalize both
    // sides in Rust to retain the existing ASCII case-folding contract.
    let mappings = sqlx::query_as::<_, (String, Option<AuthRole>, Vec<String>)>(
        "SELECT group_name, role, environments FROM oidc_group_mappings FOR SHARE",
    )
    .fetch_all(&mut *tx)
    .await?;
    let mappings: Vec<OidcMappingMatchRow> = mappings
        .into_iter()
        .filter(|(name, _, _)| groups.contains(&name.trim().to_ascii_lowercase()))
        .map(|(_, role, environments)| OidcMappingMatchRow { role, environments })
        .collect();
    let role = if mappings.iter().any(|m| m.role == Some(AuthRole::Admin)) {
        AuthRole::Admin
    } else if mappings.iter().any(|m| m.role == Some(AuthRole::Operator)) {
        AuthRole::Operator
    } else {
        AuthRole::Viewer
    };
    let mut names: Vec<String> = mappings
        .into_iter()
        .flat_map(|m| m.environments)
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names.dedup();
    let environments = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, name FROM environments WHERE name = ANY($1) AND is_active = true FOR SHARE",
    )
    .bind(&names)
    .fetch_all(&mut *tx)
    .await?;
    ensure!(
        environments.len() == names.len(),
        "unknown or inactive mapped OIDC environment"
    );

    // Remove only this managed contribution; rows with local ownership survive.
    // SECURITY: Authoritative IdP revocation also removes the last managed
    // Admin. Recovery requires an explicit local/bootstrap administrator grant;
    // stale IdP privilege retention is never a recovery mechanism.
    for table in ["user_role_assignments", "user_environment_memberships"] {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE user_id = $1 AND oidc_identity_id = $2
             AND NOT local_grant AND NOT legacy_ownership_unresolved"
        ))
        .bind(user_id)
        .bind(identity_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(&format!(
            "UPDATE {table} SET oidc_identity_id = NULL
             WHERE user_id = $1 AND oidc_identity_id = $2"
        ))
        .bind(user_id)
        .bind(identity_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO user_role_assignments (user_id, role, local_grant, oidc_identity_id)
         VALUES ($1, $2, false, $3)
         ON CONFLICT (user_id, role) DO UPDATE SET oidc_identity_id = EXCLUDED.oidc_identity_id",
    )
    .bind(user_id)
    .bind(role)
    .bind(identity_id)
    .execute(&mut *tx)
    .await?;
    for (environment_id, _) in environments {
        sqlx::query(
            "INSERT INTO user_environment_memberships
             (user_id, environment_id, local_grant, oidc_identity_id)
             VALUES ($1, $2, false, $3)
             ON CONFLICT (user_id, environment_id)
             DO UPDATE SET oidc_identity_id = EXCLUDED.oidc_identity_id",
        )
        .bind(user_id)
        .bind(environment_id)
        .bind(identity_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE external_identities SET entitlement_sync_state = 'valid',
         last_successful_entitlement_sync_at = clock_timestamp(), claims = $3
         WHERE id = $1 AND user_id = $2 AND provider_key = 'oidc'",
    )
    .bind(identity_id)
    .bind(user_id)
    .bind(claims)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(ReconciliationOutcome::Reconciled)
}

/// Requires fresh authoritative entitlements for the active, bound OIDC owner.
///
/// Uses database time and a strict age boundary: equality is stale. Zero maximum
/// age denies. The bound identity must use provider key `oidc`. Pending, missing,
/// invalid, legacy ownership and multiple linked identities of any provider deny.
/// This is an eligibility check, not a bearer-route admission
/// transaction; future request handlers must serialize their own authorization.
///
/// # Errors
/// Returns an error when eligibility fails or the database query fails.
pub async fn require_fresh_entitlements(
    pool: &PgPool,
    user_id: Uuid,
    identity_id: Uuid,
    max_age_seconds: u64,
) -> Result<()> {
    ensure!(
        max_age_seconds > 0,
        "OIDC maximum entitlement age must be positive"
    );
    let fresh: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM external_identities identity JOIN users owner ON owner.id = identity.user_id
            WHERE identity.id = $1 AND identity.user_id = $2 AND owner.is_active
              AND identity.provider_key = 'oidc'
              AND identity.entitlement_sync_state = 'valid'
              AND identity.last_successful_entitlement_sync_at >
                  statement_timestamp() - make_interval(secs => $3)
              AND identity.last_successful_entitlement_sync_at <= statement_timestamp()
              AND (SELECT COUNT(*) FROM external_identities WHERE user_id = $2) = 1
              AND NOT EXISTS (SELECT 1 FROM user_role_assignments
                              WHERE user_id = $2 AND legacy_ownership_unresolved)
              AND NOT EXISTS (SELECT 1 FROM user_environment_memberships
                              WHERE user_id = $2 AND legacy_ownership_unresolved))",
    )
    .bind(identity_id)
    .bind(user_id)
    .bind(max_age_seconds as f64)
    .fetch_one(pool)
    .await?;
    ensure!(fresh, "OIDC entitlements are not fresh");
    Ok(())
}
