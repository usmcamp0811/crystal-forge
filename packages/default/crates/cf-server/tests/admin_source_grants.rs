//! Proves administrator classification preserves the effective source union.
//! SQLx supplies isolated databases with the full production migrations.

use anyhow::Result;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::patch,
};
use crystal_forge::auth::session::{
    CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
};
use crystal_forge::handlers::api::admin::update_user;
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::queries::admin::{
    GuardedMutationOutcome, delete_user_with_admin_guard, disable_user_with_admin_guard,
    list_user_environment_names, replace_user_environment_memberships,
    replace_user_primary_role_with_admin_guard,
};
use crystal_forge::queries::oidc_entitlements::{
    ReconciliationOutcome, reconcile, reserve_entitlement_generation,
};
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn user(pool: &PgPool) -> Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, first_name, last_name)
         VALUES ($1, $2, $3, 'Admin', 'Regression')",
    )
    .bind(id)
    .bind(format!("admin-{id}"))
    .bind(format!("{id}@example.invalid"))
    .execute(pool)
    .await?;
    Ok(id)
}

async fn fixture(pool: &PgPool) -> Result<(Uuid, Uuid, Uuid, Uuid)> {
    let owner = user(pool).await?;
    let actor = user(pool).await?;
    let identity = sqlx::query_scalar(
        "INSERT INTO external_identities (user_id, provider_key, subject)
         VALUES ($1, 'oidc', $2) RETURNING id",
    )
    .bind(owner)
    .bind(owner.to_string())
    .fetch_one(pool)
    .await?;
    let environment =
        sqlx::query_scalar("INSERT INTO environments (name) VALUES ('admin-source') RETURNING id")
            .fetch_one(pool)
            .await?;
    sqlx::query(
        "INSERT INTO oidc_group_mappings (group_name, role, environments)
         VALUES ('admin-source', 'admin', ARRAY['admin-source'])",
    )
    .execute(pool)
    .await?;
    Ok((owner, actor, identity, environment))
}

async fn sync(pool: &PgPool, owner: Uuid, identity: Uuid) -> Result<()> {
    assert_eq!(
        reconcile(
            pool,
            owner,
            identity,
            reserve_entitlement_generation(pool).await?,
            Ok(vec!["admin-source".into()]),
            &json!({"groups": ["admin-source"]}),
        )
        .await?,
        ReconciliationOutcome::Reconciled
    );
    Ok(())
}

async fn role_sources(
    pool: &PgPool,
    owner: Uuid,
) -> Result<Vec<(String, bool, Option<Uuid>, bool)>> {
    Ok(sqlx::query_as(
        "SELECT role::text, local_grant, oidc_identity_id, legacy_ownership_unresolved
         FROM user_role_assignments WHERE user_id = $1 ORDER BY role::text",
    )
    .bind(owner)
    .fetch_all(pool)
    .await?)
}

async fn replace_role(pool: &PgPool, owner: Uuid, actor: Uuid, role: AuthRole) -> Result<()> {
    assert_eq!(
        replace_user_primary_role_with_admin_guard(pool, owner, role, actor, true).await?,
        GuardedMutationOutcome::Applied
    );
    Ok(())
}

#[sqlx::test]
async fn overlapping_managed_grants_survive_local_replacement(pool: PgPool) -> Result<()> {
    let (owner, actor, identity, _) = fixture(&pool).await?;
    sync(&pool, owner, identity).await?;
    replace_role(&pool, owner, actor, AuthRole::Admin).await?;
    replace_user_environment_memberships(&pool, owner, actor, &["admin-source".into()])
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        role_sources(&pool, owner).await?,
        [("admin".into(), true, Some(identity), false)]
    );
    // The only enabled Admin remains effective through its IdP source.
    replace_role(&pool, owner, actor, AuthRole::Viewer).await?;
    replace_user_environment_memberships(&pool, owner, actor, &[])
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        role_sources(&pool, owner).await?,
        [
            ("admin".into(), false, Some(identity), false),
            ("viewer".into(), true, None, false),
        ]
    );
    let membership: (bool, Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT local_grant, oidc_identity_id, assigned_by_user_id
         FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    assert_eq!(membership, (false, Some(identity), None));
    let attribution: Option<Uuid> = sqlx::query_scalar(
        "SELECT granted_by_user_id FROM user_role_assignments
         WHERE user_id = $1 AND role = 'admin'",
    )
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    assert_eq!(attribution, None);
    assert_eq!(
        list_user_environment_names(&pool, owner).await?,
        ["admin-source"]
    );
    Ok(())
}

async fn legacy(pool: &PgPool, owner: Uuid, environment: Uuid) -> Result<()> {
    sqlx::query(
        "INSERT INTO user_role_assignments
         (user_id, role, local_grant, legacy_ownership_unresolved)
         VALUES ($1, 'operator', false, true)",
    )
    .bind(owner)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO user_environment_memberships
         (user_id, environment_id, local_grant, legacy_ownership_unresolved)
         VALUES ($1, $2, false, true)",
    )
    .bind(owner)
    .bind(environment)
    .execute(pool)
    .await?;
    Ok(())
}

#[sqlx::test]
async fn explicit_empty_environments_classify_only_environment_legacy(pool: PgPool) -> Result<()> {
    let (owner, actor, _, environment) = fixture(&pool).await?;
    legacy(&pool, owner, environment).await?;
    replace_user_environment_memberships(&pool, owner, actor, &[])
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(list_user_environment_names(&pool, owner).await?.is_empty());
    assert_eq!(
        role_sources(&pool, owner).await?,
        [("operator".into(), false, None, true)]
    );
    replace_role(&pool, owner, actor, AuthRole::Viewer).await?;
    assert_eq!(
        role_sources(&pool, owner).await?,
        [("viewer".into(), true, None, false)]
    );
    Ok(())
}

#[sqlx::test]
async fn role_classification_leaves_legacy_environments_until_explicit_replacement(
    pool: PgPool,
) -> Result<()> {
    let (owner, actor, _, environment) = fixture(&pool).await?;
    legacy(&pool, owner, environment).await?;
    replace_role(&pool, owner, actor, AuthRole::Viewer).await?;
    let unresolved: bool = sqlx::query_scalar(
        "SELECT legacy_ownership_unresolved FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    assert!(unresolved);
    replace_user_environment_memberships(&pool, owner, actor, &["admin-source".into()])
        .await
        .map_err(anyhow::Error::msg)?;
    let classified: (bool, bool) = sqlx::query_as(
        "SELECT local_grant, legacy_ownership_unresolved
         FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    assert_eq!(classified, (true, false));
    Ok(())
}

#[sqlx::test]
async fn failed_replacements_roll_back_removal_and_classification(pool: PgPool) -> Result<()> {
    let (owner, actor, _, environment) = fixture(&pool).await?;
    legacy(&pool, owner, environment).await?;
    assert!(
        replace_user_environment_memberships(&pool, owner, actor, &["missing".into()])
            .await
            .is_err()
    );
    let membership: (bool, bool) = sqlx::query_as(
        "SELECT local_grant, legacy_ownership_unresolved FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(owner)
    .fetch_one(&pool)
    .await?;
    assert_eq!(membership, (false, true));
    // A bad actor fails after the old role has been removed inside the tx.
    assert!(
        replace_user_primary_role_with_admin_guard(
            &pool,
            owner,
            AuthRole::Viewer,
            Uuid::new_v4(),
            true,
        )
        .await
        .is_err()
    );
    assert_eq!(
        role_sources(&pool, owner).await?,
        [("operator".into(), false, None, true)]
    );
    Ok(())
}

#[sqlx::test]
async fn last_admin_guards_use_effective_union_and_rollback_legacy_removal(
    pool: PgPool,
) -> Result<()> {
    let (owner, actor, _, _) = fixture(&pool).await?;
    sqlx::query(
        "INSERT INTO user_role_assignments
         (user_id, role, local_grant, legacy_ownership_unresolved)
         VALUES ($1, 'admin', false, true)",
    )
    .bind(owner)
    .execute(&pool)
    .await?;
    assert_eq!(
        replace_user_primary_role_with_admin_guard(&pool, owner, AuthRole::Viewer, actor, true)
            .await?,
        GuardedMutationOutcome::GuardrailViolation
    );
    assert_eq!(
        role_sources(&pool, owner).await?,
        [("admin".into(), false, None, true)]
    );
    assert_eq!(
        disable_user_with_admin_guard(&pool, owner).await?,
        GuardedMutationOutcome::GuardrailViolation
    );
    assert_eq!(
        delete_user_with_admin_guard(&pool, owner).await?,
        GuardedMutationOutcome::GuardrailViolation
    );
    // A non-admin may still be disabled when only one enabled Admin exists.
    assert_eq!(
        disable_user_with_admin_guard(&pool, actor).await?,
        GuardedMutationOutcome::Applied
    );
    Ok(())
}

#[sqlx::test]
async fn concurrent_admin_demotions_cannot_remove_both_enabled_admins(pool: PgPool) -> Result<()> {
    let first = user(&pool).await?;
    let second = user(&pool).await?;
    replace_role(&pool, first, first, AuthRole::Admin).await?;
    replace_role(&pool, second, second, AuthRole::Admin).await?;
    let work = async {
        tokio::join!(
            replace_user_primary_role_with_admin_guard(
                &pool,
                first,
                AuthRole::Viewer,
                second,
                true,
            ),
            replace_user_primary_role_with_admin_guard(
                &pool,
                second,
                AuthRole::Viewer,
                first,
                true,
            ),
        )
    };
    let (first_result, second_result) =
        tokio::time::timeout(std::time::Duration::from_secs(15), work).await?;
    let results = [first_result?, second_result?];
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == GuardedMutationOutcome::Applied)
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == GuardedMutationOutcome::GuardrailViolation)
            .count(),
        1
    );
    Ok(())
}

#[sqlx::test]
async fn concurrent_sync_and_admin_replacement_preserve_independent_sources(
    pool: PgPool,
) -> Result<()> {
    let (owner, actor, identity, _) = fixture(&pool).await?;
    sync(&pool, owner, identity).await?;
    let work = async {
        let (managed, local) = tokio::join!(
            sync(&pool, owner, identity),
            replace_role(&pool, owner, actor, AuthRole::Viewer),
        );
        managed?;
        local?;
        let (managed, local) = tokio::join!(
            sync(&pool, owner, identity),
            replace_user_environment_memberships(&pool, owner, actor, &[]),
        );
        managed?;
        local.map_err(anyhow::Error::msg)?;
        Ok::<_, anyhow::Error>(())
    };
    tokio::time::timeout(std::time::Duration::from_secs(15), work).await??;
    assert_eq!(
        role_sources(&pool, owner).await?,
        [
            ("admin".into(), false, Some(identity), false),
            ("viewer".into(), true, None, false),
        ]
    );
    assert_eq!(
        list_user_environment_names(&pool, owner).await?,
        ["admin-source"]
    );
    Ok(())
}

#[sqlx::test]
async fn committed_managed_revocation_precedes_guarded_admin_mutations(pool: PgPool) -> Result<()> {
    let (owner, local_admin, identity, _) = fixture(&pool).await?;
    replace_role(&pool, local_admin, local_admin, AuthRole::Admin).await?;
    // Pause deletion inside reconciliation, after it holds the shared guard
    // lock. This separate barrier controls commit order without timing guesses.
    sqlx::raw_sql(
        "CREATE FUNCTION pause_managed_admin_revocation() RETURNS trigger AS $$
         BEGIN
             IF OLD.role = 'admin' AND OLD.oidc_identity_id IS NOT NULL THEN
                 PERFORM pg_advisory_xact_lock(526, 5262);
             END IF;
             RETURN OLD;
         END;
         $$ LANGUAGE plpgsql;
         CREATE TRIGGER pause_managed_admin_revocation
         BEFORE DELETE ON user_role_assignments
         FOR EACH ROW EXECUTE FUNCTION pause_managed_admin_revocation();",
    )
    .execute(&pool)
    .await?;

    for mutation in ["demote", "disable", "delete"] {
        sync(&pool, owner, identity).await?;
        let mut barrier = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(526, 5262)")
            .execute(&mut *barrier)
            .await?;
        let generation = reserve_entitlement_generation(&pool).await?;
        let revocation_pool = pool.clone();
        let revocation = tokio::spawn(async move {
            reconcile(
                &revocation_pool,
                owner,
                identity,
                generation,
                Ok(vec![]),
                &json!({"groups": []}),
            )
            .await
        });
        wait_for_advisory_waiter(&pool, 5262).await?;

        let mutation_pool = pool.clone();
        let guarded = tokio::spawn(async move {
            match mutation {
                "demote" => {
                    replace_user_primary_role_with_admin_guard(
                        &mutation_pool,
                        local_admin,
                        AuthRole::Viewer,
                        local_admin,
                        true,
                    )
                    .await
                }
                "disable" => disable_user_with_admin_guard(&mutation_pool, local_admin).await,
                "delete" => delete_user_with_admin_guard(&mutation_pool, local_admin).await,
                _ => unreachable!(),
            }
        });
        // Prove that the guarded mutation is queued behind reconciliation,
        // rather than merely starting it after revocation has already finished.
        wait_for_advisory_waiter(&pool, 2).await?;
        barrier.commit().await?;
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(15), revocation).await???,
            ReconciliationOutcome::Reconciled
        );
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(15), guarded).await???,
            GuardedMutationOutcome::GuardrailViolation,
            "{mutation} must count the committed managed revocation"
        );
        assert_eq!(
            role_sources(&pool, owner).await?,
            [("viewer".into(), false, Some(identity), false)]
        );
        assert_eq!(
            role_sources(&pool, local_admin).await?,
            [("admin".into(), true, None, false)]
        );
        let active: bool = sqlx::query_scalar("SELECT is_active FROM users WHERE id = $1")
            .bind(local_admin)
            .fetch_one(&pool)
            .await?;
        assert!(active);
    }
    Ok(())
}

async fn wait_for_advisory_waiter(pool: &PgPool, key: i64) -> Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_locks
                 WHERE locktype = 'advisory' AND classid::bigint = 526
                   AND objid::bigint = $1 AND objsubid = 2 AND NOT granted
                   AND database = (SELECT oid FROM pg_database WHERE datname = current_database()))",
            )
            .bind(key)
            .fetch_one(pool)
            .await?;
            if waiting {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}

#[sqlx::test]
async fn authoritative_revocation_does_not_retain_the_final_managed_admin(
    pool: PgPool,
) -> Result<()> {
    let (owner, _, identity, _) = fixture(&pool).await?;
    sync(&pool, owner, identity).await?;
    assert_eq!(
        reconcile(
            &pool,
            owner,
            identity,
            reserve_entitlement_generation(&pool).await?,
            Ok(vec![]),
            &json!({"groups": []}),
        )
        .await?,
        ReconciliationOutcome::Reconciled
    );
    assert_eq!(
        role_sources(&pool, owner).await?,
        [("viewer".into(), false, Some(identity), false)]
    );
    Ok(())
}

#[sqlx::test]
async fn deletion_removes_sources_before_identity_and_retains_actor_identifier(
    pool: PgPool,
) -> Result<()> {
    let (owner, actor, identity, _) = fixture(&pool).await?;
    sync(&pool, owner, identity).await?;
    replace_role(&pool, actor, actor, AuthRole::Admin).await?;
    sqlx::query(
        "INSERT INTO admin_audit_events (actor_user_id, actor_identifier, action, target)
         VALUES ($1, 'retained@example.invalid', 'user_updated', 'regression')",
    )
    .bind(owner)
    .execute(&pool)
    .await?;
    assert_eq!(
        delete_user_with_admin_guard(&pool, owner).await?,
        GuardedMutationOutcome::Applied
    );
    let audit: (Option<Uuid>, String) = sqlx::query_as(
        "SELECT actor_user_id, actor_identifier FROM admin_audit_events WHERE target = 'regression'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(audit, (None, "retained@example.invalid".into()));
    let identities: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM external_identities WHERE user_id = $1")
            .bind(owner)
            .fetch_one(&pool)
            .await?;
    assert_eq!(identities, 0);
    Ok(())
}

// Exercise the real Axum JSON/session/CSRF boundary in process. SQLx owns the
// isolated database; these tests need no listening HTTP server.
async fn patch_session(pool: &PgPool, actor: Uuid) -> Result<String> {
    let token = format!("admin-patch-{}", Uuid::new_v4());
    crystal_forge::queries::auth_identity::create_user_session(
        pool,
        actor,
        hash_token(&token),
        chrono::Utc::now() + chrono::Duration::hours(1),
        None,
        None,
        "local".into(),
    )
    .await?;
    Ok(token)
}

async fn http_patch(
    pool: &PgPool,
    actor_token: &str,
    owner: Uuid,
    body: serde_json::Value,
    csrf: bool,
) -> Result<(StatusCode, serde_json::Value)> {
    let app = Router::new()
        .route("/api/v1/admin/users/:id", patch(update_user))
        .with_state(pool.clone());
    let mut request = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/admin/users/{owner}"))
        .header("content-type", "application/json")
        .header("x-real-ip", "192.0.2.42")
        .header(
            "cookie",
            format!("{SESSION_COOKIE_NAME}={actor_token}; {CSRF_COOKIE_NAME}=patch-csrf"),
        );
    if csrf {
        request = request.header(CSRF_HEADER_NAME.as_str(), "patch-csrf");
    }
    let response = app
        .oneshot(request.body(Body::from(body.to_string()))?)
        .await?;
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((status, serde_json::from_slice(&body)?))
}

async fn patch_fixture(pool: &PgPool) -> Result<(Uuid, Uuid, Uuid, String)> {
    let (owner, actor, identity, environment) = fixture(pool).await?;
    legacy(pool, owner, environment).await?;
    replace_role(pool, actor, actor, AuthRole::Admin).await?;
    sqlx::query(
        "UPDATE users SET is_active = false, password_hash = 'original-hash' WHERE id = $1",
    )
    .bind(owner)
    .execute(pool)
    .await?;
    // Keep independent managed authority beside the unresolved local category.
    sqlx::query(
        "INSERT INTO user_role_assignments (user_id, role, local_grant, oidc_identity_id, legacy_ownership_unresolved)
         VALUES ($1, 'viewer', false, $2, false)",
    ).bind(owner).bind(identity).execute(pool).await?;
    let token = patch_session(pool, actor).await?;
    Ok((owner, actor, identity, token))
}

async fn patch_snapshot(pool: &PgPool, owner: Uuid) -> Result<serde_json::Value> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(
          'user', (SELECT to_jsonb(u) FROM users u WHERE id = $1),
          'roles', (SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY role), '[]'::jsonb)
                    FROM user_role_assignments r WHERE user_id = $1),
          'environments', (SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY environment_id), '[]'::jsonb)
                           FROM user_environment_memberships m WHERE user_id = $1),
          'preferences', (SELECT to_jsonb(p) FROM user_notification_preferences p WHERE user_id = $1),
          'audits', (SELECT coalesce(jsonb_agg(to_jsonb(a) ORDER BY id), '[]'::jsonb) FROM admin_audit_events a))",
    ).bind(owner).fetch_one(pool).await?)
}

fn all_fields_patch() -> serde_json::Value {
    json!({"edit_local_grants": true, "enabled": true, "role": "Admin", "environments": ["admin-source"], "password": "replacement-password"})
}

#[sqlx::test]
async fn http_mixed_patch_validates_every_field_before_mutation(pool: PgPool) -> Result<()> {
    let (owner, _, _, token) = patch_fixture(&pool).await?;
    let before = patch_snapshot(&pool, owner).await?;
    for invalid in [
        json!({"edit_local_grants": true, "enabled": true, "role": "Admin", "environments": ["does-not-exist"], "password": "replacement-password"}),
        json!({"edit_local_grants": true, "enabled": true, "role": "Admin", "environments": ["admin-source"], "password": "short"}),
        json!({"edit_local_grants": true, "clear_local_role": true, "enabled": true, "environments": ["does-not-exist"], "password": "replacement-password"}),
    ] {
        let (status, body) = http_patch(&pool, &token, owner, invalid, true).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(patch_snapshot(&pool, owner).await?, before);
    }
    let (status, _) = http_patch(&pool, &token, owner, all_fields_patch(), false).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(patch_snapshot(&pool, owner).await?, before);
    let (status, _) = http_patch(&pool, "invalid-session", owner, all_fields_patch(), true).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(patch_snapshot(&pool, owner).await?, before);
    let (status, _) = http_patch(&pool, &token, Uuid::new_v4(), json!({}), true).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let viewer = user(&pool).await?;
    replace_role(&pool, viewer, viewer, AuthRole::Viewer).await?;
    let viewer_token = patch_session(&pool, viewer).await?;
    let (status, _) = http_patch(&pool, &viewer_token, owner, all_fields_patch(), true).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(patch_snapshot(&pool, owner).await?, before);
    Ok(())
}

#[sqlx::test]
async fn http_patch_rolls_back_failures_after_each_write_and_audits(pool: PgPool) -> Result<()> {
    let (owner, _, _, token) = patch_fixture(&pool).await?;
    let before = patch_snapshot(&pool, owner).await?;
    sqlx::raw_sql(
        "CREATE SEQUENCE admin_patch_failures;
         CREATE FUNCTION fail_admin_patch_write() RETURNS trigger AS $$
         BEGIN
             PERFORM nextval('admin_patch_failures');
             RAISE EXCEPTION 'injected private database failure';
         END;
         $$ LANGUAGE plpgsql;",
    )
    .execute(&pool)
    .await?;
    // AFTER triggers prove the mutation actually ran before the failure. The
    // final audit trigger allows the summary and earlier detail audits to write.
    for (index, (table, event, condition)) in [
        (
            "users",
            "UPDATE OF is_active",
            format!("NEW.id = '{owner}'::uuid"),
        ),
        (
            "user_role_assignments",
            "INSERT",
            format!("NEW.user_id = '{owner}'::uuid AND NEW.local_grant"),
        ),
        (
            "user_environment_memberships",
            "INSERT",
            format!("NEW.user_id = '{owner}'::uuid AND NEW.local_grant"),
        ),
        (
            "users",
            "UPDATE OF password_hash",
            format!("NEW.id = '{owner}'::uuid"),
        ),
        (
            "admin_audit_events",
            "INSERT",
            "NEW.metadata @> '{\"changes\":{\"password_reset\":true}}'::jsonb".into(),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::raw_sql(&format!(
            "CREATE TRIGGER fail_patch AFTER {event} ON {table}
             FOR EACH ROW WHEN ({condition}) EXECUTE FUNCTION fail_admin_patch_write();",
        ))
        .execute(&pool)
        .await?;
        let (status, body) = http_patch(&pool, &token, owner, all_fields_patch(), true).await?;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{table}: {body}");
        assert_eq!(body["message"], "Failed to update user");
        assert!(body["details"].is_null());
        assert!(!body.to_string().contains("injected private"));
        // Sequences do not roll back: prove this exact fault was reached,
        // rather than accepting an unrelated earlier database error as proof.
        let (hits, called): (i64, bool) =
            sqlx::query_as("SELECT last_value, is_called FROM admin_patch_failures")
                .fetch_one(&pool)
                .await?;
        assert!(called, "{table} {event}");
        assert_eq!(hits, index as i64 + 1, "{table} {event}");
        assert_eq!(
            patch_snapshot(&pool, owner).await?,
            before,
            "{table} {event}"
        );
        sqlx::raw_sql(&format!("DROP TRIGGER fail_patch ON {table}"))
            .execute(&pool)
            .await?;
    }
    Ok(())
}

#[sqlx::test]
async fn http_patch_commits_all_fields_and_coherent_audits(pool: PgPool) -> Result<()> {
    let (owner, actor, identity, token) = patch_fixture(&pool).await?;
    let (status, body) = http_patch(&pool, &token, owner, all_fields_patch(), true).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["enabled"], true);
    assert_eq!(body["role"], "Admin");
    assert_eq!(body["environments"], json!(["admin-source"]));
    assert_eq!(
        role_sources(&pool, owner).await?,
        [
            ("admin".into(), true, None, false),
            ("viewer".into(), false, Some(identity), false),
        ]
    );
    let snapshot = patch_snapshot(&pool, owner).await?;
    let hash = snapshot["user"]["password_hash"].as_str().unwrap();
    crystal_forge::auth::password::verify_password("replacement-password", hash)?;
    assert_eq!(snapshot["environments"][0]["local_grant"], true);
    assert_eq!(
        snapshot["environments"][0]["assigned_by_user_id"],
        actor.to_string()
    );
    assert_eq!(
        snapshot["environments"][0]["legacy_ownership_unresolved"],
        false
    );
    assert!(!snapshot["preferences"].is_null());
    let audits = snapshot["audits"].as_array().unwrap();
    assert_eq!(audits.len(), 5);
    let actor_email = format!("{actor}@example.invalid");
    for audit in audits {
        assert_eq!(audit["actor_user_id"], actor.to_string());
        assert_eq!(audit["actor_identifier"], actor_email);
        assert_eq!(audit["request_origin"], "192.0.2.42");
        assert_eq!(
            audit["target"],
            format!("{owner}@example.invalid ({owner})")
        );
        assert_eq!(audit["created_at"], audits[0]["created_at"]);
        assert!(!audit.to_string().contains(hash));
        assert!(!audit.to_string().contains("replacement-password"));
    }
    assert_eq!(
        audits
            .iter()
            .filter(|a| a["metadata"]["change_count"] == 4)
            .count(),
        1
    );
    let role_audit = audits
        .iter()
        .find(|a| a["action"] == "user_role_assigned")
        .unwrap();
    assert_eq!(
        role_audit["metadata"]["changes"],
        json!({
            "from": "Operator", "local_role": "Admin", "classifies_legacy_roles": true,
        })
    );
    let environment_audit = audits
        .iter()
        .find(|a| a["action"] == "user_environment_membership_updated")
        .unwrap();
    assert_eq!(
        environment_audit["metadata"]["changes"],
        json!({
            "from": ["admin-source"], "local_environments": ["admin-source"],
            "classifies_legacy_environments": true,
        })
    );
    Ok(())
}

#[sqlx::test]
async fn http_omitted_grants_preserve_unresolved_and_managed_sources(pool: PgPool) -> Result<()> {
    let (owner, _, _, token) = patch_fixture(&pool).await?;
    let before = patch_snapshot(&pool, owner).await?;
    let (status, _) = http_patch(&pool, &token, owner, json!({"enabled": true}), true).await?;
    assert_eq!(status, StatusCode::OK);
    let after = patch_snapshot(&pool, owner).await?;
    assert_eq!(before["roles"], after["roles"]);
    assert_eq!(before["environments"], after["environments"]);
    let (status, _) = http_patch(
        &pool,
        &token,
        owner,
        json!({"password": "replacement-password"}),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let after = patch_snapshot(&pool, owner).await?;
    assert_eq!(before["roles"], after["roles"]);
    assert_eq!(before["environments"], after["environments"]);
    crystal_forge::auth::password::verify_password(
        "replacement-password",
        after["user"]["password_hash"].as_str().unwrap(),
    )?;
    let (status, _) = http_patch(
        &pool,
        &token,
        owner,
        json!({"edit_local_grants": true, "environments": []}),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let after = patch_snapshot(&pool, owner).await?;
    assert_eq!(before["roles"], after["roles"]);
    assert_eq!(after["environments"], json!([]));
    Ok(())
}

#[sqlx::test]
async fn http_combined_patch_last_admin_rejection_is_atomic(pool: PgPool) -> Result<()> {
    let (owner, actor, _, token) = patch_fixture(&pool).await?;
    // The actor is the only enabled Admin. Every requested category must roll
    // back together when the combined final state would remove that Admin.
    let before = patch_snapshot(&pool, actor).await?;
    for body in [
        json!({"edit_local_grants": true, "enabled": false, "role": "Admin", "environments": ["admin-source"], "password": "replacement-password"}),
        json!({"edit_local_grants": true, "enabled": true, "role": "Viewer", "environments": [], "password": "replacement-password"}),
    ] {
        let (status, _) = http_patch(&pool, &token, actor, body, true).await?;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(patch_snapshot(&pool, actor).await?, before);
    }
    // A managed Admin survives an explicit local Viewer edit, but cannot
    // satisfy the guard if the same PATCH also disables the account.
    sqlx::query("UPDATE users SET is_active = true WHERE id = $1")
        .bind(owner)
        .execute(&pool)
        .await?;
    let identity: Uuid =
        sqlx::query_scalar("SELECT id FROM external_identities WHERE user_id = $1")
            .bind(owner)
            .fetch_one(&pool)
            .await?;
    replace_role(&pool, owner, actor, AuthRole::Viewer).await?;
    replace_user_environment_memberships(&pool, owner, actor, &[])
        .await
        .map_err(anyhow::Error::msg)?;
    sync(&pool, owner, identity).await?;
    let (status, _) = http_patch(&pool, &token, actor, json!({"enabled": false}), true).await?;
    assert_eq!(status, StatusCode::OK);
    // Authenticate as the remaining managed Admin.
    let managed_token = patch_session(&pool, owner).await?;
    let (status, body) = http_patch(
        &pool,
        &managed_token,
        owner,
        json!({"edit_local_grants": true, "role": "Viewer"}),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["role"], "Admin");
    let before = patch_snapshot(&pool, owner).await?;
    let (status, _) = http_patch(
        &pool,
        &managed_token,
        owner,
        json!({"edit_local_grants": true, "enabled": false, "role": "Viewer"}),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(patch_snapshot(&pool, owner).await?, before);
    Ok(())
}

#[sqlx::test]
async fn concurrent_http_multi_field_patches_retain_one_enabled_admin(pool: PgPool) -> Result<()> {
    let first = user(&pool).await?;
    let second = user(&pool).await?;
    replace_role(&pool, first, first, AuthRole::Admin).await?;
    replace_role(&pool, second, second, AuthRole::Admin).await?;
    let first_token = patch_session(&pool, first).await?;
    let second_token = patch_session(&pool, second).await?;
    let first_before = patch_snapshot(&pool, first).await?;
    let second_before = patch_snapshot(&pool, second).await?;
    let body = json!({"edit_local_grants": true, "enabled": false, "role": "Viewer", "environments": [], "password": "replacement-password"});
    let work = async {
        tokio::join!(
            http_patch(&pool, &first_token, first, body.clone(), true),
            http_patch(&pool, &second_token, second, body.clone(), true),
        )
    };
    let (first_result, second_result) =
        tokio::time::timeout(std::time::Duration::from_secs(15), work).await?;
    let (first_status, first_body) = first_result?;
    let (second_status, second_body) = second_result?;
    assert!(
        (first_status == StatusCode::OK && second_status == StatusCode::CONFLICT)
            || (second_status == StatusCode::OK && first_status == StatusCode::CONFLICT),
        "{first_status}: {first_body}; {second_status}: {second_body}",
    );
    let (rejected, before) = if first_status == StatusCode::CONFLICT {
        (first, first_before)
    } else {
        (second, second_before)
    };
    let after = patch_snapshot(&pool, rejected).await?;
    for category in ["user", "roles", "environments", "preferences"] {
        assert_eq!(after[category], before[category], "{category}");
    }
    assert_eq!(after["audits"].as_array().unwrap().len(), 5);
    assert_eq!(
        crystal_forge::queries::admin::count_enabled_admins(&pool).await?,
        1
    );
    Ok(())
}

#[sqlx::test]
async fn http_grant_edits_require_explicit_intent_for_every_user(pool: PgPool) -> Result<()> {
    let (owner, actor, _, token) = patch_fixture(&pool).await?;
    // Both externally linked and local accounts reject old clients that send
    // effective values back without deliberate local-source edit intent.
    for (target, role, environments) in [
        (owner, "Operator", vec!["admin-source"]),
        (actor, "Admin", vec![]),
    ] {
        let before = patch_snapshot(&pool, target).await?;
        for invalid in [
            json!({"role": role, "enabled": true, "password": "replacement-password"}),
            json!({"environments": environments, "enabled": true, "password": "replacement-password"}),
            json!({"role": role, "environments": environments}),
            json!({"clear_local_role": true}),
            json!({"edit_local_grants": false, "role": role}),
            json!({"edit_local_grants": false, "environments": []}),
            json!({"edit_local_grants": true, "role": role, "clear_local_role": true,
                   "enabled": true, "password": "replacement-password"}),
        ] {
            let (status, body) = http_patch(&pool, &token, target, invalid, true).await?;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(patch_snapshot(&pool, target).await?, before);
        }
    }
    Ok(())
}

async fn clear_role_fixture(pool: &PgPool) -> Result<(Uuid, Uuid, Uuid, String)> {
    let (owner, actor, identity, token) = patch_fixture(pool).await?;
    // Overlap local with both other sources and add a local-only orphan. Clear
    // must remove only the local contributions, without classifying legacy.
    sqlx::query(
        "UPDATE user_role_assignments SET local_grant = true, granted_by_user_id = $2
         WHERE user_id = $1",
    )
    .bind(owner)
    .bind(actor)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO user_role_assignments
         (user_id, role, local_grant, legacy_ownership_unresolved, granted_by_user_id)
         VALUES ($1, 'admin', true, false, $2)",
    )
    .bind(owner)
    .bind(actor)
    .execute(pool)
    .await?;
    Ok((owner, actor, identity, token))
}

#[sqlx::test]
async fn http_clear_local_role_preserves_managed_and_unresolved_legacy_sources(
    pool: PgPool,
) -> Result<()> {
    let (owner, _, identity, token) = clear_role_fixture(&pool).await?;
    let before = patch_snapshot(&pool, owner).await?;
    let (status, body) = http_patch(
        &pool,
        &token,
        owner,
        json!({"edit_local_grants": true, "clear_local_role": true, "enabled": true,
               "password": "replacement-password"}),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["role"], "Operator");
    assert_eq!(
        role_sources(&pool, owner).await?,
        [
            ("operator".into(), false, None, true),
            ("viewer".into(), false, Some(identity), false),
        ]
    );
    let after = patch_snapshot(&pool, owner).await?;
    assert_eq!(before["environments"], after["environments"]);
    for row in after["roles"].as_array().unwrap() {
        let original = before["roles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|prior| prior["id"] == row["id"])
            .unwrap();
        let mut expected = original.clone();
        expected["local_grant"] = json!(false);
        assert_eq!(*row, expected);
    }
    let audits = after["audits"].as_array().unwrap();
    assert_eq!(audits.len(), 4);
    let clear = audits
        .iter()
        .find(|a| a["action"] == "user_role_assigned")
        .unwrap();
    assert_eq!(
        clear["metadata"]["changes"],
        json!({
            "from": "Admin", "clear_local_role": true, "classifies_legacy_roles": false,
        })
    );
    // Like explicit replacement, a repeated clear audits source intent even
    // when there is no local contribution left to remove.
    let (status, _) = http_patch(
        &pool,
        &token,
        owner,
        json!({"edit_local_grants": true, "clear_local_role": true}),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let repeated = patch_snapshot(&pool, owner).await?;
    for category in ["user", "roles", "environments", "preferences"] {
        assert_eq!(repeated[category], after[category], "{category}");
    }
    assert_eq!(repeated["audits"].as_array().unwrap().len(), 6);
    Ok(())
}

#[sqlx::test]
async fn http_clear_local_role_audit_failure_rolls_back_all_fields(pool: PgPool) -> Result<()> {
    let (owner, _, _, token) = clear_role_fixture(&pool).await?;
    let before = patch_snapshot(&pool, owner).await?;
    sqlx::raw_sql(
        "CREATE SEQUENCE clear_role_audit_failure;
         CREATE FUNCTION fail_clear_role_audit() RETURNS trigger AS $$
         BEGIN
             PERFORM nextval('clear_role_audit_failure');
             RAISE EXCEPTION 'private clear role audit failure';
         END;
         $$ LANGUAGE plpgsql;
         CREATE TRIGGER fail_clear_role_audit AFTER INSERT ON admin_audit_events
         FOR EACH ROW WHEN (NEW.metadata @> '{\"changes\":{\"clear_local_role\":true}}'::jsonb)
         EXECUTE FUNCTION fail_clear_role_audit();",
    )
    .execute(&pool)
    .await?;
    let (status, body) = http_patch(
        &pool,
        &token,
        owner,
        json!({"edit_local_grants": true, "clear_local_role": true, "enabled": true,
               "environments": [], "password": "replacement-password"}),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["message"], "Failed to update user");
    let called: bool = sqlx::query_scalar("SELECT is_called FROM clear_role_audit_failure")
        .fetch_one(&pool)
        .await?;
    assert!(called);
    assert_eq!(patch_snapshot(&pool, owner).await?, before);
    Ok(())
}

#[sqlx::test]
async fn http_clear_local_role_uses_final_effective_union_for_last_admin(
    pool: PgPool,
) -> Result<()> {
    let actor = user(&pool).await?;
    replace_role(&pool, actor, actor, AuthRole::Admin).await?;
    let token = patch_session(&pool, actor).await?;
    let before = patch_snapshot(&pool, actor).await?;
    let (status, _) = http_patch(
        &pool,
        &token,
        actor,
        json!({"edit_local_grants": true, "clear_local_role": true, "enabled": true,
               "environments": [], "password": "replacement-password"}),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(patch_snapshot(&pool, actor).await?, before);
    // The same Admin row may also carry managed or unresolved legacy sources.
    // Either survives clear, but disabling it still removes the final Admin.
    let identity: Uuid = sqlx::query_scalar(
        "INSERT INTO external_identities (user_id, provider_key, subject)
         VALUES ($1, 'oidc', $2) RETURNING id",
    )
    .bind(actor)
    .bind(actor.to_string())
    .fetch_one(&pool)
    .await?;
    for managed in [true, false] {
        sqlx::query(
            "UPDATE user_role_assignments
             SET local_grant = true, oidc_identity_id = $2, legacy_ownership_unresolved = $3
             WHERE user_id = $1 AND role = 'admin'",
        )
        .bind(actor)
        .bind(if managed { Some(identity) } else { None })
        .bind(!managed)
        .execute(&pool)
        .await?;
        let before = patch_snapshot(&pool, actor).await?;
        let (status, _) = http_patch(
            &pool,
            &token,
            actor,
            json!({"edit_local_grants": true, "clear_local_role": true, "enabled": false,
                   "environments": [], "password": "replacement-password"}),
            true,
        )
        .await?;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(patch_snapshot(&pool, actor).await?, before);
        let (status, body) = http_patch(
            &pool,
            &token,
            actor,
            json!({"edit_local_grants": true, "clear_local_role": true}),
            true,
        )
        .await?;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["role"], "Admin");
        assert_eq!(
            role_sources(&pool, actor).await?,
            [(
                "admin".into(),
                false,
                if managed { Some(identity) } else { None },
                !managed
            ),]
        );
    }
    Ok(())
}

async fn http_source_list(pool: &PgPool, token: &str) -> Result<(StatusCode, serde_json::Value)> {
    let response = Router::new()
        .route(
            "/api/v1/admin/users",
            axum::routing::get(crystal_forge::handlers::api::admin::list_users),
        )
        .with_state(pool.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/users")
                .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
                .body(Body::empty())?,
        )
        .await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

#[sqlx::test]
async fn batched_source_projection_matches_admin_http_and_patch_summary(
    pool: PgPool,
) -> Result<()> {
    use crystal_forge::api::models::AdminUserSummary;
    use crystal_forge::queries::admin::list_admin_user_grant_sources;

    let (owner, actor, identity, overlap_environment) = fixture(&pool).await?;
    let empty = user(&pool).await?;
    replace_role(&pool, actor, actor, AuthRole::Admin).await?;
    // Three independent flags on one row must appear in all three vectors.
    // The remaining rows prove false local flags never become inferred grants.
    sqlx::query(
        "INSERT INTO user_role_assignments
         (user_id, role, local_grant, oidc_identity_id, legacy_ownership_unresolved)
         VALUES ($1, 'viewer', false, NULL, true),
                ($1, 'operator', false, $2, false),
                ($1, 'admin', true, $2, true)",
    )
    .bind(owner)
    .bind(identity)
    .execute(&pool)
    .await?;
    for (name, local, managed, legacy) in [
        ("z-source-legacy", false, false, true),
        ("a-source-local", true, false, false),
        ("m-source-oidc", false, true, false),
    ] {
        let environment: Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                .bind(name)
                .fetch_one(&pool)
                .await?;
        sqlx::query(
            "INSERT INTO user_environment_memberships
             (user_id, environment_id, local_grant, oidc_identity_id, legacy_ownership_unresolved)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(owner)
        .bind(environment)
        .bind(local)
        .bind(if managed { Some(identity) } else { None })
        .bind(legacy)
        .execute(&pool)
        .await?;
    }
    sqlx::query(
        "INSERT INTO user_environment_memberships
         (user_id, environment_id, local_grant, oidc_identity_id, legacy_ownership_unresolved)
         VALUES ($1, $2, true, $3, true)",
    )
    .bind(owner)
    .bind(overlap_environment)
    .bind(identity)
    .execute(&pool)
    .await?;
    sqlx::query("UPDATE users SET password_hash = 'source-projection-secret-hash' WHERE id = $1")
        .bind(owner)
        .execute(&pool)
        .await?;
    sqlx::query(
        "UPDATE external_identities SET claims = '{\"private\":\"source-projection-secret-claim\"}'
         WHERE id = $1",
    )
    .bind(identity)
    .execute(&pool)
    .await?;

    let rows = list_admin_user_grant_sources(&pool, &[owner, empty]).await?;
    assert_eq!(rows.len(), 7);
    assert!(rows.iter().all(|row| row.user_id == owner));
    let admin = rows
        .iter()
        .find(|row| row.role == Some(AuthRole::Admin))
        .unwrap();
    assert!(admin.local_grant && admin.legacy_ownership_unresolved);
    assert_eq!(admin.oidc_identity_id, Some(identity));
    let overlap = rows
        .iter()
        .find(|row| row.environment_name.as_deref() == Some("admin-source"))
        .unwrap();
    assert!(overlap.local_grant && overlap.legacy_ownership_unresolved);
    assert_eq!(overlap.oidc_identity_id, Some(identity));
    assert!(list_admin_user_grant_sources(&pool, &[]).await?.is_empty());

    let token = patch_session(&pool, actor).await?;
    let (status, body) = http_source_list(&pool, &token).await?;
    assert_eq!(status, StatusCode::OK);
    let summaries = body.as_array().unwrap();
    let summary = summaries
        .iter()
        .find(|value| value["id"] == owner.to_string())
        .unwrap();
    let expected_sources = json!({
        "local_roles": ["Admin"],
        "local_environments": ["a-source-local", "admin-source"],
        "oidc_roles": ["Admin", "Operator"],
        "oidc_environments": ["admin-source", "m-source-oidc"],
        "legacy_roles": ["Admin", "Viewer"],
        "legacy_environments": ["admin-source", "z-source-legacy"]
    });
    assert_eq!(summary["grant_sources"], expected_sources);
    assert_eq!(summary["role"], "Admin");
    assert_eq!(summary["identity_source"], "oidc_derived");
    assert_eq!(
        summary["environments"],
        json!([
            "a-source-local",
            "admin-source",
            "m-source-oidc",
            "z-source-legacy"
        ])
    );
    let empty_summary = summaries
        .iter()
        .find(|value| value["id"] == empty.to_string())
        .unwrap();
    assert_eq!(empty_summary["role"], serde_json::Value::Null);
    assert_eq!(empty_summary["environments"], json!([]));
    assert_eq!(
        empty_summary["grant_sources"],
        json!({
            "local_roles": [], "local_environments": [],
            "oidc_roles": [], "oidc_environments": [],
            "legacy_roles": [], "legacy_environments": []
        })
    );
    // Exact key sets exclude credentials, claims, tokens and identity IDs.
    let mut keys = summary
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(
        keys,
        [
            "enabled",
            "environments",
            "grant_sources",
            "id",
            "identifier",
            "identity_source",
            "role",
            "updated_at"
        ]
    );
    let serialized = body.to_string();
    for secret in [
        "source-projection-secret-hash",
        "source-projection-secret-claim",
        &token,
        &identity.to_string(),
    ] {
        assert!(!serialized.contains(secret));
    }
    let (status, patched) =
        http_patch(&pool, &token, owner, json!({"enabled": true}), true).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["grant_sources"], expected_sources);
    assert_eq!(patched["role"], summary["role"]);
    assert_eq!(patched["environments"], summary["environments"]);

    let mut older_response = summary.clone();
    older_response
        .as_object_mut()
        .unwrap()
        .remove("grant_sources");
    assert!(
        serde_json::from_value::<AdminUserSummary>(older_response)?
            .grant_sources
            .is_none()
    );
    let non_admin = patch_session(&pool, empty).await?;
    assert_eq!(
        http_source_list(&pool, &non_admin).await?.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        http_source_list(&pool, "invalid-session").await?.0,
        StatusCode::FORBIDDEN
    );
    Ok(())
}
