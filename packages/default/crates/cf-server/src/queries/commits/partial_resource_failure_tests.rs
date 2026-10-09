//! Exercises partial resource terminalization only in disposable SQLx databases.

use super::{
    EvalFailureOutcome, EvalStartOutcome, cancel_commit_evaluation,
    finalize_partial_evaluation_resource_failure, mark_commit_evaluation_started,
};
use crate::models::deployment_policies::{
    AssignedPolicy, CompositePolicyConfig, CreateDeploymentPolicyRequest, DeploymentPolicy,
    EvaluationTerminalOutcome, PolicyCheckResult,
};
use crate::models::evaluate_with_policies::{
    ConfirmedSystemFailure, EvaluationPlan, SuccessfulSystemResult, SystemPersistenceOutcome,
    UnacknowledgedCompletion, activate_evaluated_system_build, persist_evaluated_system,
};
use crate::models::evaluation_snapshots::{EvaluatedOption, SafeOptionValue};
use crate::queries::deployment_policies::create_deployment_policy;
use crate::services::composite_enforcement::initialize_eval_passed_attempt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

struct Fixture {
    commit_id: i32,
    attempt: i32,
    plan: EvaluationPlan,
    assigned: AssignedPolicy,
    policy_lineage_id: Uuid,
}

async fn fixture(pool: &PgPool) -> Fixture {
    fixture_with_confirmed_failure(pool, false).await
}

async fn fixture_with_confirmed_failure(pool: &PgPool, confirm_beta: bool) -> Fixture {
    fixture_at_completion_phase(pool, confirm_beta, CompletionPhase::Acknowledged).await
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompletionPhase {
    Acknowledged,
    CommittedWithoutAcknowledgement,
    NeverCommitted,
}

async fn fixture_at_completion_phase(
    pool: &PgPool,
    confirm_beta: bool,
    phase: CompletionPhase,
) -> Fixture {
    let flake_id: i32 = sqlx::query_scalar(
        "INSERT INTO flakes (name, repo_url) VALUES ('partial-resource', 'https://example.invalid/partial.git') RETURNING id",
    ).fetch_one(pool).await.unwrap();
    let commit_id: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, NOW()) RETURNING id",
    ).bind(flake_id).bind("a".repeat(40)).fetch_one(pool).await.unwrap();
    sqlx::query(
        "INSERT INTO commit_artifacts_cache (commit_id, nixos_configurations) VALUES ($1, ARRAY['alpha','beta','remaining','unselected'])",
    ).bind(commit_id).execute(pool).await.unwrap();
    let config: CompositePolicyConfig = serde_json::from_value(json!({
        "schema_version": 1, "mode": "all",
        "rules": [{"id": Uuid::new_v4(), "kind": "eval_passed", "config": {}}],
    }))
    .unwrap();
    let policy = create_deployment_policy(
        pool,
        &CreateDeploymentPolicyRequest {
            name: "partial-resource-evidence".into(),
            policy_type: "composite".into(),
            config: serde_json::to_value(&config).unwrap(),
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy.id)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET trust_state = 'trusted' WHERE id = $1")
        .bind(version_id)
        .execute(pool)
        .await
        .unwrap();
    let assigned = AssignedPolicy {
        policy_id: version_id,
        policy_name: "partial-resource-evidence".into(),
        enforcement_mode: Default::default(),
        policy: DeploymentPolicy::Composite { config },
    };
    let mut policies = BTreeMap::new();
    for name in ["alpha", "beta", "remaining", "unselected"] {
        let system_id: Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, is_active, public_key, derivation, reachability, flake_id, system_configuration_name) VALUES ($1, TRUE, $2, $2, 'direct', $3, $1) RETURNING id",
        ).bind(name).bind(format!("fixture-{name}")).bind(flake_id).fetch_one(pool).await.unwrap();
        sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
            .bind(system_id)
            .bind(policy.id)
            .execute(pool)
            .await
            .unwrap();
        policies.insert(name.into(), vec![assigned.clone()]);
    }
    let attempt = match mark_commit_evaluation_started(pool, commit_id)
        .await
        .unwrap()
    {
        EvalStartOutcome::Started { attempt } => attempt,
        _ => panic!("fixture attempt must start"),
    };
    initialize_eval_passed_attempt(pool, commit_id, attempt, &policies)
        .await
        .unwrap();
    // The normal retry policy would schedule children. Resource terminalization
    // must override it without changing any normal failure-class behavior.
    sqlx::query("UPDATE automatic_retry_policy SET max_evaluation_retries = 5, transient_only = FALSE WHERE id = 1")
        .execute(pool).await.unwrap();
    let mut successes = Vec::new();
    let mut checks = Vec::new();
    let mut confirmed_failures = Vec::new();
    for (name, enabled) in [("alpha", true), ("beta", false)] {
        if name == "beta" && phase != CompletionPhase::Acknowledged {
            continue;
        }
        let result = SuccessfulSystemResult {
            system_name: name.into(),
            derivation_target: format!(
                "git+https://example.invalid/partial.git#nixosConfigurations.{name}"
            ),
            drv_path: format!("/nix/store/{}-{name}.drv", "a".repeat(32)),
            expected_store_path: Some(format!("/nix/store/{}-{name}", "b".repeat(32))),
            cf_agent_enabled: Some(enabled),
            build_eligible: true,
        };
        if name == "beta" && confirm_beta {
            let diagnostic = "assertion failed: required Nix configuration option is invalid";
            checks.push(PolicyCheckResult::for_evaluation_terminal(
                name.into(),
                std::slice::from_ref(&assigned),
                EvaluationTerminalOutcome::ConfirmedFailure,
                diagnostic,
            ));
            confirmed_failures.push(ConfirmedSystemFailure {
                system_name: name.into(),
                derivation_target: result.derivation_target,
                error: diagnostic.into(),
            });
            continue;
        }
        let check = PolicyCheckResult::from_assigned(
            name.into(),
            &json!({"cfAgentEnabled": enabled}),
            std::slice::from_ref(&assigned),
        )
        .unwrap();
        if phase != CompletionPhase::NeverCommitted {
            let persisted = persist_evaluated_system(
                pool,
                commit_id,
                attempt,
                &result,
                &check,
                std::slice::from_ref(&assigned),
            )
            .await
            .unwrap();
            if phase == CompletionPhase::Acknowledged
                && let SystemPersistenceOutcome::NeedsBuildPreparation { derivation_id, .. } =
                    persisted
            {
                // Exercise real activation without invoking Nix or creating roots.
                activate_evaluated_system_build(pool, commit_id, attempt, derivation_id)
                    .await
                    .unwrap();
            }
        }
        successes.push(result);
        checks.push(check);
    }
    Fixture {
        commit_id,
        attempt,
        assigned,
        policy_lineage_id: policy.id,
        plan: EvaluationPlan {
            results: vec![],
            policy_checks: checks,
            successful_systems: successes,
            confirmed_failures,
            evaluation_snapshots: HashMap::from([(
                "alpha".into(),
                vec![EvaluatedOption {
                    path: "services.openssh.enable".into(),
                    declared_type: Some("boolean".into()),
                    metadata_error: None,
                    value: SafeOptionValue::Scalar(json!(true)),
                    definitions: vec![],
                    overridden: Some(false),
                }],
            )]),
            snapshot_capture_failures: if confirm_beta || phase != CompletionPhase::Acknowledged {
                HashMap::new()
            } else {
                HashMap::from([("beta".into(), "Snapshot not captured".into())])
            },
            flake_output_snapshot: None,
            had_system_eval_errors: true,
            force_build_job_insert_failure: false,
        },
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn partial_resource_failure_retains_rejected_completion_with_preserved_build_history(
    pool: PgPool,
) {
    use crate::models::deployment_policies::{
        UpdateDeploymentPolicyRequest, composite_rule_result_key,
    };
    use crate::queries::commits::{mark_commit_evaluation_complete, reset_commit_evaluation};
    use crate::queries::deployment_policies::update_deployment_policy;
    use crate::queries::derivations::{
        EvaluationStatus, queue_derivations_for_build, update_derivation_status,
    };

    let mut fixture = fixture(&pool).await;
    // First attempt: both configurations pass and have admitted job history.
    // Activation is the database phase only; no Nix/root probe is invoked.
    fixture.plan.successful_systems[1].cf_agent_enabled = Some(true);
    let beta_check = PolicyCheckResult::from_assigned(
        "beta".into(),
        &json!({"cfAgentEnabled": true}),
        std::slice::from_ref(&fixture.assigned),
    )
    .unwrap();
    let beta_derivation = match persist_evaluated_system(
        &pool,
        fixture.commit_id,
        fixture.attempt,
        &fixture.plan.successful_systems[1],
        &beta_check,
        std::slice::from_ref(&fixture.assigned),
    )
    .await
    .unwrap()
    {
        SystemPersistenceOutcome::NeedsBuildPreparation { derivation_id, .. } => derivation_id,
        other => panic!("beta must gain initial preparation: {other:?}"),
    };
    activate_evaluated_system_build(&pool, fixture.commit_id, fixture.attempt, beta_derivation)
        .await
        .unwrap();
    queue_derivations_for_build(&pool, fixture.commit_id)
        .await
        .unwrap();
    update_derivation_status(
        &pool,
        beta_derivation,
        EvaluationStatus::BuildInProgress,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE build_jobs SET status = 'building', started_at = NOW() WHERE derivation_id = $1",
    )
    .bind(beta_derivation)
    .execute(&pool)
    .await
    .unwrap();
    mark_commit_evaluation_complete(&pool, fixture.commit_id, fixture.attempt)
        .await
        .unwrap();
    reset_commit_evaluation(&pool, fixture.commit_id)
        .await
        .unwrap();
    fixture.attempt = match mark_commit_evaluation_started(&pool, fixture.commit_id)
        .await
        .unwrap()
    {
        EvalStartOutcome::Started { attempt } => attempt,
        _ => panic!("manual child attempt must start"),
    };
    assert_eq!(fixture.attempt, 2);

    // Preserve real passing completions in the second attempt before policy
    // replacement. Historical state is neither normalized nor newly admitted.
    let mut passing_checks = Vec::new();
    for result in &fixture.plan.successful_systems {
        let check = PolicyCheckResult::from_assigned(
            result.system_name.clone(),
            &json!({"cfAgentEnabled": true}),
            std::slice::from_ref(&fixture.assigned),
        )
        .unwrap();
        assert!(matches!(
            persist_evaluated_system(
                &pool,
                fixture.commit_id,
                fixture.attempt,
                result,
                &check,
                std::slice::from_ref(&fixture.assigned)
            )
            .await
            .unwrap(),
            SystemPersistenceOutcome::ExistingBuildJob { .. }
        ));
        passing_checks.push(check);
    }
    for preparation in [Some("queued"), None] {
        for (status_id, job_status) in [(8, "building"), (10, "success"), (12, "failed")] {
            let mut tx = lock_history_fixture(&pool, &fixture).await;
            sqlx::query("UPDATE derivations SET build_preparation_state = $2 WHERE commit_id = $1")
                .bind(fixture.commit_id)
                .bind(preparation)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("UPDATE derivations SET status_id = $2 WHERE id = $1")
                .bind(beta_derivation)
                .bind(status_id)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("UPDATE build_jobs SET status = $2 WHERE derivation_id = $1")
                .bind(beta_derivation)
                .bind(job_status)
                .execute(&mut *tx)
                .await
                .unwrap();
            for (result, check) in fixture.plan.successful_systems.iter().zip(&passing_checks) {
                assert!(
                    super::completion_committed_tx(
                        &mut tx,
                        fixture.commit_id,
                        fixture.attempt,
                        result,
                        check
                    )
                    .await
                    .unwrap(),
                    "passing history supports queued and legacy NULL"
                );
            }
            // Resource-terminal admission can recognize only this existing
            // history idempotently; the orphan selector still excludes NULL.
            sqlx::query("UPDATE commits SET evaluation_status = 'failed' WHERE id = $1")
                .bind(fixture.commit_id)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("UPDATE evaluation_attempts SET status = 'failed', failure_class = 'transient',
                error_message = 'resource_pressure: fixture bounded recovery stopped', completed_at = NOW()
                WHERE commit_id = $1 AND attempt_number = $2")
                .bind(fixture.commit_id).bind(fixture.attempt).execute(&mut *tx).await.unwrap();
            assert!(
                crate::queries::build_jobs::resource_terminal_build_preparation_allowed_in_tx(
                    &mut tx,
                    fixture.commit_id,
                    fixture.attempt,
                    beta_derivation
                )
                .await
                .unwrap()
            );
            tx.rollback().await.unwrap();
        }
    }
    // Model pre-0195 history through the actual early-return persistence path.
    sqlx::query("UPDATE derivations SET build_preparation_state = NULL WHERE id = $1")
        .bind(beta_derivation)
        .execute(&pool)
        .await
        .unwrap();

    let mut config = match &fixture.assigned.policy {
        DeploymentPolicy::Composite { config } => serde_json::to_value(config).unwrap(),
        _ => unreachable!("fixture uses a composite"),
    };
    let strict_rule = Uuid::new_v4();
    config["rules"].as_array_mut().unwrap().push(json!({
        "id": strict_rule, "kind": "custom_eval",
        "config": {"expression": "false", "message": "fixture strict policy rejection"},
    }));
    update_deployment_policy(
        &pool,
        &fixture.policy_lineage_id,
        &UpdateDeploymentPolicyRequest {
            config: Some(config.clone()),
            ..Default::default()
        },
        None,
    )
    .await
    .unwrap()
    .unwrap();
    let version: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(fixture.policy_lineage_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET trust_state = 'trusted' WHERE id = $1")
        .bind(version)
        .execute(&pool)
        .await
        .unwrap();
    fixture.assigned.policy_id = version;
    fixture.assigned.policy = DeploymentPolicy::Composite {
        config: serde_json::from_value(config).unwrap(),
    };
    let policies = ["alpha", "beta", "remaining", "unselected"]
        .into_iter()
        .map(|name| (name.into(), vec![fixture.assigned.clone()]))
        .collect();
    initialize_eval_passed_attempt(&pool, fixture.commit_id, fixture.attempt, &policies)
        .await
        .unwrap();
    fixture.plan.policy_checks.clear();
    for result in &fixture.plan.successful_systems {
        let mut metadata = json!({"cfAgentEnabled": true});
        metadata[composite_rule_result_key(&version, &strict_rule)] =
            json!({"success": true, "value": false});
        let check = PolicyCheckResult::from_assigned(
            result.system_name.clone(),
            &metadata,
            std::slice::from_ref(&fixture.assigned),
        )
        .unwrap();
        assert!(!check.meets_requirements);
        assert!(matches!(
            persist_evaluated_system(
                &pool,
                fixture.commit_id,
                fixture.attempt,
                result,
                &check,
                std::slice::from_ref(&fixture.assigned)
            )
            .await
            .unwrap(),
            SystemPersistenceOutcome::RecordedWithoutBuild { .. }
        ));
        fixture.plan.policy_checks.push(check);
    }
    fixture
        .plan
        .evaluation_snapshots
        .insert("beta".into(), vec![]);
    fixture.plan.snapshot_capture_failures.clear();
    let before = durable_state(&pool, fixture.commit_id).await;
    let jobs: Vec<(String, String)> = sqlx::query_as(
        "SELECT d.derivation_name, j.status FROM build_jobs j JOIN derivations d ON d.id = j.derivation_id WHERE d.commit_id = $1 ORDER BY d.derivation_name",
    ).bind(fixture.commit_id).fetch_all(&pool).await.unwrap();
    assert_eq!(
        jobs,
        vec![
            ("alpha".into(), "cancelled".into()),
            ("beta".into(), "building".into())
        ]
    );
    for row in before["derivations"].as_array().unwrap() {
        if row["derivation_name"] == "beta" {
            assert!(row["build_preparation_state"].is_null());
        } else {
            assert_eq!(row["build_preparation_state"], "queued");
        }
        assert_eq!(row["policy_requirements_met"], false);
        assert_eq!(row["policy_results"]["evaluation_attempt"], 2);
    }
    for preparation in [Some("queued"), None] {
        for (status_id, job_status) in [(8, "building"), (10, "success"), (12, "failed")] {
            let mut tx = lock_history_fixture(&pool, &fixture).await;
            sqlx::query("UPDATE derivations SET build_preparation_state = $2 WHERE commit_id = $1")
                .bind(fixture.commit_id)
                .bind(preparation)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("UPDATE derivations SET status_id = $2 WHERE id = $1")
                .bind(beta_derivation)
                .bind(status_id)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("UPDATE build_jobs SET status = $2 WHERE derivation_id = $1")
                .bind(beta_derivation)
                .bind(job_status)
                .execute(&mut *tx)
                .await
                .unwrap();
            for (result, check) in fixture
                .plan
                .successful_systems
                .iter()
                .zip(&fixture.plan.policy_checks)
            {
                assert!(
                    super::completion_committed_tx(
                        &mut tx,
                        fixture.commit_id,
                        fixture.attempt,
                        result,
                        check
                    )
                    .await
                    .unwrap(),
                    "rejected history supports queued and legacy NULL"
                );
            }
            tx.rollback().await.unwrap();
        }
    }
    let mut no_history = lock_history_fixture(&pool, &fixture).await;
    let mut unprepared = fixture.plan.successful_systems[0].clone();
    unprepared.system_name = "remaining".into();
    unprepared.derivation_target =
        "git+https://example.invalid/partial.git#nixosConfigurations.remaining".into();
    unprepared.drv_path = format!("/nix/store/{}-remaining.drv", "c".repeat(32));
    let mut metadata = json!({"cfAgentEnabled": true});
    metadata[composite_rule_result_key(&version, &strict_rule)] =
        json!({"success": true, "value": false});
    let unprepared_check = PolicyCheckResult::from_assigned(
        "remaining".into(),
        &metadata,
        std::slice::from_ref(&fixture.assigned),
    )
    .unwrap();
    let mut evidence = crate::models::deployment_policies::policy_results_json(
        &unprepared_check,
        std::slice::from_ref(&fixture.assigned),
    );
    evidence["evaluation_attempt"] = json!(fixture.attempt);
    let no_history_id: i32 = sqlx::query_scalar(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_target,
             derivation_path, expected_store_path, cf_agent_enabled, policy_requirements_met,
             policy_results, status_id, build_preparation_state)
         VALUES ($1, 'nixos', $2, $3, $4, $5, TRUE, FALSE, $6, 5, NULL) RETURNING id",
    )
    .bind(fixture.commit_id)
    .bind(&unprepared.system_name)
    .bind(&unprepared.derivation_target)
    .bind(&unprepared.drv_path)
    .bind(&unprepared.expected_store_path)
    .bind(evidence)
    .fetch_one(&mut *no_history)
    .await
    .unwrap();
    assert!(
        super::completion_committed_tx(
            &mut no_history,
            fixture.commit_id,
            fixture.attempt,
            &unprepared,
            &unprepared_check
        )
        .await
        .is_err(),
        "NULL without compatible history cannot prove completion"
    );
    assert!(
        crate::queries::build_jobs::create_build_job_for_derivation_tx(
            &mut no_history,
            no_history_id
        )
        .await
        .unwrap()
        .is_none(),
        "rejected NULL preparation cannot create a job"
    );
    no_history.rollback().await.unwrap();
    let mut invalid_preparation = pool.begin().await.unwrap();
    crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut invalid_preparation)
        .await
        .unwrap();
    super::lock_eval_queue_order_tx(&mut invalid_preparation)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM commits WHERE id = $1 FOR UPDATE")
        .bind(fixture.commit_id)
        .execute(&mut *invalid_preparation)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM evaluation_attempts WHERE commit_id = $1 AND attempt_number = $2 FOR UPDATE")
        .bind(fixture.commit_id).bind(fixture.attempt).execute(&mut *invalid_preparation).await.unwrap();
    sqlx::query("UPDATE derivations SET build_preparation_state = 'pending' WHERE commit_id = $1 AND derivation_name = 'alpha'")
        .bind(fixture.commit_id).execute(&mut *invalid_preparation).await.unwrap();
    assert!(
        super::completion_committed_tx(
            &mut invalid_preparation,
            fixture.commit_id,
            fixture.attempt,
            &fixture.plan.successful_systems[0],
            &fixture.plan.policy_checks[0]
        )
        .await
        .is_err(),
        "rejected completion cannot acquire new pending preparation eligibility"
    );
    invalid_preparation.rollback().await.unwrap();
    let (outcome, completed, remaining, _) = finalize_partial_evaluation_resource_failure(
        &pool,
        fixture.commit_id,
        fixture.attempt,
        &fixture.plan,
        &["remaining".into()],
        &[],
        "resource capacity exhausted",
    )
    .await
    .unwrap();
    assert_eq!(outcome, EvalFailureOutcome::PermanentlyFailed);
    assert_eq!((completed, remaining), (2, 1));
    let after = durable_state(&pool, fixture.commit_id).await;
    for key in ["derivations", "jobs", "assessments", "rules"] {
        assert_eq!(
            before[key], after[key],
            "preserved rejected {key} cannot change during resource failure"
        );
    }
    assert_eq!(after["jobs"].as_array().unwrap().len(), 2);
    assert_eq!(
        after["attempts"].as_array().unwrap().len(),
        2,
        "no automatic child after the manual retry"
    );
    assert_eq!(after["commit"]["evaluation_status"], "failed");
    assert_eq!(after["summary"]["systems_failed_policy_strict"], 2);
    assert_eq!(after["summary"]["systems_with_eval_error"], 1);
    let rule_outcomes: Vec<String> = sqlx::query_scalar(
        "SELECT r.outcome FROM composite_policy_rule_results r JOIN composite_policy_assessments a ON a.id = r.assessment_id WHERE a.policy_version_id = $1 AND r.kind = 'custom_eval' ORDER BY a.system_id",
    ).bind(version).fetch_all(&pool).await.unwrap();
    assert_eq!(rule_outcomes, vec!["fail", "fail"]);
    let snapshot_lifecycles: Vec<(String, String)> = sqlx::query_as(
        "SELECT s.configuration_name, s.lifecycle FROM evaluation_snapshot_selections selected JOIN evaluation_snapshots s ON s.id = selected.current_snapshot_id WHERE selected.commit_id = $1 ORDER BY s.configuration_name",
    ).bind(fixture.commit_id).fetch_all(&pool).await.unwrap();
    assert_eq!(
        snapshot_lifecycles,
        vec![
            ("alpha".into(), "available".into()),
            ("beta".into(), "available".into()),
            ("remaining".into(), "failed".into())
        ]
    );
    assert!(matches!(
        activate_evaluated_system_build(&pool, fixture.commit_id, fixture.attempt, beta_derivation)
            .await
            .unwrap(),
        crate::models::evaluate_with_policies::SystemBuildActivationOutcome::Superseded
    ));
}

async fn lock_history_fixture(
    pool: &PgPool,
    fixture: &Fixture,
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.begin().await.unwrap();
    crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx)
        .await
        .unwrap();
    super::lock_eval_queue_order_tx(&mut tx).await.unwrap();
    sqlx::query("SELECT id FROM commits WHERE id = $1 FOR UPDATE")
        .bind(fixture.commit_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM evaluation_attempts WHERE commit_id = $1 AND attempt_number = $2 FOR UPDATE")
        .bind(fixture.commit_id).bind(fixture.attempt).execute(&mut *tx).await.unwrap();
    tx
}

fn take_unacknowledged_alpha(plan: &mut EvaluationPlan) -> UnacknowledgedCompletion {
    UnacknowledgedCompletion {
        result: plan.successful_systems.remove(0),
        policy_check: plan.policy_checks.remove(0),
        snapshot: plan.evaluation_snapshots.remove("alpha"),
        snapshot_capture_failure: plan.snapshot_capture_failures.remove("alpha"),
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn partial_resource_failure_reconciles_committed_but_unacknowledged_completion(pool: PgPool) {
    let mut fixture = fixture_at_completion_phase(
        &pool,
        false,
        CompletionPhase::CommittedWithoutAcknowledgement,
    )
    .await;
    let candidate = take_unacknowledged_alpha(&mut fixture.plan);
    assert!(fixture.plan.successful_systems.is_empty());
    let before = durable_state(&pool, fixture.commit_id).await;
    assert!(
        super::reconcile_unacknowledged_completion(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &candidate,
        )
        .await
        .unwrap()
    );
    assert_eq!(
        before,
        durable_state(&pool, fixture.commit_id).await,
        "catch-up is read-only"
    );
    assert!(
        super::reconcile_unacknowledged_completion(
            &pool,
            fixture.commit_id,
            fixture.attempt + 1,
            &candidate,
        )
        .await
        .is_err(),
        "changed attempt is not an uncommitted candidate"
    );
    let mut contradictory = candidate.clone();
    contradictory.result.expected_store_path = Some(format!("/nix/store/{}-wrong", "c".repeat(32)));
    assert!(
        super::reconcile_unacknowledged_completion(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &contradictory,
        )
        .await
        .is_err(),
        "a current COMMIT marker cannot bless a different output"
    );
    let mut contradictory_policy = candidate.clone();
    contradictory_policy
        .policy_check
        .assigned_results
        .values_mut()
        .next()
        .unwrap()
        .blocking = true;
    assert!(
        super::reconcile_unacknowledged_completion(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &contradictory_policy,
        )
        .await
        .is_err(),
        "a current COMMIT marker cannot bless different policy evidence"
    );
    assert_eq!(before, durable_state(&pool, fixture.commit_id).await);
    let checkpoint = json!({
        "completed_count": 0, "remaining_count": 2,
        "remaining_systems": ["alpha", "remaining"],
        "resource_failures": [
            {"configuration": "alpha", "diagnostic": "COMMIT reply was interrupted"},
            {"configuration": "remaining", "diagnostic": "resource capacity exhausted"},
        ],
    })
    .to_string();
    let (outcome, completed, remaining, diagnostic) = finalize_partial_evaluation_resource_failure(
        &pool,
        fixture.commit_id,
        fixture.attempt,
        &fixture.plan,
        &["alpha".into(), "remaining".into()],
        std::slice::from_ref(&candidate),
        &checkpoint,
    )
    .await
    .unwrap();
    assert_eq!(outcome, EvalFailureOutcome::PermanentlyFailed);
    assert_eq!((completed, remaining), (1, 1));
    let diagnostic: Value =
        serde_json::from_str(diagnostic.strip_prefix("resource_pressure: ").unwrap()).unwrap();
    assert_eq!(diagnostic["completed_count"], 1);
    assert_eq!(diagnostic["remaining_systems"], json!(["remaining"]));
    assert_eq!(diagnostic["resource_failures"].as_array().unwrap().len(), 1);
    assert_eq!(
        diagnostic["resource_failures"][0]["configuration"],
        "remaining"
    );
    let after = durable_state(&pool, fixture.commit_id).await;
    for key in ["derivations", "jobs", "assessments", "rules"] {
        assert_eq!(
            before[key], after[key],
            "unacknowledged completed {key} is preserved"
        );
    }
    assert_eq!(
        after["derivations"][0]["build_preparation_state"],
        "pending"
    );
    assert!(
        after["jobs"].is_null(),
        "finalization cannot queue before GC rooting"
    );
    assert_eq!(after["commit"]["evaluation_status"], "failed");
    assert_eq!(after["attempts"].as_array().unwrap().len(), 1);
    let snapshots: Vec<(String, String, i32)> = sqlx::query_as(
        "SELECT s.configuration_name, s.lifecycle, s.option_count FROM evaluation_snapshot_selections selected JOIN evaluation_snapshots s ON s.id = selected.current_snapshot_id WHERE selected.commit_id = $1 ORDER BY s.configuration_name",
    ).bind(fixture.commit_id).fetch_all(&pool).await.unwrap();
    assert_eq!(
        snapshots,
        vec![
            ("alpha".into(), "available".into(), 1),
            ("remaining".into(), "failed".into(), 0)
        ]
    );
    let outcomes: Vec<(String, String)> = sqlx::query_as(
        "SELECT r.configuration_name, r.outcome FROM composite_eval_attempt_rule_results r JOIN evaluation_attempts a ON a.id = r.evaluation_attempt_id WHERE a.commit_id = $1 ORDER BY configuration_name",
    ).bind(fixture.commit_id).fetch_all(&pool).await.unwrap();
    assert_eq!(
        outcomes,
        vec![
            ("alpha".into(), "pass".into()),
            ("beta".into(), "not_checked".into()),
            ("remaining".into(), "error".into()),
            ("unselected".into(), "not_checked".into())
        ]
    );
    assert!(
        super::reconcile_unacknowledged_completion(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &candidate,
        )
        .await
        .is_err(),
        "terminalized attempts cannot authorize catch-up"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn partial_resource_failure_ignores_never_committed_completion_candidate(pool: PgPool) {
    let mut fixture =
        fixture_at_completion_phase(&pool, false, CompletionPhase::NeverCommitted).await;
    let candidate = take_unacknowledged_alpha(&mut fixture.plan);
    let before = durable_state(&pool, fixture.commit_id).await;
    assert!(
        !super::reconcile_unacknowledged_completion(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &candidate,
        )
        .await
        .unwrap()
    );
    assert_eq!(before, durable_state(&pool, fixture.commit_id).await);
    let (outcome, completed, remaining, _) = finalize_partial_evaluation_resource_failure(
        &pool,
        fixture.commit_id,
        fixture.attempt,
        &fixture.plan,
        &["alpha".into(), "remaining".into()],
        std::slice::from_ref(&candidate),
        "resource capacity exhausted",
    )
    .await
    .unwrap();
    assert_eq!(outcome, EvalFailureOutcome::PermanentlyFailed);
    assert_eq!((completed, remaining), (0, 2));
    let after = durable_state(&pool, fixture.commit_id).await;
    assert!(after["derivations"].is_null());
    assert!(after["jobs"].is_null());
    assert_eq!(after["summary"]["systems_passed_policy"], 0);
    let snapshots: Vec<(String, String)> = sqlx::query_as(
        "SELECT s.configuration_name, s.lifecycle FROM evaluation_snapshot_selections selected JOIN evaluation_snapshots s ON s.id = selected.current_snapshot_id WHERE selected.commit_id = $1 ORDER BY s.configuration_name",
    ).bind(fixture.commit_id).fetch_all(&pool).await.unwrap();
    assert_eq!(
        snapshots,
        vec![
            ("alpha".into(), "failed".into()),
            ("remaining".into(), "failed".into())
        ]
    );
}

#[test]
fn partial_failure_cohort_keeps_confirmed_and_resource_systems_disjoint() {
    let plan = EvaluationPlan {
        results: vec![],
        policy_checks: vec![],
        successful_systems: vec![],
        confirmed_failures: vec![ConfirmedSystemFailure {
            system_name: "beta".into(),
            derivation_target: "selected-beta".into(),
            error: "Nix assertion".into(),
        }],
        evaluation_snapshots: HashMap::new(),
        snapshot_capture_failures: HashMap::new(),
        flake_output_snapshot: None,
        had_system_eval_errors: true,
        force_build_job_insert_failure: false,
    };
    assert_eq!(
        super::partial_evaluation_cohort(&["alpha".into()], &plan, &["remaining".into()]).unwrap(),
        vec!["alpha", "beta", "remaining"]
    );
    for invalid in [
        vec!["beta".into()],
        vec!["alpha".into()],
        vec!["remaining".into(), "remaining".into()],
        vec!["".into()],
    ] {
        assert!(super::partial_evaluation_cohort(&["alpha".into()], &plan, &invalid).is_err());
    }
}

#[test]
fn evaluation_failure_resource_marker_is_server_owned() {
    use crate::models::retry_policy::RetryFailureClass;
    let diagnostic = "resource_pressure: evaluator-provided diagnostic";
    for class in [
        RetryFailureClass::Transient,
        RetryFailureClass::Deterministic,
        RetryFailureClass::Authorization,
        RetryFailureClass::Cancelled,
        RetryFailureClass::Unknown,
    ] {
        assert!(
            !super::evaluation_failure_diagnostic(diagnostic, class)
                .starts_with("resource_pressure: ")
        );
    }
    assert!(
        super::evaluation_failure_diagnostic(diagnostic, RetryFailureClass::ResourceFailure)
            .starts_with("resource_pressure: ")
    );
    assert_eq!(
        super::evaluation_failure_diagnostic("ordinary failure", RetryFailureClass::Transient),
        "ordinary failure"
    );
}

#[test]
fn partial_failure_diagnostic_uses_proven_counts_not_checkpoint_guesses() {
    let checkpoint = json!({
        "completed_count": 0, "remaining_count": 2, "remaining_systems": ["alpha", "remaining"],
        "resource_failures": [
            {"configuration": "alpha", "diagnostic": "reply interrupted"},
            {"configuration": "remaining", "diagnostic": "persistence acknowledgement unavailable; infrastructure cause unknown"},
        ],
    })
    .to_string();
    let diagnostic =
        super::verified_partial_failure_diagnostic(&checkpoint, 1, &["remaining".into()], 0);
    let persisted: Value =
        serde_json::from_str(diagnostic.strip_prefix("resource_pressure: ").unwrap()).unwrap();
    assert_eq!(persisted["completed_count"], 1);
    assert_eq!(persisted["remaining_count"], 1);
    assert_eq!(persisted["remaining_systems"], json!(["remaining"]));
    assert_eq!(persisted["resource_failures"].as_array().unwrap().len(), 1);
    assert_eq!(
        persisted["resource_failures"][0]["configuration"],
        "remaining"
    );
    assert_eq!(
        persisted["action"],
        "Check evaluator resource diagnostics and database acknowledgement; resolve the recorded cause before manual retry."
    );
    assert_eq!(persisted["failure_code"], "resource_pressure");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn partial_resource_failure_mixed_confirmed_and_resource_cohorts_terminalize(pool: PgPool) {
    let fixture = fixture_with_confirmed_failure(&pool, true).await;
    let before = durable_state(&pool, fixture.commit_id).await;
    assert_eq!(
        finalize_partial_evaluation_resource_failure(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &fixture.plan,
            &["remaining".into()],
            &[],
            "remaining: resource capacity exhausted",
        )
        .await
        .unwrap()
        .0,
        EvalFailureOutcome::PermanentlyFailed
    );
    let after = durable_state(&pool, fixture.commit_id).await;
    for key in ["derivations", "jobs", "assessments", "rules"] {
        assert_eq!(
            before[key], after[key],
            "completed alpha {key} is preserved"
        );
    }
    assert_eq!(after["commit"]["evaluation_status"], "failed");
    assert_eq!(after["attempts"].as_array().unwrap().len(), 1);
    assert_eq!(after["attempts"][0]["status"], "failed");
    assert_eq!(after["jobs"].as_array().unwrap().len(), 1);
    let outcomes: Vec<(String, String, Value)> = sqlx::query_as(
        "SELECT r.configuration_name, r.outcome, r.evidence FROM composite_eval_attempt_rule_results r JOIN evaluation_attempts a ON a.id = r.evaluation_attempt_id WHERE a.commit_id = $1 ORDER BY configuration_name",
    ).bind(fixture.commit_id).fetch_all(&pool).await.unwrap();
    assert_eq!(outcomes[0].1, "pass");
    assert_eq!(outcomes[1].1, "fail");
    assert_eq!(outcomes[1].2["terminal_outcome"], "confirmed_failure");
    assert!(outcomes[1].2.get("failure_code").is_none());
    assert_eq!(outcomes[2].1, "error");
    assert_eq!(outcomes[2].2["failure_code"], "resource_pressure");
    assert_eq!(outcomes[3].1, "not_checked");
    let snapshots: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT s.configuration_name, s.lifecycle, s.error FROM evaluation_snapshot_selections selected JOIN evaluation_snapshots s ON s.id = selected.current_snapshot_id WHERE selected.commit_id = $1 ORDER BY s.configuration_name",
    ).bind(fixture.commit_id).fetch_all(&pool).await.unwrap();
    assert_eq!(snapshots.len(), 3);
    assert_eq!(
        (snapshots[0].0.as_str(), snapshots[0].1.as_str()),
        ("alpha", "available")
    );
    assert_eq!(
        snapshots[1],
        (
            "beta".into(),
            "failed".into(),
            Some(fixture.plan.confirmed_failures[0].error.clone())
        )
    );
    assert!(
        snapshots[2]
            .2
            .as_deref()
            .unwrap()
            .starts_with("resource_pressure: ")
    );
    assert_eq!(after["summary"]["total_systems"], 3);
    assert_eq!(after["summary"]["systems_passed_policy"], 1);
    assert_eq!(after["summary"]["systems_with_eval_error"], 2);
}

async fn durable_state(pool: &PgPool, commit_id: i32) -> Value {
    sqlx::query_scalar(
        r#"SELECT jsonb_build_object(
            'commit', (SELECT to_jsonb(c) FROM commits c WHERE id = $1),
            'attempts', (SELECT jsonb_agg(to_jsonb(a) ORDER BY attempt_number) FROM evaluation_attempts a WHERE commit_id = $1),
            'derivations', (SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM derivations d WHERE commit_id = $1),
            'jobs', (SELECT jsonb_agg(to_jsonb(j) ORDER BY j.id) FROM build_jobs j JOIN derivations d ON d.id = j.derivation_id WHERE d.commit_id = $1),
            'attempt_rules', (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.configuration_name, r.policy_version_id, r.rule_id) FROM composite_eval_attempt_rule_results r JOIN evaluation_attempts a ON a.id = r.evaluation_attempt_id WHERE a.commit_id = $1),
            'assessments', (SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM composite_policy_assessments p JOIN derivations d ON d.id = p.derivation_id WHERE d.commit_id = $1),
            'rules', (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.assessment_id, r.rule_id) FROM composite_policy_rule_results r JOIN composite_policy_assessments p ON p.id = r.assessment_id JOIN derivations d ON d.id = p.derivation_id WHERE d.commit_id = $1),
            'snapshots', (SELECT jsonb_agg(to_jsonb(s) ORDER BY id) FROM evaluation_snapshots s WHERE commit_id = $1),
            'summary', (SELECT to_jsonb(m) FROM commit_metadata_cache m WHERE commit_id = $1)
        )"#,
    ).bind(commit_id).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn partial_resource_failure_retains_completed_evidence_snapshots_and_jobs(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let before = durable_state(&pool, fixture.commit_id).await;
    assert_eq!(
        finalize_partial_evaluation_resource_failure(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &fixture.plan,
            &["remaining".into()],
            &[],
            "remaining: evaluator resource capacity exhausted",
        )
        .await
        .unwrap()
        .0,
        EvalFailureOutcome::PermanentlyFailed
    );
    let after = durable_state(&pool, fixture.commit_id).await;
    for key in ["derivations", "jobs", "assessments", "rules"] {
        assert_eq!(
            before[key], after[key],
            "completed {key} must remain byte-identical"
        );
    }
    assert_eq!(after["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(after["commit"]["evaluation_status"], "failed");
    let attempts = after["attempts"].as_array().unwrap();
    assert_eq!(
        attempts.len(),
        1,
        "resource exhaustion cannot queue a whole-flake retry"
    );
    assert_eq!(attempts[0]["status"], "failed");
    assert_eq!(attempts[0]["failure_class"], "transient");
    assert!(
        attempts[0]["error_message"]
            .as_str()
            .unwrap()
            .starts_with("resource_pressure: ")
    );
    let rule_states: Vec<(String, String, Value)> = sqlx::query_as(
        "SELECT r.configuration_name, r.outcome, r.evidence FROM composite_eval_attempt_rule_results r JOIN evaluation_attempts a ON a.id = r.evaluation_attempt_id WHERE a.commit_id = $1 ORDER BY configuration_name",
    ).bind(fixture.commit_id).fetch_all(&pool).await.unwrap();
    assert_eq!(rule_states[0].1, "pass");
    assert_eq!(
        rule_states[1].1, "pass",
        "completed policy-rejected evaluation still completed"
    );
    assert_eq!(
        rule_states[2].1, "error",
        "unfinished resource work is not policy Fail"
    );
    assert_eq!(rule_states[2].2["failure_code"], "resource_pressure");
    assert_eq!(
        rule_states[3].1, "not_checked",
        "unselected configuration is untouched"
    );
    let snapshots: Vec<(String, String, i32)> = sqlx::query_as(
        "SELECT s.configuration_name, s.lifecycle, s.option_count FROM evaluation_snapshot_selections selected JOIN evaluation_snapshots s ON s.id = selected.current_snapshot_id WHERE selected.commit_id = $1 ORDER BY s.configuration_name",
    ).bind(fixture.commit_id).fetch_all(&pool).await.unwrap();
    assert_eq!(
        snapshots,
        vec![
            ("alpha".into(), "available".into(), 1),
            ("beta".into(), "unavailable".into(), 0),
            ("remaining".into(), "failed".into(), 0)
        ]
    );
    assert_eq!(after["summary"]["systems_passed_policy"], 1);
    assert_eq!(after["summary"]["systems_failed_policy_strict"], 1);
    assert_eq!(after["summary"]["systems_with_eval_error"], 1);
    assert_eq!(after["summary"]["total_systems"], 3);
    assert_eq!(
        finalize_partial_evaluation_resource_failure(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &fixture.plan,
            &["remaining".into()],
            &[],
            "duplicate",
        )
        .await
        .unwrap()
        .0,
        EvalFailureOutcome::SupersededOrCancelled
    );
    assert_eq!(after, durable_state(&pool, fixture.commit_id).await);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn partial_resource_failure_rejects_unverified_retention_atomically(pool: PgPool) {
    let mut fixture = fixture(&pool).await;
    let before = durable_state(&pool, fixture.commit_id).await;
    fixture.plan.successful_systems[0].drv_path =
        format!("/nix/store/{}-unverified.drv", "c".repeat(32));
    assert!(
        finalize_partial_evaluation_resource_failure(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &fixture.plan,
            &["remaining".into()],
            &[],
            "resource exhausted",
        )
        .await
        .is_err()
    );
    assert_eq!(before, durable_state(&pool, fixture.commit_id).await);
    fixture.plan.successful_systems[0].drv_path =
        format!("/nix/store/{}-alpha.drv", "a".repeat(32));
    // A matching path from another attempt is still not completion authority.
    sqlx::query("UPDATE derivations SET policy_results = jsonb_set(policy_results, '{evaluation_attempt}', to_jsonb(999)) WHERE commit_id = $1")
        .bind(fixture.commit_id).execute(&pool).await.unwrap();
    let before = durable_state(&pool, fixture.commit_id).await;
    assert!(
        finalize_partial_evaluation_resource_failure(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &fixture.plan,
            &["remaining".into()],
            &[],
            "resource exhausted",
        )
        .await
        .is_err()
    );
    assert_eq!(before, durable_state(&pool, fixture.commit_id).await);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn partial_resource_failure_respects_supersession_and_cancellation(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let candidate = UnacknowledgedCompletion {
        result: fixture.plan.successful_systems[0].clone(),
        policy_check: fixture.plan.policy_checks[0].clone(),
        snapshot: fixture.plan.evaluation_snapshots.get("alpha").cloned(),
        snapshot_capture_failure: None,
    };
    let before = durable_state(&pool, fixture.commit_id).await;
    assert_eq!(
        finalize_partial_evaluation_resource_failure(
            &pool,
            fixture.commit_id,
            fixture.attempt + 1,
            &fixture.plan,
            &["remaining".into()],
            &[],
            "stale resource report",
        )
        .await
        .unwrap()
        .0,
        EvalFailureOutcome::SupersededOrCancelled
    );
    assert_eq!(before, durable_state(&pool, fixture.commit_id).await);
    cancel_commit_evaluation(&pool, fixture.commit_id)
        .await
        .unwrap();
    let cancelled = durable_state(&pool, fixture.commit_id).await;
    assert!(
        super::reconcile_unacknowledged_completion(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &candidate,
        )
        .await
        .is_err(),
        "cancellation cannot become a false missing-commit result"
    );
    assert_eq!(cancelled, durable_state(&pool, fixture.commit_id).await);
    assert_eq!(
        finalize_partial_evaluation_resource_failure(
            &pool,
            fixture.commit_id,
            fixture.attempt,
            &fixture.plan,
            &["remaining".into()],
            &[],
            "cancelled resource report",
        )
        .await
        .unwrap()
        .0,
        EvalFailureOutcome::SupersededOrCancelled
    );
    assert_eq!(cancelled, durable_state(&pool, fixture.commit_id).await);
}
