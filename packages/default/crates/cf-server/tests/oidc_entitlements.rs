//! Source ownership, fail-closed freshness and populated upgrade regressions.
//! Each SQLx test owns its database and uses the full repository migrations.

use anyhow::Result;
use chrono::{DateTime, Utc};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::queries::auth_identity::{
    assign_role_to_user, clear_user_environment_memberships, clear_user_role_assignments,
    insert_user_environment_membership, sync_user_role,
};
use crystal_forge::queries::oidc_entitlements::{
    ReconciliationOutcome, reconcile, require_fresh_entitlements, reserve_entitlement_generation,
};
use serde_json::json;
use sqlx::{PgPool, migrate::Migrate};
use uuid::Uuid;

async fn user(pool: &PgPool) -> Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, first_name, last_name)
         VALUES ($1, $2, $3, 'OIDC', 'Regression')",
    )
    .bind(id)
    .bind(format!("oidc-{id}"))
    .bind(format!("{id}@example.invalid"))
    .execute(pool)
    .await?;
    Ok(id)
}

async fn identity(pool: &PgPool, user_id: Uuid) -> Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO external_identities (user_id, provider_key, subject)
         VALUES ($1, 'oidc', $2) RETURNING id",
    )
    .bind(user_id)
    .bind(Uuid::new_v4().to_string())
    .fetch_one(pool)
    .await?)
}

async fn fixture(pool: &PgPool) -> Result<(Uuid, Uuid, Uuid)> {
    let user_id = user(pool).await?;
    let identity_id = identity(pool, user_id).await?;
    let environment_id = sqlx::query_scalar(
        "INSERT INTO environments (name) VALUES ('oidc-regression') RETURNING id",
    )
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO oidc_group_mappings (group_name, role, environments)
         VALUES (' Operators ', 'operator', ARRAY['oidc-regression'])",
    )
    .execute(pool)
    .await?;
    Ok((user_id, identity_id, environment_id))
}

async fn sync(pool: &PgPool, user_id: Uuid, identity_id: Uuid) -> Result<ReconciliationOutcome> {
    reconcile(
        pool,
        user_id,
        identity_id,
        reserve_entitlement_generation(pool).await?,
        Ok(vec![" OPERATORS ".into()]),
        &json!({"authoritative": true}),
    )
    .await
}

async fn state(pool: &PgPool, identity_id: Uuid) -> Result<(String, Option<DateTime<Utc>>)> {
    Ok(sqlx::query_as(
        "SELECT entitlement_sync_state, last_successful_entitlement_sync_at
         FROM external_identities WHERE id = $1",
    )
    .bind(identity_id)
    .fetch_one(pool)
    .await?)
}

async fn roles(pool: &PgPool, user_id: Uuid) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT role::text FROM user_role_assignments WHERE user_id = $1 ORDER BY role::text",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

#[sqlx::test]
async fn fresh_sync_replaces_managed_roles_and_removes_memberships(pool: PgPool) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    assert!(
        require_fresh_entitlements(&pool, user_id, identity_id, 3600)
            .await
            .is_err()
    );
    for evidence_error in ["missing groups claim", "invalid groups claim"] {
        assert!(
            reconcile(
                &pool,
                user_id,
                identity_id,
                reserve_entitlement_generation(&pool).await?,
                Err(evidence_error.into()),
                &json!({"must_not_persist": true}),
            )
            .await
            .is_err()
        );
        assert_eq!(state(&pool, identity_id).await?, ("invalid".into(), None));
        assert!(roles(&pool, user_id).await?.is_empty());
        assert!(
            require_fresh_entitlements(&pool, user_id, identity_id, 3600)
                .await
                .is_err()
        );
    }
    assert_eq!(
        sync(&pool, user_id, identity_id).await?,
        ReconciliationOutcome::Reconciled
    );
    require_fresh_entitlements(&pool, user_id, identity_id, 3600).await?;
    assert_eq!(roles(&pool, user_id).await?, ["operator"]);
    let owned: (bool, Option<Uuid>) = sqlx::query_as(
        "SELECT local_grant, oidc_identity_id FROM user_role_assignments WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(owned, (false, Some(identity_id)));
    let membership_owner: (bool, Option<Uuid>) = sqlx::query_as(
        "SELECT local_grant, oidc_identity_id FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(membership_owner, (false, Some(identity_id)));
    reconcile(
        &pool,
        user_id,
        identity_id,
        reserve_entitlement_generation(&pool).await?,
        Ok(vec![]),
        &json!({"empty": true}),
    )
    .await?;
    assert_eq!(roles(&pool, user_id).await?, ["viewer"]);
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_environment_memberships WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(count, 0);
    let claims: serde_json::Value =
        sqlx::query_scalar("SELECT claims FROM external_identities WHERE id = $1")
            .bind(identity_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(claims, json!({"empty": true}));
    // An unmatched but valid group is also authoritative removal, not retention.
    sync(&pool, user_id, identity_id).await?;
    reconcile(
        &pool,
        user_id,
        identity_id,
        reserve_entitlement_generation(&pool).await?,
        Ok(vec!["unknown".into()]),
        &json!({}),
    )
    .await?;
    assert_eq!(roles(&pool, user_id).await?, ["viewer"]);
    Ok(())
}

#[sqlx::test]
async fn delayed_valid_and_invalid_evidence_cannot_overwrite_newer_success(
    pool: PgPool,
) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    sqlx::query("INSERT INTO oidc_group_mappings (group_name, role) VALUES ('old-admin', 'admin')")
        .execute(&pool)
        .await?;
    // Reserve in initiation order, then complete the newer login first.
    let old_generation = reserve_entitlement_generation(&pool).await?;
    let new_generation = reserve_entitlement_generation(&pool).await?;
    assert!(old_generation > 0 && new_generation > old_generation);
    let current_claims = json!({"groups": [], "current": true});
    reconcile(
        &pool,
        user_id,
        identity_id,
        new_generation,
        Ok(vec![]),
        &current_claims,
    )
    .await?;
    assert_eq!(roles(&pool, user_id).await?, ["viewer"]);
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(identity) FROM external_identities identity WHERE id = $1",
    )
    .bind(identity_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        before["highest_accepted_entitlement_generation"],
        json!(new_generation)
    );
    assert_eq!(before["claims"], current_claims);
    assert_eq!(before["entitlement_sync_state"], json!("valid"));
    assert!(!before["last_successful_entitlement_sync_at"].is_null());
    for (generation, evidence) in [
        (old_generation, Ok(vec!["old-admin".into()])),
        (old_generation, Err("old invalid evidence".into())),
        (new_generation, Ok(vec!["old-admin".into()])),
        (0, Err("unreserved evidence".into())),
        (-1, Ok(vec!["old-admin".into()])),
    ] {
        assert!(
            reconcile(
                &pool,
                user_id,
                identity_id,
                generation,
                evidence,
                &json!({"old": true}),
            )
            .await
            .is_err()
        );
        let after: serde_json::Value = sqlx::query_scalar(
            "SELECT to_jsonb(identity) FROM external_identities identity WHERE id = $1",
        )
        .bind(identity_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(after, before);
        assert_eq!(roles(&pool, user_id).await?, ["viewer"]);
        let memberships: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_environment_memberships WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(memberships, 0);
        require_fresh_entitlements(&pool, user_id, identity_id, 3600).await?;
    }
    Ok(())
}

#[sqlx::test]
async fn mapped_environment_names_normalize_before_resolution(pool: PgPool) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    let prod: Uuid = sqlx::query_scalar(
        "INSERT INTO environments (name) VALUES ('prod')
             ON CONFLICT (name) DO UPDATE SET is_active = true RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    sqlx::query("UPDATE oidc_group_mappings SET environments = ARRAY['Prod', ' prod ', '']")
        .execute(&pool)
        .await?;
    sync(&pool, user_id, identity_id).await?;
    let memberships: Vec<Uuid> = sqlx::query_scalar(
        "SELECT environment_id FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(memberships, vec![prod]);
    require_fresh_entitlements(&pool, user_id, identity_id, 3600).await?;
    Ok(())
}

#[sqlx::test]
async fn reconciliation_waits_for_admin_advisory_lock_before_user_lock(pool: PgPool) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    let mut admin_guard = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(526, 2)")
        .execute(&mut *admin_guard)
        .await?;
    let writer_pool = pool.clone();
    let writer = tokio::spawn(async move { sync(&writer_pool, user_id, identity_id).await });
    wait_for_lock(
        &pool,
        "SELECT EXISTS (SELECT 1 FROM pg_locks
         WHERE locktype = 'advisory' AND classid = 526 AND objid = 2
           AND objsubid = 2 AND NOT granted)",
    )
    .await?;
    assert_eq!(state(&pool, identity_id).await?, ("pending".into(), None));
    // Pending registration released its user lock. The grant-changing phase
    // must not hold that lock while waiting for the shared admin advisory lock.
    let mut probe = pool.begin().await?;
    sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE NOWAIT")
        .bind(user_id)
        .execute(&mut *probe)
        .await?;
    probe.rollback().await?;
    admin_guard.rollback().await?;
    assert_eq!(writer.await??, ReconciliationOutcome::Reconciled);
    Ok(())
}

#[sqlx::test]
async fn overlapping_sources_survive_independent_removals_and_preserve_actors(
    pool: PgPool,
) -> Result<()> {
    let (user_id, identity_id, environment_id) = fixture(&pool).await?;
    let actor_id = user(&pool).await?;
    let local = assign_role_to_user(&pool, user_id, AuthRole::Operator, Some(actor_id)).await?;
    insert_user_environment_membership(&pool, user_id, environment_id).await?;
    sync(&pool, user_id, identity_id).await?;
    reconcile(
        &pool,
        user_id,
        identity_id,
        reserve_entitlement_generation(&pool).await?,
        Ok(vec![]),
        &json!({}),
    )
    .await?;
    assert_eq!(roles(&pool, user_id).await?, ["operator", "viewer"]);
    let retained: (Uuid, Option<Uuid>, bool, Option<Uuid>) = sqlx::query_as(
        "SELECT id, granted_by_user_id, local_grant, oidc_identity_id
         FROM user_role_assignments WHERE user_id = $1 AND role = 'operator'",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(retained, (local.id, Some(actor_id), true, None));
    let local_env: bool = sqlx::query_scalar(
        "SELECT local_grant FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await?;
    assert!(local_env);
    sync(&pool, user_id, identity_id).await?;
    clear_user_role_assignments(&pool, user_id).await?;
    clear_user_environment_memberships(&pool, user_id).await?;
    assert_eq!(roles(&pool, user_id).await?, ["operator"]);
    let retained: (Uuid, Option<Uuid>, bool, Option<Uuid>) = sqlx::query_as(
        "SELECT id, granted_by_user_id, local_grant, oidc_identity_id
         FROM user_role_assignments WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        retained,
        (local.id, Some(actor_id), false, Some(identity_id))
    );
    sync_user_role(&pool, user_id, AuthRole::Admin).await?;
    assert_eq!(roles(&pool, user_id).await?, ["admin", "operator"]);
    Ok(())
}

#[sqlx::test]
async fn invalid_evidence_or_environment_rolls_back_without_advancing_success(
    pool: PgPool,
) -> Result<()> {
    let (user_id, identity_id, environment_id) = fixture(&pool).await?;
    sync(&pool, user_id, identity_id).await?;
    let successful = state(&pool, identity_id).await?.1;
    for environments in [vec!["does-not-exist"], vec!["oidc-regression"]] {
        sqlx::query("UPDATE environments SET is_active = false WHERE id = $1")
            .bind(environment_id)
            .execute(&pool)
            .await?;
        sqlx::query("UPDATE oidc_group_mappings SET role = 'admin', environments = $1")
            .bind(environments)
            .execute(&pool)
            .await?;
        assert!(sync(&pool, user_id, identity_id).await.is_err());
        assert_eq!(roles(&pool, user_id).await?, ["operator"]);
        assert_eq!(
            state(&pool, identity_id).await?,
            ("invalid".into(), successful)
        );
        assert!(
            require_fresh_entitlements(&pool, user_id, identity_id, 3600)
                .await
                .is_err()
        );
    }
    assert!(
        reconcile(
            &pool,
            user_id,
            identity_id,
            reserve_entitlement_generation(&pool).await?,
            Err("missing".into()),
            &json!({"bad": true})
        )
        .await
        .is_err()
    );
    assert_eq!(
        state(&pool, identity_id).await?,
        ("invalid".into(), successful)
    );
    let claims: serde_json::Value =
        sqlx::query_scalar("SELECT claims FROM external_identities WHERE id = $1")
            .bind(identity_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(claims, json!({"authoritative": true}));
    sqlx::query("UPDATE environments SET is_active = true WHERE id = $1")
        .bind(environment_id)
        .execute(&pool)
        .await?;
    sync(&pool, user_id, identity_id).await?;
    assert_eq!(roles(&pool, user_id).await?, ["admin"]);
    require_fresh_entitlements(&pool, user_id, identity_id, 3600).await?;
    Ok(())
}

#[sqlx::test]
async fn database_failure_preserves_grants_claims_and_success_time(pool: PgPool) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    sync(&pool, user_id, identity_id).await?;
    let successful = state(&pool, identity_id).await?.1;
    // Fail after managed-source deletion, proving the replacement is atomic.
    sqlx::raw_sql(
        "CREATE FUNCTION reject_oidc_grant() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN RAISE EXCEPTION 'injected regression failure'; END $$;
         CREATE TRIGGER reject_oidc_grant BEFORE INSERT ON user_role_assignments
         FOR EACH ROW EXECUTE FUNCTION reject_oidc_grant();",
    )
    .execute(&pool)
    .await?;
    assert!(
        reconcile(
            &pool,
            user_id,
            identity_id,
            reserve_entitlement_generation(&pool).await?,
            Ok(vec![]),
            &json!({"bad": true})
        )
        .await
        .is_err()
    );
    assert_eq!(roles(&pool, user_id).await?, ["operator"]);
    assert_eq!(
        state(&pool, identity_id).await?,
        ("invalid".into(), successful)
    );
    Ok(())
}

#[sqlx::test]
async fn foreign_identity_cannot_own_sources_or_invalidate_another_owner(
    pool: PgPool,
) -> Result<()> {
    let (user_id, identity_id, environment_id) = fixture(&pool).await?;
    sync(&pool, user_id, identity_id).await?;
    let other_user = user(&pool).await?;
    let other_identity = identity(&pool, other_user).await?;
    let before = state(&pool, identity_id).await?;
    assert!(
        reconcile(
            &pool,
            other_user,
            identity_id,
            reserve_entitlement_generation(&pool).await?,
            Err("invalid".into()),
            &json!({})
        )
        .await
        .is_err()
    );
    assert_eq!(state(&pool, identity_id).await?, before);
    assert!(
        sqlx::query(
            "INSERT INTO user_role_assignments (user_id, role, local_grant, oidc_identity_id)
         VALUES ($1, 'admin', false, $2)",
        )
        .bind(user_id)
        .bind(other_identity)
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            "UPDATE user_environment_memberships SET oidc_identity_id = $3
         WHERE user_id = $1 AND environment_id = $2",
        )
        .bind(user_id)
        .bind(environment_id)
        .bind(other_identity)
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM external_identities WHERE id = $1")
            .bind(identity_id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        require_fresh_entitlements(&pool, user_id, other_identity, 3600)
            .await
            .is_err()
    );
    Ok(())
}

#[sqlx::test]
async fn freshness_denies_nonvalid_states_inactive_users_and_stale_boundary(
    pool: PgPool,
) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    sync(&pool, user_id, identity_id).await?;
    assert!(
        require_fresh_entitlements(&pool, user_id, identity_id, 0)
            .await
            .is_err()
    );
    // Database-relative times avoid wall-clock sleeps. Equality is stale; any
    // later eligibility statement remains stale under the strict SQL boundary.
    sqlx::query(
        "UPDATE external_identities SET last_successful_entitlement_sync_at =
         statement_timestamp() - interval '3600 seconds' WHERE id = $1",
    )
    .bind(identity_id)
    .execute(&pool)
    .await?;
    assert!(
        require_fresh_entitlements(&pool, user_id, identity_id, 3600)
            .await
            .is_err()
    );
    for invalid_state in ["missing", "pending", "invalid", "legacy_unresolved"] {
        sqlx::query(
            "UPDATE external_identities SET entitlement_sync_state = $2,
             last_successful_entitlement_sync_at = statement_timestamp() WHERE id = $1",
        )
        .bind(identity_id)
        .bind(invalid_state)
        .execute(&pool)
        .await?;
        assert!(
            require_fresh_entitlements(&pool, user_id, identity_id, 3600)
                .await
                .is_err()
        );
    }
    sync(&pool, user_id, identity_id).await?;
    sqlx::query("UPDATE users SET is_active = false WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await?;
    assert!(
        require_fresh_entitlements(&pool, user_id, identity_id, 3600)
            .await
            .is_err()
    );
    let successful = state(&pool, identity_id).await?.1;
    assert!(sync(&pool, user_id, identity_id).await.is_err());
    assert_eq!(
        state(&pool, identity_id).await?,
        ("invalid".into(), successful)
    );
    Ok(())
}

#[sqlx::test]
async fn multiple_linked_identities_deny_without_replacing_rights(pool: PgPool) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    sync(&pool, user_id, identity_id).await?;
    let successful = state(&pool, identity_id).await?.1;
    let second_identity = identity(&pool, user_id).await?;
    assert!(
        require_fresh_entitlements(&pool, user_id, identity_id, 3600)
            .await
            .is_err()
    );
    assert!(sync(&pool, user_id, second_identity).await.is_err());
    assert_eq!(
        state(&pool, second_identity).await?,
        ("invalid".into(), None)
    );
    assert!(
        reconcile(
            &pool,
            user_id,
            identity_id,
            reserve_entitlement_generation(&pool).await?,
            Ok(vec![]),
            &json!({})
        )
        .await
        .is_err()
    );
    assert_eq!(
        state(&pool, identity_id).await?,
        ("invalid".into(), successful)
    );
    assert_eq!(roles(&pool, user_id).await?, ["operator"]);
    Ok(())
}

#[sqlx::test]
async fn non_oidc_provider_cannot_reconcile_or_pass_freshness(pool: PgPool) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    sync(&pool, user_id, identity_id).await?;
    sqlx::query("UPDATE external_identities SET provider_key = 'other-provider' WHERE id = $1")
        .bind(identity_id)
        .execute(&pool)
        .await?;
    let before = state(&pool, identity_id).await?;
    let claims_before: serde_json::Value =
        sqlx::query_scalar("SELECT claims FROM external_identities WHERE id = $1")
            .bind(identity_id)
            .fetch_one(&pool)
            .await?;
    assert!(
        require_fresh_entitlements(&pool, user_id, identity_id, 3600)
            .await
            .is_err()
    );
    assert!(sync(&pool, user_id, identity_id).await.is_err());
    assert!(
        reconcile(
            &pool,
            user_id,
            identity_id,
            reserve_entitlement_generation(&pool).await?,
            Err("missing".into()),
            &json!({})
        )
        .await
        .is_err()
    );
    assert_eq!(state(&pool, identity_id).await?, before);
    assert_eq!(roles(&pool, user_id).await?, ["operator"]);
    let claims_after: serde_json::Value =
        sqlx::query_scalar("SELECT claims FROM external_identities WHERE id = $1")
            .bind(identity_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(claims_after, claims_before);
    Ok(())
}

#[sqlx::test]
async fn other_linked_provider_blocks_replacement_of_its_managed_source(
    pool: PgPool,
) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    sync(&pool, user_id, identity_id).await?;
    let successful = state(&pool, identity_id).await?.1;
    let other_identity = identity(&pool, user_id).await?;
    sqlx::query("UPDATE external_identities SET provider_key = 'other-provider' WHERE id = $1")
        .bind(other_identity)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO user_role_assignments (user_id, role, local_grant, oidc_identity_id)
         VALUES ($1, 'admin', false, $2)",
    )
    .bind(user_id)
    .bind(other_identity)
    .execute(&pool)
    .await?;
    assert!(
        require_fresh_entitlements(&pool, user_id, identity_id, 3600)
            .await
            .is_err()
    );
    assert!(sync(&pool, user_id, identity_id).await.is_err());
    assert_eq!(
        state(&pool, identity_id).await?,
        ("invalid".into(), successful)
    );
    assert_eq!(
        state(&pool, other_identity).await?,
        ("missing".into(), None)
    );
    let sources: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT role::text, oidc_identity_id FROM user_role_assignments
         WHERE user_id = $1 ORDER BY role::text",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        sources,
        vec![
            ("admin".into(), other_identity),
            ("operator".into(), identity_id)
        ]
    );
    Ok(())
}

#[sqlx::test]
async fn concurrent_local_writer_and_reconciliation_preserve_both_sources(
    pool: PgPool,
) -> Result<()> {
    let (user_id, identity_id, environment_id) = fixture(&pool).await?;
    let (local, managed) = tokio::join!(
        async {
            assign_role_to_user(&pool, user_id, AuthRole::Operator, None).await?;
            insert_user_environment_membership(&pool, user_id, environment_id).await?;
            Ok::<_, anyhow::Error>(())
        },
        sync(&pool, user_id, identity_id),
    );
    local?;
    managed?;
    for table in ["user_role_assignments", "user_environment_memberships"] {
        let sources: (bool, Option<Uuid>) = sqlx::query_as(&format!(
            "SELECT local_grant, oidc_identity_id FROM {table} WHERE user_id = $1"
        ))
        .bind(user_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(sources, (true, Some(identity_id)));
    }
    require_fresh_entitlements(&pool, user_id, identity_id, 3600).await?;
    Ok(())
}

#[sqlx::test]
async fn older_failure_cannot_invalidate_a_newer_attempt(pool: PgPool) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    sync(&pool, user_id, identity_id).await?;
    sqlx::raw_sql(
        "CREATE FUNCTION gate_viewer_failure() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
             IF NEW.role = 'viewer' THEN
                 PERFORM pg_advisory_xact_lock(526335);
                 RAISE EXCEPTION 'injected older attempt failure';
             END IF;
             RETURN NEW;
         END $$;
         CREATE TRIGGER gate_viewer_failure BEFORE INSERT ON user_role_assignments
         FOR EACH ROW EXECUTE FUNCTION gate_viewer_failure();",
    )
    .execute(&pool)
    .await?;
    let mut gate = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(526335)")
        .execute(&mut *gate)
        .await?;
    let older_pool = pool.clone();
    let older = tokio::spawn(async move {
        let generation = reserve_entitlement_generation(&older_pool).await?;
        reconcile(
            &older_pool,
            user_id,
            identity_id,
            generation,
            Ok(vec![]),
            &json!({"older": true}),
        )
        .await
    });
    // The older replacement holds the user lock while blocked inside the trigger.
    wait_for_lock(
        &pool,
        "SELECT EXISTS (SELECT 1 FROM pg_locks
         WHERE locktype = 'advisory' AND objid = 526335 AND NOT granted)",
    )
    .await?;
    let newer_pool = pool.clone();
    let newer = tokio::spawn(async move { sync(&newer_pool, user_id, identity_id).await });
    // Queue the newer pending transaction before releasing the old failure.
    // PostgreSQL grants its user lock before the old failure's cleanup lock.
    wait_for_lock(
        &pool,
        "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
         WHERE datname = current_database() AND wait_event_type = 'Lock'
           AND query = 'SELECT is_active FROM users WHERE id = $1 FOR UPDATE')",
    )
    .await?;
    gate.rollback().await?;
    assert!(older.await?.is_err());
    assert_eq!(newer.await??, ReconciliationOutcome::Reconciled);
    assert_eq!(state(&pool, identity_id).await?.0, "valid");
    assert_eq!(roles(&pool, user_id).await?, ["operator"]);
    require_fresh_entitlements(&pool, user_id, identity_id, 3600).await?;
    Ok(())
}

// Observe database lock state rather than relying on task scheduling or sleeps
// as proof that the required reconciliation interleaving has occurred.
async fn wait_for_lock(pool: &PgPool, query: &str) -> Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if sqlx::query_scalar::<_, bool>(query).fetch_one(pool).await? {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}

#[sqlx::test]
async fn cancellation_leaves_durable_pending_and_denies_previous_freshness(
    pool: PgPool,
) -> Result<()> {
    let (user_id, identity_id, _) = fixture(&pool).await?;
    sync(&pool, user_id, identity_id).await?;
    let successful = state(&pool, identity_id).await?.1;
    let mut blocker = pool.begin().await?;
    // Block replacement after pending commits, without blocking identity reads.
    sqlx::query("LOCK TABLE user_role_assignments IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await?;
    let writer_pool = pool.clone();
    let writer = tokio::spawn(async move { sync(&writer_pool, user_id, identity_id).await });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if state(&pool, identity_id).await?.0 == "pending" {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await??;
    assert!(
        require_fresh_entitlements(&pool, user_id, identity_id, 3600)
            .await
            .is_err()
    );
    writer.abort();
    assert!(writer.await.unwrap_err().is_cancelled());
    blocker.rollback().await?;
    assert_eq!(
        state(&pool, identity_id).await?,
        ("pending".into(), successful)
    );
    assert_eq!(roles(&pool, user_id).await?, ["operator"]);
    sync(&pool, user_id, identity_id).await?;
    require_fresh_entitlements(&pool, user_id, identity_id, 3600).await?;
    Ok(())
}

#[sqlx::test]
async fn assignments_require_at_least_one_source(pool: PgPool) -> Result<()> {
    let (user_id, _, environment_id) = fixture(&pool).await?;
    assert!(
        sqlx::query(
            "INSERT INTO user_role_assignments (user_id, role, local_grant)
         VALUES ($1, 'admin', false)",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            "INSERT INTO user_environment_memberships (user_id, environment_id, local_grant)
         VALUES ($1, $2, false)",
        )
        .bind(user_id)
        .bind(environment_id)
        .execute(&pool)
        .await
        .is_err()
    );
    Ok(())
}

#[sqlx::test(migrations = false)]
async fn populated_upgrade_preserves_legacy_grants_and_blocks_until_classified(
    pool: PgPool,
) -> Result<()> {
    static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
    let mut connection = pool.acquire().await?;
    connection.ensure_migrations_table().await?;
    for migration in MIGRATOR.iter().filter(|m| m.version < 335) {
        connection.apply(migration).await?;
    }
    drop(connection);
    let (user_id, identity_id, environment_id) = fixture(&pool).await?;
    let local_user = user(&pool).await?;
    let role_id: Uuid = sqlx::query_scalar(
        "INSERT INTO user_role_assignments (user_id, role, granted_by_user_id)
         VALUES ($1, 'admin', $2) RETURNING id",
    )
    .bind(user_id)
    .bind(local_user)
    .fetch_one(&pool)
    .await?;
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO user_environment_memberships (user_id, environment_id, assigned_by_user_id)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(user_id)
    .bind(environment_id)
    .bind(local_user)
    .fetch_one(&pool)
    .await?;
    sqlx::query("INSERT INTO user_role_assignments (user_id, role) VALUES ($1, 'viewer')")
        .bind(local_user)
        .execute(&pool)
        .await?;
    MIGRATOR.run(&pool).await?;
    let legacy_role: (Uuid, Option<Uuid>, bool, bool, Option<Uuid>) = sqlx::query_as(
        "SELECT id, granted_by_user_id, local_grant, legacy_ownership_unresolved, oidc_identity_id
         FROM user_role_assignments WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(legacy_role, (role_id, Some(local_user), false, true, None));
    let legacy_env: (Uuid, Option<Uuid>, bool, bool, Option<Uuid>) = sqlx::query_as(
        "SELECT id, assigned_by_user_id, local_grant, legacy_ownership_unresolved, oidc_identity_id
         FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        legacy_env,
        (membership_id, Some(local_user), false, true, None)
    );
    let local: bool =
        sqlx::query_scalar("SELECT local_grant FROM user_role_assignments WHERE user_id = $1")
            .bind(local_user)
            .fetch_one(&pool)
            .await?;
    assert!(local);
    assert_eq!(
        sync(&pool, user_id, identity_id).await?,
        ReconciliationOutcome::LegacyUnresolved
    );
    assert_eq!(roles(&pool, user_id).await?, ["admin"]);
    assert_eq!(
        state(&pool, identity_id).await?,
        ("legacy_unresolved".into(), None)
    );
    assert!(
        require_fresh_entitlements(&pool, user_id, identity_id, 3600)
            .await
            .is_err()
    );
    let original_claims: serde_json::Value =
        sqlx::query_scalar("SELECT claims FROM external_identities WHERE id = $1")
            .bind(identity_id)
            .fetch_one(&pool)
            .await?;
    for evidence_error in ["missing groups claim", "invalid groups claim"] {
        assert_eq!(
            reconcile(
                &pool,
                user_id,
                identity_id,
                reserve_entitlement_generation(&pool).await?,
                Err(evidence_error.into()),
                &json!({"must_not_persist": evidence_error}),
            )
            .await?,
            ReconciliationOutcome::LegacyUnresolved,
        );
        assert_eq!(
            state(&pool, identity_id).await?,
            ("legacy_unresolved".into(), None)
        );
        let unchanged_role: (Uuid, Option<Uuid>, bool, bool, Option<Uuid>) = sqlx::query_as(
            "SELECT id, granted_by_user_id, local_grant, legacy_ownership_unresolved, oidc_identity_id
             FROM user_role_assignments WHERE user_id = $1 AND role = 'admin'",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(unchanged_role, legacy_role);
        let unchanged_env: (Uuid, Option<Uuid>, bool, bool, Option<Uuid>) = sqlx::query_as(
            "SELECT id, assigned_by_user_id, local_grant, legacy_ownership_unresolved, oidc_identity_id
             FROM user_environment_memberships WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(unchanged_env, legacy_env);
        assert_eq!(roles(&pool, user_id).await?, ["admin"]);
        let unchanged_claims: serde_json::Value =
            sqlx::query_scalar("SELECT claims FROM external_identities WHERE id = $1")
                .bind(identity_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(unchanged_claims, original_claims);
        assert!(
            require_fresh_entitlements(&pool, user_id, identity_id, 3600)
                .await
                .is_err()
        );
    }
    // Explicit local writer decisions classify the legacy contributions. Merely
    // writing a valid identity state cannot bypass unresolved assignment checks.
    sqlx::query("UPDATE external_identities SET entitlement_sync_state = 'valid', last_successful_entitlement_sync_at = statement_timestamp() WHERE id = $1")
        .bind(identity_id).execute(&pool).await?;
    assert!(
        require_fresh_entitlements(&pool, user_id, identity_id, 3600)
            .await
            .is_err()
    );
    assign_role_to_user(&pool, user_id, AuthRole::Admin, Some(local_user)).await?;
    insert_user_environment_membership(&pool, user_id, environment_id).await?;
    assert_eq!(
        sync(&pool, user_id, identity_id).await?,
        ReconciliationOutcome::Reconciled
    );
    assert_eq!(roles(&pool, user_id).await?, ["admin", "operator"]);
    require_fresh_entitlements(&pool, user_id, identity_id, 3600).await?;
    Ok(())
}
