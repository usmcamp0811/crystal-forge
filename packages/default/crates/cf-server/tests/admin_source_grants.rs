//! Proves administrator classification preserves the effective source union.
//! SQLx supplies isolated databases with the full production migrations.

use anyhow::Result;
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
