//! Migration-backed restart cases for the existing Snowfall server regressions.
//!
//! Tests use canonical claims, the normal capturing loader, and production
//! outcome persistence. They require a disposable SQLx database, never a preview.

use super::*;
use crate::models::deployment_policies::PolicyCheckResult;
use crate::models::evaluate_with_policies::{SuccessfulSystemResult, persist_evaluated_system};
use crate::queries::evaluation_inventory::{InventoryExecutionMode, persist_inventory};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
struct Attempt {
    id: Uuid,
    parent_attempt_id: Option<Uuid>,
    root_attempt_id: Option<Uuid>,
    automatic_retry_source_id: Option<Uuid>,
    attempt_number: i32,
    automatic_retry_count: i32,
    status: String,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    available_at: chrono::DateTime<chrono::Utc>,
    error_message: Option<String>,
    failure_class: Option<String>,
}

async fn attempts(pool: &PgPool, commit: i32) -> Vec<Attempt> {
    sqlx::query_as("SELECT * FROM evaluation_attempts WHERE commit_id = $1 ORDER BY attempt_number")
        .bind(commit)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn fixture(pool: &PgPool) -> (i32, i32) {
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(format!("restart-{}", Uuid::new_v4()))
            .bind(format!("https://example.invalid/{}", Uuid::new_v4()))
            .fetch_one(pool)
            .await
            .unwrap();
    let commit = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
         VALUES ($1, $2, NOW()) RETURNING id",
    )
    .bind(flake)
    .bind("a".repeat(40))
    .fetch_one(pool)
    .await
    .unwrap();
    (flake, commit)
}

async fn claim(pool: &PgPool, commit: i32) -> i32 {
    match mark_commit_evaluation_started(pool, commit).await.unwrap() {
        EvalStartOutcome::Started { attempt } => attempt,
        other => panic!("canonical claim must succeed: {other:?}"),
    }
}

async fn retry_policy(pool: &PgPool, budget: i16, backoff: i32) {
    sqlx::query(
        "UPDATE automatic_retry_policy SET max_evaluation_retries = $1,
             backoff_seconds = $2, transient_only = TRUE WHERE id = 1",
    )
    .bind(budget)
    .bind(backoff)
    .execute(pool)
    .await
    .unwrap();
}

async fn failure(pool: &PgPool, commit: i32, number: i32) -> EvalFailureOutcome {
    mark_commit_evaluation_failed(
        pool,
        commit,
        "temporary source transport failure",
        number,
        RetryFailureClass::Transient,
    )
    .await
    .unwrap()
}

async fn policy(pool: &PgPool, name: &str) -> (Uuid, Uuid) {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies (name, policy_type, config, enabled)
         VALUES ($1, 'custom_check', $2, TRUE) RETURNING id",
    )
    .bind(name)
    .bind(json!({"expression":"true", "strict":true,
        "field_name": name.replace(' ', ""), "description": name}))
    .fetch_one(pool)
    .await
    .unwrap();
    let version: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE deployment_policy_versions SET publication_state = 'accepted',
             published_at = NOW(), trust_state = 'trusted', semantic_digest = 'restart-test'
         WHERE id = $1",
    )
    .bind(version)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE deployment_policies SET current_published_version_id = $2 WHERE id = $1")
        .bind(id)
        .bind(version)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (id, version)
}

async fn immutable_evidence(pool: &PgPool, owner: Uuid) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
             'plan', (SELECT to_jsonb(p) FROM evaluation_policy_plans p WHERE evaluation_attempt_id = $1),
             'inventory', (SELECT to_jsonb(i) FROM evaluation_attempt_inventory i WHERE evaluation_attempt_id = $1),
             'outcomes', (SELECT jsonb_agg(to_jsonb(n) ORDER BY configuration_name)
                  FROM evaluation_native_outcomes n WHERE evaluation_attempt_id = $1))",
    ).bind(owner).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn restart_recovery_fresh_loader_capture_preserves_partial_durable_evidence(pool: PgPool) {
    let (flake, commit) = fixture(&pool).await;
    let system: Uuid = sqlx::query_scalar(
        "INSERT INTO systems (hostname, flake_id, system_configuration_name, public_key, derivation)
         VALUES ('restart-alpha', $1, 'alpha', 'test-key', '/nix/store/test') RETURNING id",
    ).bind(flake).fetch_one(&pool).await.unwrap();
    let first = policy(&pool, "Before interruption").await;
    sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
        .bind(system)
        .bind(first.0)
        .execute(&pool)
        .await
        .unwrap();
    let number = claim(&pool, commit).await;
    // Requires pub(crate) visibility on the existing production loader. A
    // test-only resolver or handcrafted plan cannot establish this contract.
    let loaded =
        crate::server::load_and_capture_policies_for_eval(&pool, flake, Some((commit, number)))
            .await
            .unwrap();
    assert_eq!(loaded["alpha"][0].policy_id, first.1);
    persist_inventory(
        &pool,
        commit,
        number,
        &["alpha".into(), "beta".into()],
        &["alpha".into(), "beta".into()],
        InventoryExecutionMode::Native,
    )
    .await
    .unwrap();
    let check = PolicyCheckResult::from_assigned(
        "alpha".into(),
        &json!({"cfAgentEnabled": true, "Beforeinterruption": true}),
        &loaded["alpha"],
    )
    .unwrap();
    persist_evaluated_system(
        &pool,
        commit,
        number,
        &SuccessfulSystemResult {
            system_name: "alpha".into(),
            derivation_target: "git+https://example.invalid/restart#nixosConfigurations.alpha"
                .into(),
            drv_path: format!("/nix/store/{}-alpha.drv", "a".repeat(32)),
            expected_store_path: Some(format!("/nix/store/{}-alpha", "b".repeat(32))),
            cf_agent_enabled: Some(true),
            build_eligible: true,
        },
        &check,
        &loaded["alpha"],
    )
    .await
    .unwrap();
    let (_, source_matrix) = crate::queries::eval_policy_plans::fetch_matrix(&pool, commit)
        .await
        .unwrap();
    assert_eq!(
        source_matrix.rows[0].cells,
        vec![crate::api::eval_policy_matrix::EvalMatrixCell::Pass]
    );
    let before = attempts(&pool, commit).await;
    let source = &before[0];
    let evidence = immutable_evidence(&pool, source.id).await;
    assert!(
        !evidence["outcomes"].is_null(),
        "production outcome must be durable"
    );
    let commit_start: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT evaluation_started_at FROM commits WHERE id = $1")
            .bind(commit)
            .fetch_one(&pool)
            .await
            .unwrap();
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    let rows = attempts(&pool, commit).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status, "failed");
    assert_eq!(rows[0].failure_class.as_deref(), Some("unknown"));
    assert!(
        rows[0]
            .error_message
            .as_deref()
            .unwrap()
            .starts_with("Server interruption:")
    );
    assert_eq!(rows[0].started_at, source.started_at);
    assert!(rows[0].completed_at.is_some());
    assert_ne!(rows[1].id, source.id);
    assert_eq!(rows[1].parent_attempt_id, Some(source.id));
    assert_eq!(rows[1].root_attempt_id, source.root_attempt_id);
    assert_eq!(rows[1].attempt_number, number + 1);
    assert_eq!(rows[1].automatic_retry_count, 0);
    assert_eq!(rows[1].automatic_retry_source_id, None);
    assert_eq!(rows[1].started_at, None);
    assert_eq!(immutable_evidence(&pool, source.id).await, evidence);
    assert!(immutable_evidence(&pool, rows[1].id).await["plan"].is_null());
    let identity: (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT git_commit_hash, evaluation_started_at FROM commits WHERE id = $1")
            .bind(commit)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(identity, ("a".repeat(40), commit_start));

    let second = policy(&pool, "After interruption").await;
    sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
        .bind(system)
        .bind(second.0)
        .execute(&pool)
        .await
        .unwrap();
    let child = claim(&pool, commit).await;
    let loaded =
        crate::server::load_and_capture_policies_for_eval(&pool, flake, Some((commit, child)))
            .await
            .unwrap();
    assert_eq!(loaded["alpha"].len(), 2);
    assert!(
        loaded["alpha"]
            .iter()
            .any(|policy| policy.policy_id == second.1)
    );
    assert!(
        loaded["alpha"]
            .iter()
            .any(|policy| policy.policy_id == first.1)
    );
    persist_inventory(
        &pool,
        commit,
        child,
        &["alpha".into()],
        &["alpha".into()],
        InventoryExecutionMode::Native,
    )
    .await
    .unwrap();
    let (_, matrix) = crate::queries::eval_policy_plans::fetch_matrix(&pool, commit)
        .await
        .unwrap();
    assert_eq!(matrix.evaluation_attempt_id, Some(rows[1].id));
    assert_eq!(matrix.columns.len(), 2);
    assert!(
        matrix
            .columns
            .iter()
            .any(|column| column.policy_version_id == second.1)
    );
    assert!(
        matrix
            .columns
            .iter()
            .any(|column| column.policy_version_id == first.1)
    );
    assert!(
        matrix
            .rows
            .iter()
            .all(|row| row.details.iter().all(Option::is_none))
    );
    assert!(matrix.rows.iter().all(|row| {
        row.cells
            .iter()
            .all(|cell| *cell == crate::api::eval_policy_matrix::EvalMatrixCell::Unknown)
    }));
    assert!(immutable_evidence(&pool, rows[1].id).await["outcomes"].is_null());
    assert_eq!(immutable_evidence(&pool, source.id).await, evidence);
    // Stale completion/failure messages cannot terminalize the successor.
    assert_eq!(
        mark_commit_evaluation_complete(&pool, commit, number)
            .await
            .unwrap(),
        EvalCompleteOutcome::SupersededOrCancelled
    );
    assert_eq!(
        failure(&pool, commit, number).await,
        EvalFailureOutcome::SupersededOrCancelled
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn restart_recovery_preserves_zero_and_exhausted_failure_budgets(pool: PgPool) {
    let (_, commit) = fixture(&pool).await;
    retry_policy(&pool, 0, 0).await;
    let first = claim(&pool, commit).await;
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    assert_eq!(attempts(&pool, commit).await[1].automatic_retry_count, 0);
    let child = claim(&pool, commit).await;
    assert_eq!(child, first + 1);
    assert_eq!(
        failure(&pool, commit, child).await,
        EvalFailureOutcome::PermanentlyFailed
    );
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    assert_eq!(attempts(&pool, commit).await.len(), 2);

    // Max=1: 0 -> recovery0 -> actual failure1 -> recovery1 -> no retry
    // -> explicit manual0. A manual child preserves existing root semantics.
    let (_, commit) = fixture(&pool).await;
    retry_policy(&pool, 1, 0).await;
    claim(&pool, commit).await;
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    let recovered = claim(&pool, commit).await;
    assert_eq!(
        failure(&pool, commit, recovered).await,
        EvalFailureOutcome::RetryScheduled
    );
    let actual_retry = claim(&pool, commit).await;
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    let recovered_retry = claim(&pool, commit).await;
    assert_eq!(recovered_retry, actual_retry + 1);
    assert_eq!(
        failure(&pool, commit, recovered_retry).await,
        EvalFailureOutcome::PermanentlyFailed
    );
    reset_commit_evaluation(&pool, commit).await.unwrap();
    let rows = attempts(&pool, commit).await;
    assert_eq!(
        rows.iter()
            .map(|a| a.automatic_retry_count)
            .collect::<Vec<_>>(),
        vec![0, 0, 1, 1, 0]
    );
    assert_eq!(rows[2].automatic_retry_source_id, Some(rows[1].id));
    assert_eq!(rows[3].automatic_retry_source_id, None);
    for pair in rows.windows(2) {
        assert_eq!(pair[1].parent_attempt_id, Some(pair[0].id));
        assert_eq!(pair[1].root_attempt_id, rows[0].root_attempt_id);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn restart_recovery_repeated_concurrent_exact_source_and_later_child_interruption(
    pool: PgPool,
) {
    let (_, commit) = fixture(&pool).await;
    claim(&pool, commit).await;
    let sources = super::restart_recovery::interrupted_sources(&pool)
        .await
        .unwrap();
    // Both invocations observe the same source before either writer can commit.
    let recover = || async {
        let mut tx = pool.begin().await.unwrap();
        let result = super::restart_recovery::recover_sources_tx(&mut tx, &sources)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        result
    };
    let (a, b) = tokio::join!(recover(), recover());
    assert_eq!(a.0 + b.0, 1);
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    assert_eq!(attempts(&pool, commit).await.len(), 2);
    claim(&pool, commit).await;
    // A delayed repeat with the old UUID must not recover the now-active child.
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        super::restart_recovery::recover_sources_tx(&mut tx, &sources)
            .await
            .unwrap(),
        (0, 0)
    );
    tx.commit().await.unwrap();
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    let rows = attempts(&pool, commit).await;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2].parent_attempt_id, Some(rows[1].id));
    assert_eq!(rows[2].root_attempt_id, rows[0].root_attempt_id);
    assert_eq!(rows[2].automatic_retry_count, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn restart_recovery_insert_failure_rolls_back_source_and_commit(pool: PgPool) {
    let (_, commit) = fixture(&pool).await;
    claim(&pool, commit).await;
    let before: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object('commit', to_jsonb(c), 'attempts',
             (SELECT jsonb_agg(to_jsonb(a)) FROM evaluation_attempts a WHERE a.commit_id = c.id))
         FROM commits c WHERE id = $1",
    )
    .bind(commit)
    .fetch_one(&pool)
    .await
    .unwrap();
    // Force the actual child INSERT to fail after the source UPDATE. This DDL
    // exists only inside this disposable test database, never in migrations.
    sqlx::query("CREATE FUNCTION reject_recovery_child() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected child failure'; END $$")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_recovery_child BEFORE INSERT ON evaluation_attempts FOR EACH ROW EXECUTE FUNCTION reject_recovery_child()")
        .execute(&pool).await.unwrap();
    assert!(reset_stuck_commit_evaluations(&pool).await.is_err());
    let after: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object('commit', to_jsonb(c), 'attempts',
             (SELECT jsonb_agg(to_jsonb(a)) FROM evaluation_attempts a WHERE a.commit_id = c.id))
         FROM commits c WHERE id = $1",
    )
    .bind(commit)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, before);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn restart_recovery_cancellation_terminal_and_missing_owner_never_enqueue(pool: PgPool) {
    for cancellation_flag_only in [false, true] {
        let (_, commit) = fixture(&pool).await;
        claim(&pool, commit).await;
        if cancellation_flag_only {
            sqlx::query("UPDATE commits SET cancellation_requested = TRUE WHERE id = $1")
                .bind(commit)
                .execute(&pool)
                .await
                .unwrap();
        } else {
            assert_eq!(
                cancel_commit_evaluation(&pool, commit).await.unwrap(),
                CancelEvalOutcome::CancellingInProgress
            );
        }
        reset_stuck_commit_evaluations(&pool).await.unwrap();
        reset_stuck_commit_evaluations(&pool).await.unwrap();
        let rows = attempts(&pool, commit).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "cancelled");
        let status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "cancelled");
    }
    let (_, complete) = fixture(&pool).await;
    let number = claim(&pool, complete).await;
    mark_commit_evaluation_complete(&pool, complete, number)
        .await
        .unwrap();
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    assert_eq!(attempts(&pool, complete).await.len(), 1);
    assert_eq!(attempts(&pool, complete).await[0].status, "complete");
    let (_, missing) = fixture(&pool).await;
    claim(&pool, missing).await;
    sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE commit_id = $1")
        .bind(missing)
        .execute(&pool)
        .await
        .unwrap();
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    assert_eq!(attempts(&pool, missing).await.len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn restart_recovery_preserves_queued_children_and_delayed_backoff(pool: PgPool) {
    let (_, pending) = fixture(&pool).await;
    let (_, commit) = fixture(&pool).await;
    retry_policy(&pool, 1, 300).await;
    let number = claim(&pool, commit).await;
    assert_eq!(
        failure(&pool, commit, number).await,
        EvalFailureOutcome::RetryScheduled
    );
    let before = attempts(&pool, commit).await;
    let queued_before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(a) FROM evaluation_attempts a WHERE commit_id = $1 AND status = 'queued'",
    )
    .bind(commit)
    .fetch_one(&pool)
    .await
    .unwrap();
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    let after = attempts(&pool, commit).await;
    assert_eq!(after.len(), 2);
    assert_eq!(after[1].id, before[1].id);
    assert_eq!(after[1].available_at, before[1].available_at);
    let queued_after: Value = sqlx::query_scalar(
        "SELECT to_jsonb(a) FROM evaluation_attempts a WHERE commit_id = $1 AND status = 'queued'",
    )
    .bind(commit)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(queued_after, queued_before);
    assert_eq!(attempts(&pool, pending).await.len(), 1);
    assert_eq!(
        mark_commit_evaluation_started(&pool, commit).await.unwrap(),
        EvalStartOutcome::NoLongerPending
    );
    assert_eq!(after[1].automatic_retry_count, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn restart_recovery_ordinary_failure_retries_before_and_after_interruption(pool: PgPool) {
    let (_, commit) = fixture(&pool).await;
    retry_policy(&pool, 2, 0).await;
    let first = claim(&pool, commit).await;
    assert_eq!(
        failure(&pool, commit, first).await,
        EvalFailureOutcome::RetryScheduled
    );
    claim(&pool, commit).await;
    reset_stuck_commit_evaluations(&pool).await.unwrap();
    let recovery = claim(&pool, commit).await;
    assert_eq!(
        failure(&pool, commit, recovery).await,
        EvalFailureOutcome::RetryScheduled
    );
    let last = claim(&pool, commit).await;
    assert_eq!(
        failure(&pool, commit, last).await,
        EvalFailureOutcome::PermanentlyFailed
    );
    let rows = attempts(&pool, commit).await;
    assert_eq!(
        rows.iter()
            .map(|a| a.automatic_retry_count)
            .collect::<Vec<_>>(),
        vec![0, 1, 1, 2]
    );
    assert_eq!(rows[1].automatic_retry_source_id, Some(rows[0].id));
    assert_eq!(rows[2].automatic_retry_source_id, None);
    assert_eq!(rows[3].automatic_retry_source_id, Some(rows[2].id));
}
