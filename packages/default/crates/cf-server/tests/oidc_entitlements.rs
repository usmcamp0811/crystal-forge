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
async fn identity_creation_conflict_cannot_rebind_owner_or_overwrite_claims(
    pool: PgPool,
) -> Result<()> {
    use crystal_forge::queries::auth_identity::{
        AuthRepositoryError, create_user_and_bind_external_identity,
    };

    let first_claims = serde_json::json!({"marker": "first-validated-binding"});
    let first = create_user_and_bind_external_identity(
        &pool,
        "oidc-binding-first@example.invalid",
        Some("First Identity"),
        "oidc",
        "stable-binding-conflict",
        Some("https://identity.example.invalid"),
        first_claims.clone(),
    )
    .await?;
    let conflicting = create_user_and_bind_external_identity(
        &pool,
        "oidc-binding-second@example.invalid",
        Some("Conflicting Identity"),
        "oidc",
        "stable-binding-conflict",
        Some("https://identity.example.invalid"),
        serde_json::json!({"marker": "must-not-replace-existing-evidence"}),
    )
    .await;
    assert!(matches!(
        conflicting,
        Err(AuthRepositoryError::IdentityAlreadyBound)
    ));
    let binding: (Uuid, serde_json::Value) = sqlx::query_as(
        "SELECT user_id, claims FROM external_identities
         WHERE provider_key = 'oidc' AND subject = 'stable-binding-conflict'
           AND tenant_discriminator = 'https://identity.example.invalid'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(binding, (first.id, first_claims));
    let orphaned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE email = 'oidc-binding-second@example.invalid'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(orphaned, 0);
    Ok(())
}

#[sqlx::test]
async fn public_identity_upsert_preserves_owner_and_evidence_without_source_rows(
    pool: PgPool,
) -> Result<()> {
    use crystal_forge::queries::auth_identity::{
        AuthIdentityRepository, AuthRepositoryError, NewExternalIdentity,
    };

    let owner = user(&pool).await?;
    let other = user(&pool).await?;
    let repo = AuthIdentityRepository::new(&pool);
    let mut record = NewExternalIdentity {
        user_id: owner,
        provider_key: "oidc".into(),
        subject: "public-helper-stable-subject".into(),
        tenant_discriminator: Some("https://issuer.example.invalid".into()),
        claims: json!({"original": true}),
    };
    let identity = repo.upsert_external_identity(&record).await?;
    // No grants reference the identity: a composite source FK cannot hide a
    // takeover bug in this helper. Include freshness in the unchanged snapshot.
    for table in ["user_role_assignments", "user_environment_memberships"] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE user_id = $1 OR user_id = $2"
        ))
        .bind(owner)
        .bind(other)
        .fetch_one(&pool)
        .await?;
        assert_eq!(count, 0);
    }
    sqlx::query(
        "UPDATE external_identities SET entitlement_sync_state = 'valid',
         highest_accepted_entitlement_generation = 17,
         last_successful_entitlement_sync_at = statement_timestamp() WHERE id = $1",
    )
    .bind(identity.id)
    .execute(&pool)
    .await?;
    let snapshot = || async {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT to_jsonb(i) FROM external_identities i WHERE id = $1",
        )
        .bind(identity.id)
        .fetch_one(&pool)
        .await
    };
    let before = snapshot().await?;
    record.user_id = other;
    record.claims = json!({"must_not_replace": true});
    assert!(matches!(
        repo.upsert_external_identity(&record).await,
        Err(AuthRepositoryError::IdentityOwnershipConflict)
    ));
    assert_eq!(snapshot().await?, before);
    record.user_id = owner;
    let same = repo.upsert_external_identity(&record).await?;
    assert_eq!(same.id, identity.id);
    assert_eq!(same.claims, json!({"original": true}));
    assert_eq!(snapshot().await?, before);
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

// These are native HTTP/SQLx regressions, enrolled through this existing test
// target. Both servers bind ephemeral loopback ports inside the farm sandbox.
mod callback_http {
    use super::*;
    use axum::{
        Extension, Form, Json, Router,
        extract::{FromRef, State},
        routing::{get, post},
    };
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use crystal_forge::{
        auth::session::{CSRF_COOKIE_NAME, SESSION_COOKIE_NAME, hash_token},
        config::{OidcConfig, ServerConfig},
        handlers::api::auth_oidc::{OidcClientState, oidc_callback, oidc_login},
    };
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use sha2::{Digest, Sha256};
    use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
    use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};

    // Public test-only RSA fixture from jsonwebtoken v9.3.1 tests/rsa/
    // private_rsa_key_pkcs8.pem and mod.rs. Never use this published key outside
    // tests. Keeping the matching JWK here avoids external tooling at test time.
    const TEST_RSA_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDJETqse41HRBsc
7cfcq3ak4oZWFCoZlcic525A3FfO4qW9BMtRO/iXiyCCHn8JhiL9y8j5JdVP2Q9Z
IpfElcFd3/guS9w+5RqQGgCR+H56IVUyHZWtTJbKPcwWXQdNUX0rBFcsBzCRESJL
eelOEdHIjG7LRkx5l/FUvlqsyHDVJEQsHwegZ8b8C0fz0EgT2MMEdn10t6Ur1rXz
jMB/wvCg8vG8lvciXmedyo9xJ8oMOh0wUEgxziVDMMovmC+aJctcHUAYubwoGN8T
yzcvnGqL7JSh36Pwy28iPzXZ2RLhAyJFU39vLaHdljwthUaupldlNyCfa6Ofy4qN
ctlUPlN1AgMBAAECggEAdESTQjQ70O8QIp1ZSkCYXeZjuhj081CK7jhhp/4ChK7J
GlFQZMwiBze7d6K84TwAtfQGZhQ7km25E1kOm+3hIDCoKdVSKch/oL54f/BK6sKl
qlIzQEAenho4DuKCm3I4yAw9gEc0DV70DuMTR0LEpYyXcNJY3KNBOTjN5EYQAR9s
2MeurpgK2MdJlIuZaIbzSGd+diiz2E6vkmcufJLtmYUT/k/ddWvEtz+1DnO6bRHh
xuuDMeJA/lGB/EYloSLtdyCF6sII6C6slJJtgfb0bPy7l8VtL5iDyz46IKyzdyzW
tKAn394dm7MYR1RlUBEfqFUyNK7C+pVMVoTwCC2V4QKBgQD64syfiQ2oeUlLYDm4
CcKSP3RnES02bcTyEDFSuGyyS1jldI4A8GXHJ/lG5EYgiYa1RUivge4lJrlNfjyf
dV230xgKms7+JiXqag1FI+3mqjAgg4mYiNjaao8N8O3/PD59wMPeWYImsWXNyeHS
55rUKiHERtCcvdzKl4u35ZtTqQKBgQDNKnX2bVqOJ4WSqCgHRhOm386ugPHfy+8j
m6cicmUR46ND6ggBB03bCnEG9OtGisxTo/TuYVRu3WP4KjoJs2LD5fwdwJqpgtHl
yVsk45Y1Hfo+7M6lAuR8rzCi6kHHNb0HyBmZjysHWZsn79ZM+sQnLpgaYgQGRbKV
DZWlbw7g7QKBgQCl1u+98UGXAP1jFutwbPsx40IVszP4y5ypCe0gqgon3UiY/G+1
zTLp79GGe/SjI2VpQ7AlW7TI2A0bXXvDSDi3/5Dfya9ULnFXv9yfvH1QwWToySpW
Kvd1gYSoiX84/WCtjZOr0e0HmLIb0vw0hqZA4szJSqoxQgvF22EfIWaIaQKBgQCf
34+OmMYw8fEvSCPxDxVvOwW2i7pvV14hFEDYIeZKW2W1HWBhVMzBfFB5SE8yaCQy
pRfOzj9aKOCm2FjjiErVNpkQoi6jGtLvScnhZAt/lr2TXTrl8OwVkPrIaN0bG/AS
aUYxmBPCpXu3UjhfQiWqFq/mFyzlqlgvuCc9g95HPQKBgAscKP8mLxdKwOgX8yFW
GcZ0izY/30012ajdHY+/QK5lsMoxTnn0skdS+spLxaS5ZEO4qvPVb8RAoCkWMMal
2pOhmquJQVDPDLuZHdrIiKiDM20dy9sMfHygWcZjQ4WSxf/J7T9canLZIXFhHAZT
3wc9h4G8BBCtWN2TN/LsGZdB
-----END PRIVATE KEY-----";
    const TEST_RSA_N: &str = "yRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTLUTv4l4sggh5_CYYi_cvI-SXVT9kPWSKXxJXBXd_4LkvcPuUakBoAkfh-eiFVMh2VrUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8HoGfG_AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBIMc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi-yUod-j8MtvIj812dkS4QMiRVN_by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQ";

    // Each authorization code is single-use and carries the login's actual PKCE
    // challenge. A valid signed JWT alone cannot skip the code-exchange contract.
    struct Code {
        jwt: String,
        challenge: String,
    }

    #[derive(Clone)]
    struct Provider {
        issuer: String,
        codes: Arc<Mutex<HashMap<String, Code>>>,
    }

    async fn discovery(State(provider): State<Provider>) -> Json<serde_json::Value> {
        Json(json!({
            "issuer": provider.issuer,
            "authorization_endpoint": format!("{}/authorize", provider.issuer),
            "token_endpoint": format!("{}/token", provider.issuer),
            "jwks_uri": format!("{}/jwks", provider.issuer),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"],
            "token_endpoint_auth_methods_supported": ["client_secret_basic"],
            "code_challenge_methods_supported": ["S256"]
        }))
    }

    async fn jwks() -> Json<serde_json::Value> {
        Json(json!({"keys": [{
            "kty": "RSA", "n": TEST_RSA_N, "e": "AQAB",
            "kid": "callback-regression", "alg": "RS256", "use": "sig"
        }]}))
    }

    async fn token(
        State(provider): State<Provider>,
        Form(form): Form<HashMap<String, String>>,
    ) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
        use axum::http::StatusCode;
        let code = provider
            .codes
            .lock()
            .await
            .remove(form.get("code").ok_or(StatusCode::BAD_REQUEST)?)
            .ok_or(StatusCode::BAD_REQUEST)?;
        let verifier = form.get("code_verifier").ok_or(StatusCode::BAD_REQUEST)?;
        if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != code.challenge
            || form.get("grant_type").map(String::as_str) != Some("authorization_code")
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok(Json(json!({
            "access_token": "public-test-access-token", "token_type": "Bearer",
            "expires_in": 300, "id_token": code.jwt
        })))
    }

    #[derive(Clone)]
    struct CallbackState {
        pool: PgPool,
        config: ServerConfig,
    }

    impl FromRef<CallbackState> for PgPool {
        fn from_ref(state: &CallbackState) -> Self {
            state.pool.clone()
        }
    }

    impl FromRef<CallbackState> for ServerConfig {
        fn from_ref(state: &CallbackState) -> Self {
            state.config.clone()
        }
    }

    // Abort listeners on assertion failure as well as successful completion.
    struct HttpServer(JoinHandle<()>);

    impl Drop for HttpServer {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    fn serve(listener: TcpListener, app: Router) -> HttpServer {
        HttpServer(tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
        }))
    }

    struct Harness {
        base: String,
        provider: Provider,
        client: reqwest::Client,
        _servers: [HttpServer; 2],
    }

    // Flow order is captured from the sequence after the actual HTTP login,
    // before another flow is initiated. The callback cannot allocate a new one.
    struct Flow {
        code: String,
        state: String,
        cookie: String,
        generation: i64,
    }

    impl Harness {
        async fn new(pool: &PgPool) -> Result<Self> {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let provider = Provider {
                issuer: format!("http://{}", listener.local_addr()?),
                codes: Arc::default(),
            };
            let mock = serve(
                listener,
                Router::new()
                    .route("/.well-known/openid-configuration", get(discovery))
                    .route("/jwks", get(jwks))
                    .route("/token", post(token))
                    .with_state(provider.clone()),
            );
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let base = format!("http://{}", listener.local_addr()?);
            let config: OidcConfig = serde_json::from_value(json!({
                "issuer_url": provider.issuer,
                "client_id": "callback-client",
                "client_secret": "public-test-client-secret",
                "redirect_uri": format!("{base}/api/auth/oidc/callback")
            }))?;
            let oidc = Arc::new(OidcClientState::new(config).await?);
            let app = Router::new()
                .route("/api/auth/oidc/login", get(oidc_login))
                .route("/api/auth/oidc/callback", get(oidc_callback))
                .layer(Extension(oidc))
                .with_state(CallbackState {
                    pool: pool.clone(),
                    config: ServerConfig::default(),
                });
            let server = serve(listener, app);
            Ok(Self {
                base,
                provider,
                client: reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(20))
                    .build()?,
                _servers: [mock, server],
            })
        }

        async fn flow(
            &self,
            pool: &PgPool,
            email: &str,
            subject: &str,
            extra: serde_json::Value,
        ) -> Result<Flow> {
            let login = self
                .client
                .get(format!("{}/api/auth/oidc/login", self.base))
                .send()
                .await?;
            assert_eq!(login.status().as_u16(), 302);
            let cookie = login.headers()["set-cookie"]
                .to_str()?
                .split(';')
                .next()
                .unwrap()
                .to_owned();
            let url = url::Url::parse(login.headers()["location"].to_str()?)?;
            let params: HashMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(params["code_challenge_method"], "S256");
            let mut claims = json!({
                "iss": self.provider.issuer, "sub": subject, "aud": "callback-client",
                "iat": Utc::now().timestamp(), "exp": Utc::now().timestamp() + 300,
                "nonce": params["nonce"], "email": email, "email_verified": true
            });
            claims
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let mut header = Header::new(Algorithm::RS256);
            header.kid = Some("callback-regression".into());
            let jwt = encode(
                &header,
                &claims,
                &EncodingKey::from_rsa_pem(TEST_RSA_KEY.as_bytes())?,
            )?;
            let code = Uuid::new_v4().to_string();
            self.provider.codes.lock().await.insert(
                code.clone(),
                Code {
                    jwt,
                    challenge: params["code_challenge"].clone(),
                },
            );
            Ok(Flow {
                code,
                cookie,
                state: params["state"].clone(),
                generation: sqlx::query_scalar(
                    "SELECT last_value FROM oidc_entitlement_generation_seq",
                )
                .fetch_one(pool)
                .await?,
            })
        }

        async fn callback(&self, flow: &Flow) -> Result<reqwest::Response> {
            Ok(self
                .client
                .get(format!("{}/api/auth/oidc/callback", self.base))
                .query(&[("code", &flow.code), ("state", &flow.state)])
                .header("cookie", &flow.cookie)
                .send()
                .await?)
        }
    }

    async fn session_owner(pool: &PgPool, response: &reqwest::Response) -> Result<Uuid> {
        assert_eq!(response.status().as_u16(), 302);
        assert_eq!(response.headers()["location"], "/");
        let cookies: Vec<_> = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect();
        assert!(
            cookies
                .iter()
                .any(|c| c.starts_with(&format!("{CSRF_COOKIE_NAME}=")))
        );
        let prefix = format!("{SESSION_COOKIE_NAME}=");
        let token = cookies
            .iter()
            .find_map(|cookie| cookie.strip_prefix(&prefix))
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        Ok(sqlx::query_scalar(
            "SELECT user_id FROM user_sessions WHERE session_token_hash = $1
             AND invalidated_at IS NULL AND expires_at > NOW() AND auth_source = 'oidc'",
        )
        .bind(hash_token(token))
        .fetch_one(pool)
        .await?)
    }

    #[sqlx::test]
    async fn concurrent_first_callbacks_recover_identity_conflict_and_later_login(
        pool: PgPool,
    ) -> Result<()> {
        concurrent_first_callbacks(pool, false).await
    }

    #[sqlx::test]
    async fn concurrent_identical_profile_callbacks_recover_binding_and_later_login(
        pool: PgPool,
    ) -> Result<()> {
        concurrent_first_callbacks(pool, true).await
    }

    // Both scenarios must enter creation after an absent callback lookup. The
    // parent holds the exact per-key lock until both creation transactions wait.
    async fn concurrent_first_callbacks(pool: PgPool, identical_profile: bool) -> Result<()> {
        let harness = Harness::new(&pool).await?;
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await?;
        let preferences_before: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM user_notification_preferences")
                .fetch_one(&pool)
                .await?;
        // Identical profiles exercise the ordinary username/email race. Different
        // snapshots also prove recovery never rewrites the winner's profile.
        let first = harness
            .flow(
                &pool,
                "race-first@example.invalid",
                "race-sub",
                json!({"groups": [], "marker": "first"}),
            )
            .await?;
        let second = harness
            .flow(
                &pool,
                if identical_profile {
                    "race-first@example.invalid"
                } else {
                    "race-second@example.invalid"
                },
                "race-sub",
                json!({"groups": [], "marker": if identical_profile { "first" } else { "second" }}),
            )
            .await?;
        assert!(second.generation > first.generation);
        let creation_key: i64 = sqlx::query_scalar(
            "SELECT hashtextextended(
                 jsonb_build_array('oidc'::text, 'race-sub'::text, $1::text)::text, 0)",
        )
        .bind(&harness.provider.issuer)
        .fetch_one(&pool)
        .await?;
        let mut gate = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(creation_key)
            .execute(&mut *gate)
            .await?;
        let (responses, released) = tokio::join!(
            async { tokio::join!(harness.callback(&first), harness.callback(&second)) },
            async {
                // A bigint advisory key is split across classid/objid in
                // pg_locks; objsubid=1 distinguishes it from two-int keys.
                // Both callbacks passed their initial absent lookup, but neither
                // can insert a user until this gate releases. The loser must
                // recover IdentityAlreadyBound, not merely find Some initially.
                wait_for_lock(
                    &pool,
                    &format!(
                        "SELECT COUNT(*) = 2 FROM pg_locks WHERE locktype = 'advisory'
                         AND classid::bigint = {} AND objid::bigint = {}
                         AND objsubid = 1 AND database = (SELECT oid FROM pg_database
                             WHERE datname = current_database()) AND NOT granted",
                        (creation_key >> 32) & 0xffff_ffff,
                        creation_key & 0xffff_ffff,
                    ),
                )
                .await?;
                gate.rollback().await?;
                Ok::<_, anyhow::Error>(())
            }
        );
        released?;
        let (first_response, second_response) = (responses.0?, responses.1?);
        for response in [&first_response, &second_response] {
            assert!(matches!(response.status().as_u16(), 302 | 403));
        }
        // Only the older flow may be superseded. Recovery must let the newest
        // callback complete, regardless of which transaction created the user.
        assert_eq!(second_response.status().as_u16(), 302);
        if first_response.status().as_u16() == 403 {
            assert!(first_response.headers().get("set-cookie").is_none());
        }
        let (identity, owner): (Uuid, Uuid) = sqlx::query_as(
            "SELECT id, user_id FROM external_identities WHERE provider_key = 'oidc'
             AND subject = 'race-sub' AND tenant_discriminator = $1",
        )
        .bind(&harness.provider.issuer)
        .fetch_one(&pool)
        .await?;
        for response in [&first_response, &second_response] {
            if response.status().as_u16() == 302 {
                assert_eq!(session_owner(&pool, response).await?, owner);
            }
        }
        let accepted: i64 = sqlx::query_scalar(
            "SELECT highest_accepted_entitlement_generation FROM external_identities WHERE id = $1",
        )
        .bind(identity)
        .fetch_one(&pool)
        .await?;
        assert_eq!(accepted, second.generation);
        let last = harness
            .flow(
                &pool,
                "race-latest@example.invalid",
                "race-sub",
                json!({"groups": [], "marker": "last"}),
            )
            .await?;
        assert_eq!(
            session_owner(&pool, &harness.callback(&last).await?).await?,
            owner
        );
        let persisted: (i64, serde_json::Value) = sqlx::query_as(
            "SELECT highest_accepted_entitlement_generation, claims
             FROM external_identities WHERE id = $1 AND user_id = $2",
        )
        .bind(identity)
        .bind(owner)
        .fetch_one(&pool)
        .await?;
        assert_eq!(persisted.0, last.generation);
        assert_eq!(persisted.1["marker"], json!("last"));
        assert_eq!(roles(&pool, owner).await?, ["viewer"]);
        require_fresh_entitlements(&pool, owner, identity, 3600).await?;
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await?;
        assert_eq!(after, before + 1);
        let bindings: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM external_identities WHERE subject = 'race-sub'",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(bindings, 1);
        let prefs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_notification_preferences")
            .fetch_one(&pool)
            .await?;
        assert_eq!(prefs, preferences_before + 1);
        let profile_email: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
            .bind(owner)
            .fetch_one(&pool)
            .await?;
        if identical_profile {
            assert_eq!(profile_email, "race-first@example.invalid");
        } else {
            assert!(matches!(
                profile_email.as_str(),
                "race-first@example.invalid" | "race-second@example.invalid"
            ));
        }
        Ok(())
    }

    #[sqlx::test]
    async fn callback_recovers_real_identity_index_conflict_from_nonlocking_writer(
        pool: PgPool,
    ) -> Result<()> {
        use crystal_forge::queries::auth_identity::{AuthIdentityRepository, NewExternalIdentity};

        let harness = Harness::new(&pool).await?;
        let owner = user(&pool).await?;
        let profile_before: serde_json::Value =
            sqlx::query_scalar("SELECT to_jsonb(u) FROM users u WHERE id = $1")
                .bind(owner)
                .fetch_one(&pool)
                .await?;
        let users_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await?;
        let preferences_before: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM user_notification_preferences")
                .fetch_one(&pool)
                .await?;
        let sessions_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_sessions")
            .fetch_one(&pool)
            .await?;
        let flow = harness
            .flow(
                &pool,
                "index-conflict-candidate@example.invalid",
                "index-conflict-sub",
                json!({"groups": [], "marker": "validated-callback"}),
            )
            .await?;
        // This gate is distinct from the bigint creation lock (objsubid=2).
        // Only the callback candidate waits: the trigger sees its own freshly
        // inserted user, while the public upsert for the existing owner bypasses
        // this gate and does not acquire the callback's creation lock.
        sqlx::raw_sql(
            "CREATE FUNCTION gate_callback_candidate() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN
                 IF EXISTS (SELECT 1 FROM users WHERE id = NEW.user_id
                            AND email = 'index-conflict-candidate@example.invalid') THEN
                     PERFORM pg_advisory_xact_lock(529, 34501);
                 END IF;
                 RETURN NEW;
             END $$;
             CREATE TRIGGER gate_callback_candidate BEFORE INSERT ON external_identities
             FOR EACH ROW EXECUTE FUNCTION gate_callback_candidate();",
        )
        .execute(&pool)
        .await?;
        let mut gate = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(529, 34501)")
            .execute(&mut *gate)
            .await?;
        let (response, winning_identity) = tokio::join!(harness.callback(&flow), async {
            // A waiting trigger proves the callback passed both absent-key
            // lookups and inserted its candidate user/preferences. The later
            // winner therefore forces an actual unique-index violation, rather
            // than IdentityAlreadyBound or the initial Some(identity) branch.
            wait_for_lock(
                &pool,
                "SELECT COUNT(*) = 1 FROM pg_locks WHERE locktype = 'advisory'
                 AND classid = 529 AND objid = 34501 AND objsubid = 2
                 AND database = (SELECT oid FROM pg_database
                     WHERE datname = current_database()) AND NOT granted",
            )
            .await?;
            let repo = AuthIdentityRepository::new(&pool);
            assert!(
                repo.find_external_identity(
                    "oidc",
                    "index-conflict-sub",
                    Some(&harness.provider.issuer),
                )
                .await?
                .is_none()
            );
            let record = NewExternalIdentity {
                user_id: owner,
                provider_key: "oidc".into(),
                subject: "index-conflict-sub".into(),
                tenant_discriminator: Some(harness.provider.issuer.clone()),
                claims: json!({"marker": "concurrent-writer"}),
            };
            let winner = tokio::time::timeout(
                Duration::from_secs(10),
                repo.upsert_external_identity(&record),
            )
            .await??;
            assert_eq!(winner.user_id, owner);
            assert_eq!(state(&pool, winner.id).await?, ("missing".into(), None));
            // The winning binding is committed before the candidate proceeds to
            // PostgreSQL's real uniqueness check. No fabricated error is raised.
            gate.rollback().await?;
            Ok::<_, anyhow::Error>(winner.id)
        });
        let identity = winning_identity?;
        let response = response?;
        assert_eq!(session_owner(&pool, &response).await?, owner);
        let bindings: Vec<(Uuid, Uuid, i64, serde_json::Value)> = sqlx::query_as(
            "SELECT id, user_id, highest_accepted_entitlement_generation, claims
             FROM external_identities WHERE provider_key = 'oidc'
               AND subject = 'index-conflict-sub' AND tenant_discriminator = $1",
        )
        .bind(&harness.provider.issuer)
        .fetch_all(&pool)
        .await?;
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            (bindings[0].0, bindings[0].1, bindings[0].2),
            (identity, owner, flow.generation)
        );
        assert_eq!(bindings[0].3["marker"], json!("validated-callback"));
        require_fresh_entitlements(&pool, owner, identity, 3600).await?;
        let profile_after: serde_json::Value =
            sqlx::query_scalar("SELECT to_jsonb(u) FROM users u WHERE id = $1")
                .bind(owner)
                .fetch_one(&pool)
                .await?;
        assert_eq!(profile_after, profile_before);
        let users_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await?;
        let preferences_after: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM user_notification_preferences")
                .fetch_one(&pool)
                .await?;
        assert_eq!(users_after, users_before);
        assert_eq!(preferences_after, preferences_before);
        let candidates: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE email = 'index-conflict-candidate@example.invalid'",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(candidates, 0);
        let sessions_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_sessions")
            .fetch_one(&pool)
            .await?;
        assert_eq!(sessions_after, sessions_before + 1);
        sqlx::raw_sql(
            "DROP TRIGGER gate_callback_candidate ON external_identities;
             DROP FUNCTION gate_callback_candidate();",
        )
        .execute(&pool)
        .await?;
        Ok(())
    }

    #[sqlx::test]
    async fn callback_missing_and_malformed_groups_deny_managed_but_preserve_legacy(
        pool: PgPool,
    ) -> Result<()> {
        let harness = Harness::new(&pool).await?;
        let environment: Uuid =
            sqlx::query_scalar("SELECT id FROM environments WHERE name = 'prod'")
                .fetch_one(&pool)
                .await?;
        for (index, evidence) in [json!({}), json!({"groups": ["operator", 7]})]
            .into_iter()
            .enumerate()
        {
            let subject = format!("missing-or-malformed-{index}");
            let flow = harness
                .flow(
                    &pool,
                    &format!("claims-{index}@example.invalid"),
                    &subject,
                    evidence.clone(),
                )
                .await?;
            let denied = harness.callback(&flow).await?;
            assert_eq!(denied.status().as_u16(), 403);
            assert!(denied.headers().get("set-cookie").is_none());
            let (identity, owner): (Uuid, Uuid) =
                sqlx::query_as("SELECT id, user_id FROM external_identities WHERE subject = $1")
                    .bind(&subject)
                    .fetch_one(&pool)
                    .await?;
            assert_eq!(state(&pool, identity).await?, ("invalid".into(), None));
            assert!(roles(&pool, owner).await?.is_empty());
            let initial_claims: serde_json::Value =
                sqlx::query_scalar("SELECT claims FROM external_identities WHERE id = $1")
                    .bind(identity)
                    .fetch_one(&pool)
                    .await?;
            // Explicit unresolved rows model an upgraded pre-source account.
            sqlx::query(
                "INSERT INTO user_role_assignments
                 (user_id, role, local_grant, legacy_ownership_unresolved)
                 VALUES ($1, 'operator', false, true)",
            )
            .bind(owner)
            .execute(&pool)
            .await?;
            sqlx::query(
                "INSERT INTO user_environment_memberships
                 (user_id, environment_id, local_grant, legacy_ownership_unresolved)
                 VALUES ($1, $2, false, true)",
            )
            .bind(owner)
            .bind(environment)
            .execute(&pool)
            .await?;
            let legacy = harness
                .flow(&pool, "changed-profile@example.invalid", &subject, evidence)
                .await?;
            assert_eq!(
                session_owner(&pool, &harness.callback(&legacy).await?).await?,
                owner
            );
            assert_eq!(roles(&pool, owner).await?, ["operator"]);
            let membership: (Uuid, bool, bool, Option<Uuid>) = sqlx::query_as(
                "SELECT environment_id, local_grant, legacy_ownership_unresolved, oidc_identity_id
                 FROM user_environment_memberships WHERE user_id = $1",
            )
            .bind(owner)
            .fetch_one(&pool)
            .await?;
            assert_eq!(membership, (environment, false, true, None));
            assert_eq!(
                state(&pool, identity).await?,
                ("legacy_unresolved".into(), None)
            );
            let unchanged: serde_json::Value =
                sqlx::query_scalar("SELECT claims FROM external_identities WHERE id = $1")
                    .bind(identity)
                    .fetch_one(&pool)
                    .await?;
            assert_eq!(unchanged, initial_claims);
            assert!(
                require_fresh_entitlements(&pool, owner, identity, 3600)
                    .await
                    .is_err()
            );
        }
        Ok(())
    }

    #[sqlx::test]
    async fn callback_does_not_recover_other_constraints_or_database_errors(
        pool: PgPool,
    ) -> Result<()> {
        use crystal_forge::queries::auth_identity::create_user_and_bind_external_identity;

        let harness = Harness::new(&pool).await?;
        let original = create_user_and_bind_external_identity(
            &pool,
            "occupied@example.invalid",
            None,
            "oidc",
            "original-sub",
            Some(&harness.provider.issuer),
            json!({"original": true}),
        )
        .await?;
        sqlx::query("UPDATE users SET username = 'occupied-local' WHERE id = $1")
            .bind(original.id)
            .execute(&pool)
            .await?;
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await?;
        // Real email and username constraints must not become account linking.
        for email in ["occupied@example.invalid", "occupied-local@another.invalid"] {
            let flow = harness
                .flow(&pool, email, "must-not-link", json!({"groups": []}))
                .await?;
            let response = harness.callback(&flow).await?;
            assert_eq!(response.status().as_u16(), 500);
            assert!(response.headers().get("set-cookie").is_none());
            assert_eq!(
                response.text().await?,
                "Database error during user lookup/creation"
            );
        }
        // Both SQLSTATE and constraint must match. Even an error message that
        // contains the identity index name is insufficient for recovery.
        for (code, constraint) in [
            ("23514", "idx_external_identities_provider_subject_tenant"),
            ("23505", "another_unique_constraint"),
            ("P0001", "idx_external_identities_provider_subject_tenant"),
        ] {
            sqlx::raw_sql(&format!(
                "CREATE OR REPLACE FUNCTION reject_callback_binding() RETURNS trigger LANGUAGE plpgsql AS $$
                 BEGIN RAISE EXCEPTION 'idx_external_identities_provider_subject_tenant'
                 USING ERRCODE = '{code}', CONSTRAINT = '{constraint}'; END $$;"
            )).execute(&pool).await?;
            sqlx::raw_sql(
                "DROP TRIGGER IF EXISTS reject_callback_binding ON external_identities;
                 CREATE TRIGGER reject_callback_binding BEFORE INSERT ON external_identities
                 FOR EACH ROW EXECUTE FUNCTION reject_callback_binding();",
            )
            .execute(&pool)
            .await?;
            let flow = harness
                .flow(
                    &pool,
                    "db-failure@example.invalid",
                    "db-failure-sub",
                    json!({"groups": []}),
                )
                .await?;
            let response = harness.callback(&flow).await?;
            assert_eq!(response.status().as_u16(), 500);
            assert!(response.headers().get("set-cookie").is_none());
            assert_eq!(
                response.text().await?,
                "Database error during user lookup/creation"
            );
        }
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await?;
        assert_eq!(after, before);
        let bindings: Vec<(Uuid, String, serde_json::Value)> =
            sqlx::query_as("SELECT user_id, subject, claims FROM external_identities")
                .fetch_all(&pool)
                .await?;
        assert_eq!(
            bindings,
            [(
                original.id,
                "original-sub".into(),
                json!({"original": true})
            )]
        );
        // A valid token cannot revive the winner after local deactivation.
        sqlx::query("UPDATE users SET is_active = false WHERE id = $1")
            .bind(original.id)
            .execute(&pool)
            .await?;
        let inactive = harness
            .flow(
                &pool,
                "occupied@example.invalid",
                "original-sub",
                json!({"groups": []}),
            )
            .await?;
        let response = harness.callback(&inactive).await?;
        assert_eq!(response.status().as_u16(), 403);
        assert!(response.headers().get("set-cookie").is_none());
        let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_sessions")
            .fetch_one(&pool)
            .await?;
        assert_eq!(sessions, 0);
        Ok(())
    }
}
