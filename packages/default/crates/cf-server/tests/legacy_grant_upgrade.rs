//! Rehearses the one-time legacy grant upgrade and its recovery procedure.
//!
//! Each test migrates its own SQLx database through 0334, seeds a populated
//! pre-0335 account set with raw SQL, then applies the remaining repository
//! migrations unchanged. The tests prove the upgrade preserves effective
//! grants and browser sessions, and that ownership of grants of OIDC-linked
//! users stays unresolved until an administrator classifies it. Recovery uses
//! the real Admin PATCH handler and the real OIDC reconciliation entry point.
//! No test fabricates freshness or source state after migration 0335.

use anyhow::{Result, ensure};
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    routing::patch,
};
use crystal_forge::auth::session::{
    CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
};
use crystal_forge::handlers::api::admin::update_user;
use crystal_forge::handlers::api::rbac::authenticated_user_roles;
use crystal_forge::queries::oidc_entitlements::{
    ReconciliationOutcome, reconcile, require_fresh_entitlements, reserve_entitlement_generation,
};
use serde_json::json;
use sqlx::{PgPool, migrate::Migrate};
use tower::ServiceExt;
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The last migration before source tracking (0335) exists.
const LAST_PRE_SOURCE_MIGRATION: i64 = 334;

/// The default positive entitlement age, in seconds.
const MAX_AGE_SECONDS: u64 = 86_400;

async fn apply_migrations(pool: &PgPool, select: impl Fn(i64) -> bool) -> Result<()> {
    let mut connection = pool.acquire().await?;
    connection.ensure_migrations_table().await?;
    for migration in MIGRATOR.iter().filter(|m| select(m.version)) {
        connection.apply(migration).await?;
    }
    Ok(())
}

/// Identifies the seeded pre-0335 population.
struct Population {
    /// Local-only Admin without an external identity; the recovery actor.
    local_admin: Uuid,
    local_admin_session: String,
    /// OIDC-linked Operator whose grants the rehearsal classifies.
    oidc_operator: Uuid,
    oidc_operator_identity: Uuid,
    oidc_operator_session: String,
    /// OIDC-linked Admin left unresolved for the whole rehearsal.
    oidc_admin: Uuid,
    oidc_admin_identity: Uuid,
    oidc_admin_session: String,
    /// User linked to two external identities.
    two_identity: Uuid,
    two_identity_first: Uuid,
    two_identity_second: Uuid,
    two_identity_session: String,
    /// Disabled OIDC-linked user that still holds grants.
    inactive: Uuid,
    inactive_identity: Uuid,
    inactive_session: String,
}

impl Population {
    fn users(&self) -> [Uuid; 5] {
        [
            self.local_admin,
            self.oidc_operator,
            self.oidc_admin,
            self.two_identity,
            self.inactive,
        ]
    }

    fn sessions(&self) -> [&str; 5] {
        [
            &self.local_admin_session,
            &self.oidc_operator_session,
            &self.oidc_admin_session,
            &self.two_identity_session,
            &self.inactive_session,
        ]
    }
}

async fn legacy_user(pool: &PgPool, label: &str, active: bool) -> Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, first_name, last_name, is_active)
         VALUES ($1, $2, $3, 'Legacy', 'Rehearsal', $4)",
    )
    .bind(id)
    .bind(format!("{label}-{id}"))
    .bind(format!("{id}@example.invalid"))
    .bind(active)
    .execute(pool)
    .await?;
    Ok(id)
}

async fn link(pool: &PgPool, user_id: Uuid, issuer: &str) -> Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO external_identities (user_id, provider_key, subject, tenant_discriminator, claims)
         VALUES ($1, 'oidc', $2, $3, '{\"pre_0335\": true}') RETURNING id",
    )
    .bind(user_id)
    .bind(format!("subject-{user_id}-{issuer}"))
    .bind(issuer)
    .fetch_one(pool)
    .await?)
}

async fn grant_role(pool: &PgPool, user_id: Uuid, role: &str, by: Option<Uuid>) -> Result<()> {
    sqlx::query(
        "INSERT INTO user_role_assignments (user_id, role, granted_by_user_id)
         VALUES ($1, $2::auth_role, $3)",
    )
    .bind(user_id)
    .bind(role)
    .bind(by)
    .execute(pool)
    .await?;
    Ok(())
}

async fn grant_environment(
    pool: &PgPool,
    user_id: Uuid,
    environment: &str,
    by: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO user_environment_memberships (user_id, environment_id, assigned_by_user_id)
         SELECT $1, id, $3 FROM environments WHERE name = $2",
    )
    .bind(user_id)
    .bind(environment)
    .bind(by)
    .execute(pool)
    .await?;
    Ok(())
}

// Pre-0335 browser sessions. The upgrade must not invalidate them.
async fn legacy_session(pool: &PgPool, user_id: Uuid, source: &str) -> Result<String> {
    let token = format!("legacy-session-{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO user_sessions (user_id, session_token_hash, expires_at, auth_source)
         VALUES ($1, $2, NOW() + INTERVAL '1 hour', $3)",
    )
    .bind(user_id)
    .bind(hash_token(&token))
    .bind(source)
    .execute(pool)
    .await?;
    Ok(token)
}

/// Migrates through 0334 and seeds the pre-0335 population with raw SQL.
///
/// Uses only pre-0335 columns, so every grant takes the column defaults that
/// 0335 must then classify.
async fn seed_pre_source_schema(pool: &PgPool) -> Result<Population> {
    ensure!(
        MIGRATOR.iter().any(|m| m.version == 335),
        "the rehearsal requires migration 0335"
    );
    apply_migrations(pool, |version| version <= LAST_PRE_SOURCE_MIGRATION).await?;
    sqlx::query(
        "INSERT INTO environments (name)
         VALUES ('rehearsal-alpha'), ('rehearsal-beta'), ('rehearsal-gamma')",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO oidc_group_mappings (group_name, role, environments) VALUES
         ('rehearsal-admins', 'admin', ARRAY['rehearsal-alpha']),
         ('rehearsal-operators', 'operator', ARRAY['rehearsal-gamma'])",
    )
    .execute(pool)
    .await?;

    let local_admin = legacy_user(pool, "local-admin", true).await?;
    grant_role(pool, local_admin, "admin", None).await?;
    grant_environment(pool, local_admin, "rehearsal-alpha", None).await?;
    grant_environment(pool, local_admin, "rehearsal-beta", None).await?;

    let oidc_operator = legacy_user(pool, "oidc-operator", true).await?;
    let oidc_operator_identity = link(pool, oidc_operator, "https://idp.example.invalid").await?;
    grant_role(pool, oidc_operator, "operator", Some(local_admin)).await?;
    grant_environment(pool, oidc_operator, "rehearsal-alpha", Some(local_admin)).await?;

    // Null actor attribution, as left by the pre-0335 OIDC role writer.
    let oidc_admin = legacy_user(pool, "oidc-admin", true).await?;
    let oidc_admin_identity = link(pool, oidc_admin, "https://idp.example.invalid").await?;
    grant_role(pool, oidc_admin, "admin", None).await?;
    grant_environment(pool, oidc_admin, "rehearsal-alpha", None).await?;
    grant_environment(pool, oidc_admin, "rehearsal-beta", None).await?;

    let two_identity = legacy_user(pool, "two-identity", true).await?;
    let two_identity_first = link(pool, two_identity, "https://idp.example.invalid").await?;
    let two_identity_second = link(pool, two_identity, "https://other-idp.example.invalid").await?;
    grant_role(pool, two_identity, "viewer", Some(local_admin)).await?;
    grant_environment(pool, two_identity, "rehearsal-beta", Some(local_admin)).await?;

    let inactive = legacy_user(pool, "inactive", false).await?;
    let inactive_identity = link(pool, inactive, "https://idp.example.invalid").await?;
    grant_role(pool, inactive, "operator", Some(local_admin)).await?;
    grant_environment(pool, inactive, "rehearsal-gamma", Some(local_admin)).await?;

    Ok(Population {
        local_admin_session: legacy_session(pool, local_admin, "local").await?,
        oidc_operator_session: legacy_session(pool, oidc_operator, "oidc").await?,
        oidc_admin_session: legacy_session(pool, oidc_admin, "oidc").await?,
        two_identity_session: legacy_session(pool, two_identity, "oidc").await?,
        inactive_session: legacy_session(pool, inactive, "oidc").await?,
        local_admin,
        oidc_operator,
        oidc_operator_identity,
        oidc_admin,
        oidc_admin_identity,
        two_identity,
        two_identity_first,
        two_identity_second,
        inactive,
        inactive_identity,
    })
}

/// Applies 0335 and every later repository migration, unchanged.
async fn upgrade(pool: &PgPool) -> Result<()> {
    apply_migrations(pool, |version| version > LAST_PRE_SOURCE_MIGRATION).await
}

async fn upgraded(pool: &PgPool) -> Result<Population> {
    let population = seed_pre_source_schema(pool).await?;
    upgrade(pool).await?;
    Ok(population)
}

// Effective grants with original row IDs, actor attribution and timestamps.
// Reads only pre-0335 columns, so the same query runs before and after.
async fn effective_grants(pool: &PgPool, user_id: Uuid) -> Result<serde_json::Value> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(
           'roles', (SELECT coalesce(jsonb_agg(jsonb_build_object(
                       'id', id, 'role', role, 'granted_by', granted_by_user_id,
                       'created_at', created_at) ORDER BY role::text), '[]'::jsonb)
                     FROM user_role_assignments WHERE user_id = $1),
           'memberships', (SELECT coalesce(jsonb_agg(jsonb_build_object(
                       'id', m.id, 'environment', e.name, 'assigned_by', m.assigned_by_user_id,
                       'created_at', m.created_at) ORDER BY e.name), '[]'::jsonb)
                     FROM user_environment_memberships m
                     JOIN environments e ON e.id = m.environment_id
                     WHERE m.user_id = $1))",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?)
}

// Resolves roles through the browser session resolver used by every
// session-authenticated API guard. `None` means the session does not log in.
async fn session_roles(pool: &PgPool, token: &str) -> Result<Option<(Uuid, Vec<String>)>> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("{SESSION_COOKIE_NAME}={token}"))?,
    );
    Ok(authenticated_user_roles(pool, &headers)
        .await
        .map(|(user_id, roles)| {
            let mut roles: Vec<String> = roles.iter().map(|role| format!("{role:?}")).collect();
            roles.sort();
            (user_id, roles)
        }))
}

type Sources = Vec<(String, bool, Option<Uuid>, bool)>;

async fn role_sources(pool: &PgPool, user_id: Uuid) -> Result<Sources> {
    Ok(sqlx::query_as(
        "SELECT role::text, local_grant, oidc_identity_id, legacy_ownership_unresolved
         FROM user_role_assignments WHERE user_id = $1 ORDER BY role::text",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

async fn membership_sources(pool: &PgPool, user_id: Uuid) -> Result<Sources> {
    Ok(sqlx::query_as(
        "SELECT e.name::text, m.local_grant, m.oidc_identity_id, m.legacy_ownership_unresolved
         FROM user_environment_memberships m JOIN environments e ON e.id = m.environment_id
         WHERE m.user_id = $1 ORDER BY e.name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

async fn sync_state(pool: &PgPool, identity_id: Uuid) -> Result<(String, i64, bool)> {
    Ok(sqlx::query_as(
        "SELECT entitlement_sync_state, highest_accepted_entitlement_generation,
                last_successful_entitlement_sync_at IS NOT NULL
         FROM external_identities WHERE id = $1",
    )
    .bind(identity_id)
    .fetch_one(pool)
    .await?)
}

async fn is_fresh(pool: &PgPool, user_id: Uuid, identity_id: Uuid) -> bool {
    require_fresh_entitlements(pool, user_id, identity_id, MAX_AGE_SECONDS)
        .await
        .is_ok()
}

async fn login_sync(
    pool: &PgPool,
    user_id: Uuid,
    identity_id: Uuid,
    groups: Result<Vec<String>, String>,
) -> Result<ReconciliationOutcome> {
    // The callback reuses the generation reserved before its redirect.
    let generation = reserve_entitlement_generation(pool).await?;
    reconcile(
        pool,
        user_id,
        identity_id,
        generation,
        groups,
        &json!({"rehearsal": true}),
    )
    .await
}

fn unresolved(names: &[&str]) -> Sources {
    names
        .iter()
        .map(|name| (name.to_string(), false, None, true))
        .collect()
}

fn local(names: &[&str]) -> Sources {
    names
        .iter()
        .map(|name| (name.to_string(), true, None, false))
        .collect()
}

// The real Axum PATCH route with the session cookie and double-submit CSRF
// token that the source-aware Users editor sends.
async fn admin_patch(
    pool: &PgPool,
    actor_token: &str,
    target: Uuid,
    body: serde_json::Value,
) -> Result<(StatusCode, serde_json::Value)> {
    let app = Router::new()
        .route("/api/v1/admin/users/:id", patch(update_user))
        .with_state(pool.clone());
    let request = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/admin/users/{target}"))
        .header("content-type", "application/json")
        .header("x-real-ip", "192.0.2.77")
        .header(
            "cookie",
            format!("{SESSION_COOKIE_NAME}={actor_token}; {CSRF_COOKIE_NAME}=rehearsal-csrf"),
        )
        .header(CSRF_HEADER_NAME.as_str(), "rehearsal-csrf")
        .body(Body::from(body.to_string()))?;
    let response = app.oneshot(request).await?;
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((status, serde_json::from_slice(&body)?))
}

async fn audits_for(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<(String, Option<Uuid>, serde_json::Value)>> {
    Ok(sqlx::query_as(
        "SELECT action, actor_user_id, metadata FROM admin_audit_events
         WHERE target = $1 ORDER BY action",
    )
    .bind(format!("{user_id}@example.invalid ({user_id})"))
    .fetch_all(pool)
    .await?)
}

#[sqlx::test(migrations = false)]
async fn upgrade_preserves_effective_grants_and_browser_login(pool: PgPool) -> Result<()> {
    let population = seed_pre_source_schema(&pool).await?;
    let mut before = Vec::new();
    for (user_id, token) in population.users().into_iter().zip(population.sessions()) {
        before.push((
            effective_grants(&pool, user_id).await?,
            session_roles(&pool, token).await?,
        ));
    }
    upgrade(&pool).await?;

    for ((user_id, token), (grants, login)) in population
        .users()
        .into_iter()
        .zip(population.sessions())
        .zip(&before)
    {
        assert_eq!(
            &effective_grants(&pool, user_id).await?,
            grants,
            "{user_id}"
        );
        assert_eq!(&session_roles(&pool, token).await?, login, "{user_id}");
    }
    // Pin the expected browser login result, not only before/after equality.
    for (user_id, token, role) in [
        (
            population.local_admin,
            &population.local_admin_session,
            "Admin",
        ),
        (
            population.oidc_operator,
            &population.oidc_operator_session,
            "Operator",
        ),
        (
            population.oidc_admin,
            &population.oidc_admin_session,
            "Admin",
        ),
        (
            population.two_identity,
            &population.two_identity_session,
            "Viewer",
        ),
    ] {
        assert_eq!(
            session_roles(&pool, token).await?,
            Some((user_id, vec![role.to_string()]))
        );
    }
    // A disabled account stays unable to log in; the upgrade grants nothing.
    assert_eq!(
        session_roles(&pool, &population.inactive_session).await?,
        None
    );
    let grants = effective_grants(&pool, population.inactive).await?;
    assert_eq!(grants["roles"][0]["role"], "operator");
    assert_eq!(grants["memberships"][0]["environment"], "rehearsal-gamma");
    Ok(())
}

#[sqlx::test(migrations = false)]
async fn upgrade_classifies_sources_and_denies_oidc_linked_freshness(pool: PgPool) -> Result<()> {
    let population = upgraded(&pool).await?;

    assert_eq!(
        role_sources(&pool, population.local_admin).await?,
        local(&["admin"])
    );
    assert_eq!(
        membership_sources(&pool, population.local_admin).await?,
        local(&["rehearsal-alpha", "rehearsal-beta"])
    );
    for (user_id, roles, environments) in [
        (
            population.oidc_operator,
            "operator",
            &["rehearsal-alpha"][..],
        ),
        (
            population.oidc_admin,
            "admin",
            &["rehearsal-alpha", "rehearsal-beta"][..],
        ),
        (population.two_identity, "viewer", &["rehearsal-beta"][..]),
        (population.inactive, "operator", &["rehearsal-gamma"][..]),
    ] {
        assert_eq!(role_sources(&pool, user_id).await?, unresolved(&[roles]));
        assert_eq!(
            membership_sources(&pool, user_id).await?,
            unresolved(environments)
        );
    }

    for (user_id, identity_id) in [
        (population.oidc_operator, population.oidc_operator_identity),
        (population.oidc_admin, population.oidc_admin_identity),
        (population.two_identity, population.two_identity_first),
        (population.two_identity, population.two_identity_second),
        (population.inactive, population.inactive_identity),
    ] {
        assert_eq!(
            sync_state(&pool, identity_id).await?,
            ("legacy_unresolved".into(), 0, false)
        );
        assert!(!is_fresh(&pool, user_id, identity_id).await);
    }

    // OIDC callback login for an unresolved single-identity user continues
    // with its preserved grants, valid or invalid evidence, but never becomes
    // fresh and never stores the callback claims.
    let before = effective_grants(&pool, population.oidc_admin).await?;
    let evidence: [Result<Vec<String>, String>; 2] = [
        Ok(vec!["rehearsal-operators".into()]),
        Err("missing groups claim".into()),
    ];
    for groups in evidence {
        assert_eq!(
            login_sync(
                &pool,
                population.oidc_admin,
                population.oidc_admin_identity,
                groups
            )
            .await?,
            ReconciliationOutcome::LegacyUnresolved
        );
        assert_eq!(
            effective_grants(&pool, population.oidc_admin).await?,
            before
        );
        assert_eq!(
            sync_state(&pool, population.oidc_admin_identity).await?.0,
            "legacy_unresolved"
        );
        assert!(!is_fresh(&pool, population.oidc_admin, population.oidc_admin_identity).await);
    }
    let claims: serde_json::Value =
        sqlx::query_scalar("SELECT claims FROM external_identities WHERE id = $1")
            .bind(population.oidc_admin_identity)
            .fetch_one(&pool)
            .await?;
    assert_eq!(claims, json!({"pre_0335": true}));

    // A disabled owner cannot reconcile; its legacy grants stay unchanged.
    let before = effective_grants(&pool, population.inactive).await?;
    assert!(
        login_sync(
            &pool,
            population.inactive,
            population.inactive_identity,
            Ok(vec!["rehearsal-admins".into()])
        )
        .await
        .is_err()
    );
    assert_eq!(effective_grants(&pool, population.inactive).await?, before);
    assert!(!is_fresh(&pool, population.inactive, population.inactive_identity).await);
    Ok(())
}

#[sqlx::test(migrations = false)]
async fn recovery_classifies_local_overrides_then_idp_sync_and_removal_touch_only_managed_grants(
    pool: PgPool,
) -> Result<()> {
    let population = upgraded(&pool).await?;
    let operator = population.oidc_operator;
    let identity = population.oidc_operator_identity;
    let admin_token = &population.local_admin_session;
    let oidc_admin_before = effective_grants(&pool, population.oidc_admin).await?;

    // An old cached client without the explicit marker cannot classify.
    let before = effective_grants(&pool, operator).await?;
    let (status, body) = admin_patch(
        &pool,
        admin_token,
        operator,
        json!({"role": "Operator", "environments": ["rehearsal-alpha"]}),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(effective_grants(&pool, operator).await?, before);
    assert_eq!(
        role_sources(&pool, operator).await?,
        unresolved(&["operator"])
    );
    assert!(audits_for(&pool, operator).await?.is_empty());

    // Step 4 of the upgrade procedure: classify both categories as local
    // overrides through the source-aware editor's PATCH, using the
    // pre-upgrade local Admin session.
    let (status, body) = admin_patch(
        &pool,
        admin_token,
        operator,
        json!({
            "edit_local_grants": true,
            "role": "Operator",
            "environments": ["rehearsal-alpha"],
        }),
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["role"], "Operator");
    assert_eq!(body["environments"], json!(["rehearsal-alpha"]));
    assert_eq!(role_sources(&pool, operator).await?, local(&["operator"]));
    assert_eq!(
        membership_sources(&pool, operator).await?,
        local(&["rehearsal-alpha"])
    );
    let grants = effective_grants(&pool, operator).await?;
    assert_eq!(
        grants["roles"][0]["granted_by"],
        json!(population.local_admin)
    );
    assert_eq!(
        grants["memberships"][0]["assigned_by"],
        json!(population.local_admin)
    );

    let audits = audits_for(&pool, operator).await?;
    let actions: Vec<&str> = audits.iter().map(|(action, ..)| action.as_str()).collect();
    assert_eq!(
        actions,
        [
            "user_environment_membership_updated",
            "user_role_assigned",
            "user_updated"
        ]
    );
    let actor_email = format!("{}@example.invalid", population.local_admin);
    for (_, actor, metadata) in &audits {
        assert_eq!(*actor, Some(population.local_admin));
        assert_eq!(metadata["actor"], actor_email);
    }
    assert_eq!(
        audits[0].2["changes"],
        json!({
            "from": ["rehearsal-alpha"], "local_environments": ["rehearsal-alpha"],
            "classifies_legacy_environments": true,
        })
    );
    assert_eq!(
        audits[1].2["changes"],
        json!({"from": "Operator", "local_role": "Operator", "classifies_legacy_roles": true})
    );
    assert_eq!(audits[2].2["change_count"], 2);

    // Classification is not an entitlement sync: the identity stays non-fresh
    // until a successful authoritative sign-in.
    assert_eq!(sync_state(&pool, identity).await?.0, "legacy_unresolved");
    assert!(!is_fresh(&pool, operator, identity).await);

    // Step 6: a fresh sign-in with authoritative groups adds the IdP source.
    assert_eq!(
        login_sync(
            &pool,
            operator,
            identity,
            Ok(vec![
                "Rehearsal-Admins".into(),
                " rehearsal-operators ".into()
            ])
        )
        .await?,
        ReconciliationOutcome::Reconciled
    );
    let (state, generation, successful) = sync_state(&pool, identity).await?;
    assert_eq!((state.as_str(), successful), ("valid", true));
    assert!(generation > 0);
    assert!(is_fresh(&pool, operator, identity).await);
    // The highest mapped role is IdP-owned; the local Operator override stays.
    assert_eq!(
        role_sources(&pool, operator).await?,
        [
            ("admin".into(), false, Some(identity), false),
            ("operator".into(), true, None, false),
        ]
    );
    assert_eq!(
        membership_sources(&pool, operator).await?,
        [
            ("rehearsal-alpha".into(), true, Some(identity), false),
            ("rehearsal-gamma".into(), false, Some(identity), false),
        ]
    );
    assert_eq!(
        session_roles(&pool, &population.oidc_operator_session).await?,
        Some((operator, vec!["Admin".into(), "Operator".into()]))
    );

    // Authoritative IdP removal: valid empty groups remove only the managed
    // contributions and leave an IdP-owned Viewer. Local overrides survive.
    assert_eq!(
        login_sync(&pool, operator, identity, Ok(vec![])).await?,
        ReconciliationOutcome::Reconciled
    );
    assert_eq!(
        role_sources(&pool, operator).await?,
        [
            ("operator".into(), true, None, false),
            ("viewer".into(), false, Some(identity), false),
        ]
    );
    assert_eq!(
        membership_sources(&pool, operator).await?,
        local(&["rehearsal-alpha"])
    );
    let grants = effective_grants(&pool, operator).await?;
    assert_eq!(
        grants["roles"][0]["granted_by"],
        json!(population.local_admin)
    );
    assert_eq!(
        grants["memberships"][0]["assigned_by"],
        json!(population.local_admin)
    );
    assert_eq!(
        session_roles(&pool, &population.oidc_operator_session).await?,
        Some((operator, vec!["Operator".into(), "Viewer".into()]))
    );
    assert!(is_fresh(&pool, operator, identity).await);

    // Recovery of one account does not classify another account.
    assert_eq!(
        effective_grants(&pool, population.oidc_admin).await?,
        oidc_admin_before
    );
    assert_eq!(
        role_sources(&pool, population.oidc_admin).await?,
        unresolved(&["admin"])
    );
    assert!(!is_fresh(&pool, population.oidc_admin, population.oidc_admin_identity).await);
    Ok(())
}

#[sqlx::test(migrations = false)]
async fn two_identity_user_stays_blocked_after_classification(pool: PgPool) -> Result<()> {
    let population = upgraded(&pool).await?;
    let user_id = population.two_identity;
    let first = population.two_identity_first;
    let second = population.two_identity_second;
    let before = effective_grants(&pool, user_id).await?;

    // Multiple linked identities fail closed even before classification.
    assert!(
        login_sync(&pool, user_id, first, Ok(vec!["rehearsal-admins".into()]))
            .await
            .is_err()
    );
    assert_eq!(effective_grants(&pool, user_id).await?, before);

    let (status, body) = admin_patch(
        &pool,
        &population.local_admin_session,
        user_id,
        json!({
            "edit_local_grants": true,
            "role": "Viewer",
            "environments": ["rehearsal-beta"],
        }),
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(role_sources(&pool, user_id).await?, local(&["viewer"]));
    assert_eq!(
        membership_sources(&pool, user_id).await?,
        local(&["rehearsal-beta"])
    );

    for identity_id in [first, second] {
        assert!(
            login_sync(
                &pool,
                user_id,
                identity_id,
                Ok(vec!["rehearsal-admins".into()])
            )
            .await
            .is_err()
        );
        assert_eq!(role_sources(&pool, user_id).await?, local(&["viewer"]));
        assert_eq!(
            membership_sources(&pool, user_id).await?,
            local(&["rehearsal-beta"])
        );
        let (_, _, successful) = sync_state(&pool, identity_id).await?;
        assert!(!successful);
    }
    for identity_id in [first, second] {
        assert_eq!(sync_state(&pool, identity_id).await?.0, "invalid");
        assert!(!is_fresh(&pool, user_id, identity_id).await);
    }
    // The browser session still logs in with the local override only.
    assert_eq!(
        session_roles(&pool, &population.two_identity_session).await?,
        Some((user_id, vec!["Viewer".into()]))
    );
    Ok(())
}
