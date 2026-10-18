//! Migration-backed authorization regressions for the production matrix reader.

use super::*;
use serde_json::json;

struct Fixture {
    commit: i32,
    users: [Uuid; 2],
    environments: [Uuid; 2],
    hosts: [Uuid; 2],
    plan: EvalPolicyMatrixSnapshot,
}

async fn fixture(pool: &PgPool, shared: bool) -> Fixture {
    let (commit, attempt, number) = tests::commit(pool).await;
    let mut users = [Uuid::nil(); 2];
    let mut environments = [Uuid::nil(); 2];
    let mut hosts = [Uuid::nil(); 2];
    let mut plan = tests::plan();
    plan.evaluation_attempt_id = Some(attempt);
    plan.attempt_number = number;
    let hidden_policy = Uuid::new_v4();
    plan.columns.push(EvalPolicyColumn {
        policy_version_id: hidden_policy,
        name: "hidden-only-policy".into(),
        ..plan.columns[0].clone()
    });
    for i in 0..2 {
        users[i] = sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email) VALUES($1,'Matrix','Viewer',$2) RETURNING id")
            .bind(format!("matrix-{}", Uuid::new_v4()))
            .bind(format!("{}@example.invalid", Uuid::new_v4()))
            .fetch_one(pool).await.unwrap();
        environments[i] =
            sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
                .bind(format!("matrix-env-{}", Uuid::new_v4()))
                .fetch_one(pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
        )
        .bind(users[i])
        .bind(environments[i])
        .execute(pool)
        .await
        .unwrap();
        let config = if shared || i == 0 {
            "shared"
        } else {
            "hidden-config"
        };
        hosts[i] = sqlx::query_scalar("INSERT INTO systems(hostname,public_key,derivation,environment_id,flake_id,system_configuration_name) SELECT $1,'test-key','test-drv',$2,c.flake_id,$3 FROM commits c WHERE c.id=$4 RETURNING systems.id")
            .bind(format!("matrix-host-{i}-{}", Uuid::new_v4()))
            .bind(environments[i]).bind(config).bind(commit)
            .fetch_one(pool).await.unwrap();
        plan.rows[i].system_id = Some(hosts[i]);
        plan.rows[i].system_name = format!("captured-host-{i}");
        plan.rows[i].configuration_name = config.into();
        // Each host owns one context; hidden context and policy must disappear
        // even when both hosts use the same deduplicated evaluator outcome.
        plan.rows[i].assigned_bundles = vec![plan.rows[i].assigned_bundles[i].clone()];
        plan.rows[i].policies[0].memberships = plan.rows[i].assigned_bundles.clone();
        if i == 1 {
            plan.rows[i].policies[0].policy_version_id = hidden_policy;
        }
    }
    let mut tx = pool.begin().await.unwrap();
    insert_plan(&mut tx, commit, &plan).await.unwrap();
    tx.commit().await.unwrap();
    let names = if shared {
        vec!["shared"]
    } else {
        vec!["hidden-config", "shared"]
    };
    tests::inventory(
        pool,
        commit,
        number,
        &names,
        crate::queries::evaluation_inventory::InventoryExecutionMode::Mock,
    )
    .await;
    let document = json!({"evaluation_attempt": number, "execution_mode":"mock",
    "global":{"cfAgentEnabled":{"passed":false,"details":"configuration-wide gate diagnostic"}},
    "assigned":{
        plan.columns[0].policy_version_id.to_string(): {"passed":true,"name":"visible-policy","details":"visible detail"},
        hidden_policy.to_string(): {"passed":false,"blocking":true,"name":"hidden-only-policy","details":"hidden-only-detail"}
    }});
    for config in if shared {
        vec!["shared"]
    } else {
        vec!["shared", "hidden-config"]
    } {
        sqlx::query("INSERT INTO evaluation_mock_outcomes(evaluation_attempt_id,commit_id,attempt_number,configuration_name,policy_results,cf_agent_enabled,policy_requirements_met) VALUES($1,$2,$3,$4,$5,FALSE,FALSE)")
            .bind(attempt).bind(commit).bind(number).bind(config).bind(&document)
            .execute(pool).await.unwrap();
    }
    sqlx::query("UPDATE evaluation_attempts SET status='complete' WHERE commit_id=$1")
        .bind(commit)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE commits SET evaluation_status='complete' WHERE id=$1")
        .bind(commit)
        .execute(pool)
        .await
        .unwrap();
    Fixture {
        commit,
        users,
        environments,
        hosts,
        plan,
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn mixed_flake_projects_all_metadata_and_shared_configuration(pool: PgPool) {
    for shared in [false, true] {
        let f = fixture(&pool, shared).await;
        // The production loader trims nonblank configuration names. Scope
        // checks must preserve access to that same captured configuration.
        sqlx::query("UPDATE systems SET system_configuration_name=' shared ' WHERE id=$1")
            .bind(f.hosts[0])
            .execute(&pool)
            .await
            .unwrap();
        for i in 0..2 {
            let (flat, matrix) = fetch_matrix_for_user(&pool, f.commit, Some(f.users[i]))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(matrix.rows.len(), 1);
            assert_eq!(matrix.rows[0].system_id, Some(f.hosts[i]));
            assert_eq!(matrix.columns.len(), 1);
            assert_eq!(
                matrix.columns[0].policy_version_id,
                f.plan.rows[i].policies[0].policy_version_id
            );
            assert_eq!(matrix.bundles.len(), 1);
            assert_eq!(
                matrix.bundles[0].bundle_version_id,
                f.plan.rows[i].assigned_bundles[0].bundle_version_id
            );
            assert_eq!(matrix.rows[0].assigned_bundles.len(), 1);
            assert_eq!(matrix.rows[0].policies[0].memberships.len(), 1);
            assert!(matrix.mock_evidence);
            assert_eq!(matrix.rows[0].cells.len(), 1);
            assert_eq!(matrix.rows[0].details.len(), 1);
            assert_eq!(
                matrix.rows[0].cells,
                vec![if i == 0 {
                    EvalMatrixCell::Pass
                } else {
                    EvalMatrixCell::Fail
                }]
            );
            let (_, pinned) = fetch_matrix_for_user_at_attempt(
                &pool,
                f.commit,
                Some(f.users[i]),
                f.plan.evaluation_attempt_id.unwrap(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(pinned.rows[0].system_id, Some(f.hosts[i]));
            assert_eq!(
                pinned.rows[0].configuration_name,
                matrix.rows[0].configuration_name
            );
            assert_eq!(flat.len(), 1);
            assert_eq!(flat[0].system_name, format!("captured-host-{i}"));
            if shared {
                assert!(flat[0].policy_results.get("global").is_none());
                assert!(flat[0].policy_requirements_met.is_none());
                assert!(flat[0].error_message.is_none());
                assert_eq!(matrix.configuration_failures.len(), 1);
                assert!(matrix.configuration_failures[0].detail.is_empty());
                let summary = matrix.configuration_summary.as_ref().unwrap();
                assert_eq!((summary.total, summary.pending, summary.failed), (1, 0, 1));
                assert!(
                    !format!("{flat:?} {pinned:?} {matrix:?}")
                        .contains("configuration-wide gate diagnostic")
                );
            } else {
                assert_eq!(
                    flat[0].policy_results["global"]["cfAgentEnabled"]["passed"],
                    false
                );
                assert_eq!(flat[0].policy_requirements_met, Some(false));
            }
            assert_eq!(
                flat[0].policy_results["assigned"]
                    .as_object()
                    .unwrap()
                    .len(),
                1
            );
            if i == 0 {
                let wire = format!("{flat:?} {}", serde_json::to_string(&matrix).unwrap());
                for hidden in [
                    "captured-host-1",
                    "hidden-config",
                    "hidden-only-policy",
                    "hidden-only-detail",
                ] {
                    assert!(!wire.contains(hidden), "leaked {hidden}");
                }
            }
        }
        let (flat, admin) = fetch_matrix(&pool, f.commit).await.unwrap();
        assert_eq!(admin.rows.len(), 2);
        assert_eq!(admin.columns.len(), 2);
        assert_eq!(admin.bundles.len(), 2);
        assert_eq!(flat.len(), if shared { 1 } else { 2 });
        assert_eq!(
            flat[0].policy_results["global"]["cfAgentEnabled"]["passed"],
            false
        );
        assert_eq!(
            admin.configuration_failures.len(),
            if shared { 1 } else { 2 }
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn foreign_missing_and_removed_membership_are_nondisclosing(pool: PgPool) {
    let f = fixture(&pool, false).await;
    let (foreign, _, _) = tests::commit(&pool).await;
    for commit in [foreign, i32::MAX] {
        assert!(
            fetch_matrix_for_user(&pool, commit, Some(f.users[0]))
                .await
                .unwrap()
                .is_none()
        );
    }
    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1")
        .bind(f.users[0])
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        fetch_matrix_for_user(&pool, f.commit, Some(f.users[0]))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(fetch_matrix(&pool, f.commit).await.unwrap().1.rows.len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn deleted_moved_and_reconfigured_hosts_fail_closed(pool: PgPool) {
    for mutation in ["delete", "environment", "configuration", "flake"] {
        let f = fixture(&pool, false).await;
        match mutation {
            "delete" => {
                sqlx::query("DELETE FROM systems WHERE id=$1")
                    .bind(f.hosts[0])
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "environment" => {
                sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
                    .bind(f.hosts[0])
                    .bind(f.environments[1])
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "configuration" => {
                sqlx::query(
                    "UPDATE systems SET system_configuration_name='replacement' WHERE id=$1",
                )
                .bind(f.hosts[0])
                .execute(&pool)
                .await
                .unwrap();
            }
            _ => {
                let (other, _, _) = tests::commit(&pool).await;
                sqlx::query("UPDATE systems SET flake_id=(SELECT flake_id FROM commits WHERE id=$2) WHERE id=$1")
                    .bind(f.hosts[0]).bind(other).execute(&pool).await.unwrap();
            }
        }
        if let Some((flat, matrix)) = fetch_matrix_for_user(&pool, f.commit, Some(f.users[0]))
            .await
            .unwrap()
        {
            assert!(flat.is_empty());
            assert!(
                matrix.rows.is_empty() && matrix.columns.is_empty() && matrix.bundles.is_empty()
            );
            assert!(!matrix.mock_evidence);
        }
        // Even a user who can now see the moved host cannot recover a deleted
        // identity or one that moved to another flake/configuration.
        if mutation != "environment" {
            let (_, matrix) = fetch_matrix_for_user(&pool, f.commit, Some(f.users[1]))
                .await
                .unwrap()
                .unwrap();
            assert!(
                matrix
                    .rows
                    .iter()
                    .all(|row| row.system_id != Some(f.hosts[0]))
            );
        }
        assert_eq!(fetch_matrix(&pool, f.commit).await.unwrap().1.rows.len(), 2);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn authorization_and_evidence_share_repeatable_read_snapshot(pool: PgPool) {
    let f = fixture(&pool, false).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .unwrap();
    let scope = scoped_read::authorize(&mut tx, f.commit, Some(f.users[0]))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1")
        .bind(f.users[0])
        .execute(&pool)
        .await
        .unwrap();
    let payload: Value =
        sqlx::query_scalar("SELECT payload FROM evaluation_policy_plans WHERE commit_id=$1")
            .bind(f.commit)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    let mut plan: EvalPolicyMatrixSnapshot = serde_json::from_value(payload).unwrap();
    scoped_read::project(&scope, &mut plan, &mut vec![]);
    assert_eq!(plan.rows.len(), 1);
    assert!(
        scoped_read::authorize(&mut tx, f.commit, Some(f.users[0]))
            .await
            .unwrap()
            .is_some()
    );
    tx.commit().await.unwrap();
    assert!(
        fetch_matrix_for_user(&pool, f.commit, Some(f.users[0]))
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn legacy_configuration_names_cannot_grant_historical_host_access(pool: PgPool) {
    let f = fixture(&pool, true).await;
    let legacy: i32 = sqlx::query_scalar(
        "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp)
         SELECT flake_id,$2,NOW() FROM commits WHERE id=$1 RETURNING id",
    )
    .bind(f.commit)
    .bind(Uuid::new_v4().to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO derivations(commit_id,derivation_name,derivation_type,status_id,policy_results,policy_requirements_met) VALUES($1,'shared','nixos',5,'{\"assigned\":{\"hidden\":{\"name\":\"hidden-policy\",\"passed\":true}}}',TRUE)")
        .bind(legacy).execute(&pool).await.unwrap();
    let scoped = fetch_matrix_for_user(&pool, legacy, Some(f.users[0]))
        .await
        .unwrap();
    assert!(
        scoped.is_none(),
        "current flake membership cannot authorize unproven history"
    );
    let (flat, admin) = fetch_matrix(&pool, legacy).await.unwrap();
    assert_eq!(admin.provenance, EvalMatrixProvenance::LegacyUnavailable);
    assert!(admin.rows.is_empty());
    assert!(admin.inventory.is_none());
    assert!(admin.configuration_summary.is_none());
    assert!(flat.is_empty());
    let stored: Value =
        sqlx::query_scalar("SELECT policy_results FROM derivations WHERE commit_id=$1")
            .bind(legacy)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(stored["assigned"].get("hidden").is_some());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn pinned_reads_preserve_scope_and_revoke_historical_access(pool: PgPool) {
    let f = fixture(&pool, false).await;
    let id = f.plan.evaluation_attempt_id.unwrap();
    sqlx::query("UPDATE commits SET evaluation_error_message='hidden environment global failure' WHERE id=$1")
        .bind(f.commit).execute(&pool).await.unwrap();
    let (_, matrix) = fetch_matrix_for_user_at_attempt(&pool, f.commit, Some(f.users[0]), id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(matrix.rows.len(), 1);
    assert_eq!(matrix.rows[0].system_id, Some(f.hosts[0]));
    assert_eq!(matrix.columns.len(), 1);
    let wire = serde_json::to_string(&matrix).unwrap();
    assert!(!wire.contains("hidden-only") && !wire.contains("captured-host-1"));
    assert!(!wire.contains("hidden environment global failure"));
    let (foreign, foreign_id, _) = tests::commit(&pool).await;
    for (commit, attempt) in [
        (foreign, foreign_id),
        (f.commit, foreign_id),
        (i32::MAX, id),
    ] {
        assert!(
            fetch_matrix_for_user_at_attempt(&pool, commit, Some(f.users[0]), attempt)
                .await
                .unwrap()
                .is_none()
        );
    }
    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1")
        .bind(f.users[0])
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        fetch_matrix_for_user_at_attempt(&pool, f.commit, Some(f.users[0]), id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fetch_matrix_for_user_at_attempt(&pool, f.commit, None, id)
            .await
            .unwrap()
            .unwrap()
            .1
            .rows
            .len(),
        2
    );
}
