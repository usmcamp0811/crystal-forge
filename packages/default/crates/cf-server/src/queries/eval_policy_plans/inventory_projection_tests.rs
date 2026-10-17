//! Exact-attempt inventory regressions for the production matrix reader.
//!
//! SQLx cases require a disposable migration-backed database. Inventory always
//! uses the production writer while attempt seven is active; native outcome
//! documents are fixtures, not proof that a Nix evaluator executed.

use super::*;
use crate::queries::evaluation_inventory::{InventoryExecutionMode, persist_inventory};
use serde_json::json;

struct Fixture {
    commit: i32,
    attempt: Uuid,
    users: [Uuid; 2],
    plan: EvalPolicyMatrixSnapshot,
}

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

async fn fixture(pool: &PgPool, configurations: &[String]) -> Fixture {
    let (commit, attempt, _) = tests::commit(pool).await;
    // The shared helper starts a real attempt. Renumber before either immutable
    // capture exists so the inventory writer must validate ownership of seven.
    sqlx::query("UPDATE evaluation_attempts SET attempt_number=7 WHERE id=$1")
        .bind(attempt)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE commits SET evaluation_attempt_count=7 WHERE id=$1")
        .bind(commit)
        .execute(pool)
        .await
        .unwrap();
    let mut users = [Uuid::nil(); 2];
    let mut environments = [Uuid::nil(); 2];
    for i in 0..2 {
        users[i] = sqlx::query_scalar(
            "INSERT INTO users(username,first_name,last_name,email)
             VALUES($1,'Inventory','Viewer',$2) RETURNING id",
        )
        .bind(format!("inventory-{}", Uuid::new_v4()))
        .bind(format!("{}@example.invalid", Uuid::new_v4()))
        .fetch_one(pool)
        .await
        .unwrap();
        environments[i] =
            sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
                .bind(format!("inventory-env-{}", Uuid::new_v4()))
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
    }
    let mut plan = tests::plan();
    plan.evaluation_attempt_id = Some(attempt);
    plan.attempt_number = 7;
    plan.columns[0].name = "visible-policy".into();
    plan.bundles[0].name = "visible-bundle".into();
    plan.bundles[1].name = "hidden-only-bundle".into();
    let hidden = Uuid::new_v4();
    plan.columns.push(EvalPolicyColumn {
        policy_version_id: hidden,
        name: "hidden-only-policy".into(),
        ..plan.columns[0].clone()
    });
    let template = plan.rows[0].clone();
    plan.rows.clear();
    for (i, configuration) in configurations.iter().enumerate() {
        // Give the last host an independent environment and policy, including
        // when two hosts intentionally share the same evaluator configuration.
        let scope = usize::from(i + 1 == configurations.len());
        let hostname = format!(
            "{}-host-{i}-{commit}",
            if scope == 0 { "visible" } else { "hidden-only" }
        );
        let id = sqlx::query_scalar(
            "INSERT INTO systems(hostname,public_key,derivation,environment_id,
                 flake_id,system_configuration_name)
             SELECT $1,'test-key','test-drv',$2,c.flake_id,$3
             FROM commits c WHERE c.id=$4 RETURNING systems.id",
        )
        .bind(&hostname)
        .bind(environments[scope])
        .bind(configuration)
        .bind(commit)
        .fetch_one(pool)
        .await
        .unwrap();
        let mut row = template.clone();
        row.system_id = Some(id);
        row.system_name = hostname;
        row.configuration_name = configuration.clone();
        row.assigned_bundles = vec![template.assigned_bundles[scope].clone()];
        row.policies[0].policy_version_id = plan.columns[scope].policy_version_id;
        row.policies[0].memberships = row.assigned_bundles.clone();
        plan.rows.push(row);
    }
    let mut tx = pool.begin().await.unwrap();
    insert_plan(&mut tx, commit, &plan).await.unwrap();
    tx.commit().await.unwrap();
    Fixture {
        commit,
        attempt,
        users,
        plan,
    }
}

async fn inventory(pool: &PgPool, f: &Fixture, discovered: &[String], selected: &[String]) {
    persist_inventory(
        pool,
        f.commit,
        7,
        discovered,
        selected,
        InventoryExecutionMode::Native,
    )
    .await
    .unwrap();
}

async fn native(pool: &PgPool, f: &Fixture, name: &str, marker: i32, agent: bool) {
    let mut assigned = serde_json::Map::new();
    for column in &f.plan.columns {
        assigned.insert(
            column.policy_version_id.to_string(),
            json!({
                "passed": true, "details": format!("{} detail", column.name),
            }),
        );
    }
    let document = json!({
        "evaluation_attempt": marker,
        "global": {"cfAgentEnabled": {
            "passed": agent,
            "details": if agent { "agent enabled".to_owned() }
                else { format!("saved global failure for {name}") }
        }},
        "assigned": assigned,
    });
    // Match native reader evidence: a completed NixOS evaluation, exact numeric
    // marker, UUID-keyed assigned results and explicit configuration gate flags.
    sqlx::query(
        "INSERT INTO derivations(commit_id,derivation_name,derivation_type,status_id,
             policy_results,cf_agent_enabled,policy_requirements_met)
         VALUES($1,$2,'nixos',5,$3,$4,$4)",
    )
    .bind(f.commit)
    .bind(name)
    .bind(document)
    .bind(agent)
    .execute(pool)
    .await
    .unwrap();
}

async fn finish(pool: &PgPool, f: &Fixture) {
    sqlx::query("UPDATE evaluation_attempts SET status='complete',completed_at=NOW() WHERE id=$1")
        .bind(f.attempt)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE commits SET evaluation_status='complete' WHERE id=$1")
        .bind(f.commit)
        .execute(pool)
        .await
        .unwrap();
}

fn assert_unknown(row: &EvalMatrixRow, columns: usize) {
    assert_eq!(row.cells, vec![EvalMatrixCell::Unknown; columns]);
    assert_eq!(row.details, vec![None; columns]);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn inventory_discovered_41_preserves_exact_names_and_12_captured_hosts(pool: PgPool) {
    let discovered: Vec<String> = (0..41).map(|i| format!("config-{i:02}-Case_é")).collect();
    let captured = discovered[..12].to_vec();
    let f = fixture(&pool, &captured).await;
    // Cache names deliberately disagree with both discovery and captured hosts.
    sqlx::query("INSERT INTO commit_artifacts_cache(commit_id,nixos_configurations) VALUES($1,$2)")
        .bind(f.commit)
        .bind(names(&["cached-decoy"]))
        .execute(&pool)
        .await
        .unwrap();
    let mut reversed = discovered.clone();
    reversed.reverse();
    inventory(&pool, &f, &reversed, &reversed).await;
    let (_, matrix) = fetch_matrix(&pool, f.commit).await.unwrap();
    let actual = matrix.inventory.as_ref().unwrap();
    assert_eq!(actual.execution_mode, "native");
    assert_eq!(actual.configurations, discovered);
    assert_eq!(actual.selected_configurations, discovered);
    assert_eq!(matrix.rows.len(), 41);
    for original in &f.plan.rows {
        let row = matrix
            .rows
            .iter()
            .find(|row| row.system_id == original.system_id)
            .unwrap();
        assert_eq!(row.configuration_name, original.configuration_name);
        assert_eq!(row.resolution, EvalMatrixResolution::Resolved);
    }
    let unmanaged: Vec<_> = matrix
        .rows
        .iter()
        .filter(|row| row.system_id.is_none())
        .collect();
    assert_eq!(unmanaged.len(), 29);
    assert_eq!(
        unmanaged
            .iter()
            .map(|row| row.configuration_name.clone())
            .collect::<Vec<_>>(),
        discovered[12..]
    );
    for row in unmanaged {
        assert_eq!(row.system_name, row.configuration_name);
        assert_eq!(row.resolution, EvalMatrixResolution::Unavailable);
        assert!(row.policies.is_empty() && row.assigned_bundles.is_empty());
        assert!(row.evaluation_digest.is_none());
        assert_unknown(row, matrix.columns.len());
    }
    let summary = matrix.configuration_summary.unwrap();
    assert_eq!(
        (
            summary.total,
            summary.pending,
            summary.passed,
            summary.excluded
        ),
        (41, 41, 0, 0)
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn inventory_absent_and_unselected_registered_rows_keep_unknown_evidence(pool: PgPool) {
    let f = fixture(&pool, &names(&["selected", "excluded", "absent"])).await;
    inventory(
        &pool,
        &f,
        &names(&["selected", "excluded", "unmanaged"]),
        &names(&["selected", "unmanaged"]),
    )
    .await;
    for name in ["selected", "excluded", "absent", "unmanaged"] {
        native(&pool, &f, name, 7, true).await;
    }
    let (_, matrix) = fetch_matrix(&pool, f.commit).await.unwrap();
    assert_eq!(matrix.rows.len(), 4);
    for name in ["excluded", "absent"] {
        let row = matrix
            .rows
            .iter()
            .find(|row| row.configuration_name == name)
            .unwrap();
        assert!(row.system_id.is_some());
        assert_eq!(row.resolution, EvalMatrixResolution::Resolved);
        assert!(!row.policies.is_empty());
        assert_unknown(row, matrix.columns.len());
    }
    let unmanaged = matrix
        .rows
        .iter()
        .find(|row| row.configuration_name == "unmanaged")
        .unwrap();
    assert_eq!(unmanaged.system_id, None);
    assert_eq!(unmanaged.resolution, EvalMatrixResolution::Unavailable);
    assert_unknown(unmanaged, matrix.columns.len());
    let summary = matrix.configuration_summary.unwrap();
    assert_eq!(
        (
            summary.total,
            summary.passed,
            summary.excluded,
            summary.pending
        ),
        (4, 2, 1, 0)
    );
    assert_eq!(summary.unavailable, 1);
    let queue = crate::queries::commits::list_eval_queue(&pool, &Default::default())
        .await
        .unwrap();
    let header = queue
        .rows
        .iter()
        .find(|row| row.commit_id == f.commit)
        .unwrap();
    assert_eq!(
        header.systems,
        names(&["absent", "excluded", "selected", "unmanaged"])
    );
    assert_eq!(header.system_count, summary.total as i64);
    assert_eq!(header.policy_pending_count, summary.pending as i64);
    assert_eq!(header.policy_unavailable_count, summary.unavailable as i64);
    assert_eq!(header.unselected_count, summary.excluded as i64);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn inventory_shared_hosts_count_one_matching_global_failure_without_fake_policies(
    pool: PgPool,
) {
    for marker in [7, 6] {
        let f = fixture(&pool, &names(&["shared", "shared"])).await;
        inventory(&pool, &f, &names(&["shared"]), &names(&["shared"])).await;
        native(&pool, &f, "shared", marker, false).await;
        let (flat, matrix) = fetch_matrix(&pool, f.commit).await.unwrap();
        assert_eq!(flat.len(), 1);
        assert_eq!(matrix.rows.len(), 2);
        assert_ne!(matrix.rows[0].system_id, matrix.rows[1].system_id);
        assert_eq!(
            matrix
                .columns
                .iter()
                .map(|c| c.policy_version_id)
                .collect::<Vec<_>>(),
            f.plan
                .columns
                .iter()
                .map(|c| c.policy_version_id)
                .collect::<Vec<_>>()
        );
        assert_eq!(matrix.bundles.len(), f.plan.bundles.len());
        for (row, captured) in matrix.rows.iter().zip(&f.plan.rows) {
            assert_eq!(row.policies.len(), captured.policies.len());
            assert_eq!(
                row.policies[0].policy_version_id,
                captured.policies[0].policy_version_id
            );
        }
        let summary = matrix.configuration_summary.as_ref().unwrap();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.failed, usize::from(marker == 7));
        assert_eq!(summary.pending, usize::from(marker != 7));
        assert_eq!(
            matrix.configuration_failures.len(),
            usize::from(marker == 7)
        );
        if marker == 7 {
            assert_eq!(
                matrix.configuration_failures[0].configuration_name,
                "shared"
            );
            assert_eq!(
                matrix.configuration_failures[0].detail,
                "saved global failure for shared"
            );
        } else {
            for row in &matrix.rows {
                // Stale execution cannot supply a result. The immutable plan
                // still proves that another host's policy was not assigned.
                for (cell, column) in row.cells.iter().zip(&matrix.columns) {
                    let applicable = row
                        .policies
                        .iter()
                        .any(|binding| binding.policy_version_id == column.policy_version_id);
                    assert_eq!(
                        *cell,
                        if applicable {
                            EvalMatrixCell::Unknown
                        } else {
                            EvalMatrixCell::NotAssigned
                        }
                    );
                }
                assert!(row.details.iter().all(Option::is_none));
            }
        }
        finish(&pool, &f).await;
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn inventory_history_ignores_cache_mutation_and_queued_retry_has_no_inventory(pool: PgPool) {
    let f = fixture(&pool, &names(&["shared", "shared"])).await;
    let discovered = names(&["shared", "unmanaged-exact"]);
    inventory(&pool, &f, &discovered, &discovered).await;
    native(&pool, &f, "shared", 7, true).await;
    finish(&pool, &f).await;
    let (_, before) = fetch_matrix_for_user_at_attempt(&pool, f.commit, None, f.attempt)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.configuration_summary.as_ref().unwrap().passed, 1);
    assert_eq!(before.configuration_summary.as_ref().unwrap().pending, 0);
    assert_eq!(
        before.configuration_summary.as_ref().unwrap().unavailable,
        1
    );
    sqlx::query("INSERT INTO commit_artifacts_cache(commit_id,nixos_configurations) VALUES($1,$2)")
        .bind(f.commit)
        .bind(names(&["cache-original"]))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE commit_artifacts_cache SET nixos_configurations=$2 WHERE commit_id=$1")
        .bind(f.commit)
        .bind(names(&["cache-replacement", "cache-extra"]))
        .execute(&pool)
        .await
        .unwrap();
    let (_, after) = fetch_matrix_for_user_at_attempt(&pool, f.commit, None, f.attempt)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&before).unwrap(),
        serde_json::to_value(&after).unwrap()
    );
    sqlx::query(
        "INSERT INTO evaluation_attempts(commit_id,attempt_number,status) VALUES($1,8,'queued')",
    )
    .bind(f.commit)
    .execute(&pool)
    .await
    .unwrap();
    let (flat, retry) = fetch_matrix(&pool, f.commit).await.unwrap();
    assert_eq!(retry.attempt_number, 8);
    assert_ne!(retry.evaluation_attempt_id, Some(f.attempt));
    assert_eq!(retry.attempt_status.as_deref(), Some("queued"));
    assert!(retry.inventory.is_none() && retry.configuration_summary.is_none());
    assert!(retry.configuration_failures.is_empty() && retry.rows.is_empty());
    assert!(
        flat.iter().all(|row| {
            row.policy_requirements_met.is_none() && row.policy_results == json!({})
        })
    );
    let (_, pinned) = fetch_matrix_for_user_at_attempt(&pool, f.commit, None, f.attempt)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&before).unwrap(),
        serde_json::to_value(&pinned).unwrap()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn inventory_scoped_reads_hide_unmanaged_hidden_columns_and_global_failure_details(
    pool: PgPool,
) {
    for shared in [false, true] {
        let hidden_config = if shared {
            "visible-config"
        } else {
            "hidden-only-config"
        };
        let f = fixture(&pool, &names(&["visible-config", hidden_config])).await;
        let discovered = names(&["visible-config", hidden_config, "unmanaged-secret"]);
        inventory(&pool, &f, &discovered, &discovered).await;
        native(&pool, &f, "visible-config", 7, !shared).await;
        if !shared {
            native(&pool, &f, hidden_config, 7, false).await;
        }
        for pin in [None, Some(f.attempt)] {
            let (flat, matrix) = fetch_matrix_selected(&pool, f.commit, Some(f.users[0]), pin)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(matrix.rows.len(), 1);
            assert_eq!(matrix.rows[0].system_id, f.plan.rows[0].system_id);
            assert_eq!(matrix.columns.len(), 1);
            assert_eq!(
                matrix.columns[0].policy_version_id,
                f.plan.columns[0].policy_version_id
            );
            assert_eq!(matrix.bundles.len(), 1);
            assert_eq!(
                matrix.inventory.as_ref().unwrap().configurations,
                names(&["visible-config"])
            );
            assert_eq!(
                matrix.inventory.as_ref().unwrap().selected_configurations,
                names(&["visible-config"])
            );
            assert_eq!(matrix.configuration_failures.len(), usize::from(shared));
            if shared {
                // The intrinsic agent gate is this visible configuration's
                // own fact. Other hosts' requirement outcomes and text are not.
                assert_eq!(
                    matrix.configuration_failures[0].kind,
                    EvalConfigurationFailureKind::CfAgent
                );
                assert!(matrix.configuration_failures[0].detail.is_empty());
            }
            let summary = matrix.configuration_summary.as_ref().unwrap();
            assert_eq!(
                (summary.total, summary.failed, summary.pending),
                (1, usize::from(shared), 0)
            );
            assert_eq!(summary.passed, usize::from(!shared));
            let wire = format!("{} {flat:?}", serde_json::to_string(&matrix).unwrap());
            for secret in ["hidden-only", "unmanaged-secret", "saved global failure"] {
                assert!(!wire.contains(secret), "scoped projection leaked {secret}");
            }
            assert_eq!(flat.len(), 1);
            assert_eq!(
                flat[0].policy_results["assigned"]
                    .as_object()
                    .unwrap()
                    .len(),
                1
            );
        }
        // A later registration must not confirm guessed historical unmanaged
        // names. Historical name access requires the original captured UUID.
        sqlx::query("INSERT INTO systems(hostname,public_key,derivation,environment_id,flake_id,system_configuration_name)
            SELECT $2,'test-key','test-drv',environment_id,flake_id,'unmanaged-secret'
            FROM systems WHERE id=$1")
            .bind(f.plan.rows[0].system_id.unwrap())
            .bind(format!("late-registration-{}",f.commit))
            .execute(&pool).await.unwrap();
        for pin in [None, Some(f.attempt)] {
            let (_, matrix) = fetch_matrix_selected(&pool, f.commit, Some(f.users[0]), pin)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                matrix.inventory.as_ref().unwrap().configurations,
                names(&["visible-config"])
            );
            assert!(
                !serde_json::to_string(&matrix)
                    .unwrap()
                    .contains("unmanaged-secret")
            );
        }
        let diagnostic = "hidden-only configuration leaked through batch Nix stderr";
        sqlx::query(
            "UPDATE derivations SET status_id=6,error_message=$2,
            policy_results=policy_results || jsonb_build_object('evaluation_failure',true,
                'evaluation_failure_error',$2::text)
            WHERE commit_id=$1 AND derivation_name='visible-config'",
        )
        .bind(f.commit)
        .bind(diagnostic)
        .execute(&pool)
        .await
        .unwrap();
        for pin in [None, Some(f.attempt)] {
            let (flat, matrix) = fetch_matrix_selected(&pool, f.commit, Some(f.users[0]), pin)
                .await
                .unwrap()
                .unwrap();
            assert!(
                !format!("{flat:?} {}", serde_json::to_string(&matrix).unwrap())
                    .contains(diagnostic)
            );
        }
        let (admin, _) = fetch_matrix(&pool, f.commit).await.unwrap();
        assert!(
            admin
                .iter()
                .any(|row| row.error_message.as_deref() == Some(diagnostic))
        );
        finish(&pool, &f).await;
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn inventory_missing_discovery_is_unavailable_despite_passing_native_and_cache(pool: PgPool) {
    let f = fixture(&pool, &names(&["shared", "shared"])).await;
    native(&pool, &f, "shared", 7, true).await;
    sqlx::query("INSERT INTO commit_artifacts_cache(commit_id,nixos_configurations) VALUES($1,$2)")
        .bind(f.commit)
        .bind(names(&["shared", "cached-only"]))
        .execute(&pool)
        .await
        .unwrap();
    let (_, matrix) = fetch_matrix(&pool, f.commit).await.unwrap();
    assert!(matrix.inventory.is_none());
    assert!(matrix.configuration_summary.is_none());
    assert!(matrix.configuration_failures.is_empty());
    assert_eq!(matrix.rows.len(), 2);
    for row in &matrix.rows {
        assert_unknown(row, matrix.columns.len());
    }
    // Successful empty discovery is explicitly available, but cannot turn
    // captured hosts outside that discovery into passing control results.
    inventory(&pool, &f, &[], &[]).await;
    let (_, empty) = fetch_matrix(&pool, f.commit).await.unwrap();
    assert!(empty.inventory.unwrap().configurations.is_empty());
    let summary = empty.configuration_summary.unwrap();
    assert_eq!(
        (
            summary.total,
            summary.passed,
            summary.failed,
            summary.pending
        ),
        (1, 0, 0, 0)
    );
    assert_eq!(summary.unavailable, 1);
    for row in &empty.rows {
        assert_unknown(row, empty.columns.len());
    }
}

#[test]
fn inventory_mask_preserves_provenance_but_clears_unselected_and_missing_evidence() {
    let original = tests::plan();
    for selected in [None, Some(Vec::new()), Some(names(&["shared"]))] {
        let mut plan = original.clone();
        plan.inventory = selected
            .clone()
            .map(|selected_configurations| EvalAttemptInventory {
                execution_mode: "native".into(),
                configurations: names(&["no-result", "shared"]),
                selected_configurations,
            });
        for row in &mut plan.rows {
            row.cells = vec![EvalMatrixCell::Pass];
            row.details = vec![Some("saved policy detail".into())];
        }
        inventory_projection::mask_outside_selection(&mut plan);
        for (row, captured) in plan.rows.iter().zip(&original.rows) {
            if selected
                .as_ref()
                .is_some_and(|names| names.contains(&row.configuration_name))
            {
                assert_eq!(row.cells, vec![EvalMatrixCell::Pass]);
                assert_eq!(row.details, vec![Some("saved policy detail".into())]);
            } else {
                assert_unknown(row, plan.columns.len());
            }
            assert_eq!(row.system_id, captured.system_id);
            assert_eq!(row.resolution, captured.resolution);
            assert_eq!(row.evaluation_digest, captured.evaluation_digest);
            assert_eq!(
                serde_json::to_value(&row.policies).unwrap(),
                serde_json::to_value(&captured.policies).unwrap()
            );
            assert_eq!(
                serde_json::to_value(&row.assigned_bundles).unwrap(),
                serde_json::to_value(&captured.assigned_bundles).unwrap()
            );
        }
    }
}
