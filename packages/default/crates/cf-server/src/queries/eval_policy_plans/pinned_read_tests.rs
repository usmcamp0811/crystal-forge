//! Exact-attempt selection and transient navigation context regressions.

use super::*;
use serde_json::json;

async fn capture(
    pool: &PgPool,
    commit: i32,
    attempt: Uuid,
    number: i32,
    mode: crate::queries::evaluation_inventory::InventoryExecutionMode,
) -> EvalPolicyMatrixSnapshot {
    let mut plan = tests::plan();
    plan.evaluation_attempt_id = Some(attempt);
    plan.attempt_number = number;
    let mut tx = pool.begin().await.unwrap();
    insert_plan(&mut tx, commit, &plan).await.unwrap();
    tx.commit().await.unwrap();
    tests::inventory(pool, commit, number, &["no-result", "shared"], mode).await;
    plan
}

async fn pinned(
    pool: &PgPool,
    commit: i32,
    attempt: Uuid,
) -> (Vec<EvalPolicySystemRow>, EvalPolicyMatrixSnapshot) {
    fetch_matrix_for_user_at_attempt(pool, commit, None, attempt)
        .await
        .unwrap()
        .unwrap()
}

async fn retry(pool: &PgPool, commit: i32, number: i32) -> Uuid {
    sqlx::query(
        "UPDATE evaluation_attempts SET status='complete', completed_at=NOW() WHERE commit_id=$1",
    )
    .bind(commit)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE commits SET evaluation_status='pending' WHERE id=$1")
        .bind(commit)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query_scalar("INSERT INTO evaluation_attempts(commit_id,attempt_number,status) VALUES($1,$2,'queued') RETURNING id")
        .bind(commit).bind(number + 1).fetch_one(pool).await.unwrap()
}

async fn finish(pool: &PgPool, commit: i32) {
    sqlx::query("WITH finished AS (UPDATE evaluation_attempts SET status='complete', completed_at=NOW() WHERE commit_id=$1) UPDATE commits SET evaluation_status='complete' WHERE id=$1")
        .bind(commit).execute(pool).await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn pin_belongs_to_commit_and_never_falls_back_to_new_retry(pool: PgPool) {
    let (commit, id, number) = tests::commit(&pool).await;
    let original = capture(
        &pool,
        commit,
        id,
        number,
        crate::queries::evaluation_inventory::InventoryExecutionMode::Native,
    )
    .await;
    finish(&pool, commit).await;
    let (foreign, foreign_id, _) = tests::commit(&pool).await;
    finish(&pool, foreign).await;
    for (commit, attempt) in [
        (commit, foreign_id),
        (foreign, id),
        (commit, Uuid::new_v4()),
        (i32::MAX, id),
    ] {
        assert!(
            fetch_matrix_for_user_at_attempt(&pool, commit, None, attempt)
                .await
                .unwrap()
                .is_none()
        );
    }
    let next_id = retry(&pool, commit, number).await;
    let old = pinned(&pool, commit, id).await.1;
    assert_eq!(old.evaluation_attempt_id, Some(id));
    assert_eq!(old.attempt_number, number);
    assert_eq!(old.provenance, EvalMatrixProvenance::Captured);
    assert_eq!(old.attempt_status.as_deref(), Some("complete"));
    assert!(old.started_at.is_some());
    assert!(old.completed_at.is_some());
    let sha: String = sqlx::query_scalar("SELECT git_commit_hash FROM commits WHERE id=$1")
        .bind(commit)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old.commit_sha.as_deref(), Some(sha.as_str()));
    let latest = fetch_matrix(&pool, commit).await.unwrap().1;
    let next = pinned(&pool, commit, next_id).await.1;
    assert_eq!(
        serde_json::to_value(&latest).unwrap(),
        serde_json::to_value(&next).unwrap()
    );
    assert_eq!(next.evaluation_attempt_id, Some(next_id));
    assert_eq!(next.provenance, EvalMatrixProvenance::LegacyUnavailable);
    assert_eq!(next.attempt_status.as_deref(), Some("queued"));
    assert!(next.started_at.is_none() && next.completed_at.is_none());
    assert!(matches!(
        crate::queries::commits::mark_commit_evaluation_started(&pool, commit).await.unwrap(),
        crate::queries::commits::EvalStartOutcome::Started { attempt } if attempt == number + 1
    ));
    sqlx::query("UPDATE commits SET evaluation_status='cancelling', cancellation_requested=TRUE WHERE id=$1")
        .bind(commit).execute(&pool).await.unwrap();
    assert_eq!(
        pinned(&pool, commit, next_id)
            .await
            .1
            .attempt_status
            .as_deref(),
        Some("cancelling")
    );
    assert_eq!(
        pinned(&pool, commit, id).await.1.attempt_status.as_deref(),
        Some("complete")
    );
    let stored: Value = sqlx::query_scalar(
        "SELECT payload FROM evaluation_policy_plans WHERE evaluation_attempt_id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, serde_json::to_value(original).unwrap());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn historical_mock_values_survive_retry_while_stale_native_is_unknown(pool: PgPool) {
    let (commit, id, number) = tests::commit(&pool).await;
    let original = capture(
        &pool,
        commit,
        id,
        number,
        crate::queries::evaluation_inventory::InventoryExecutionMode::Mock,
    )
    .await;
    let policy = original.columns[0].policy_version_id;
    sqlx::query("INSERT INTO evaluation_mock_outcomes(evaluation_attempt_id,commit_id,attempt_number,configuration_name,policy_results,cf_agent_enabled,policy_requirements_met) VALUES($1,$2,$3,'shared',$4,TRUE,FALSE)")
        .bind(id).bind(commit).bind(number)
        .bind(json!({"evaluation_attempt": number, "execution_mode":"mock", "assigned":{
            policy.to_string():{"passed":false,"blocking":true,"details":"[MOCK] immutable old failure"}
        }})).execute(&pool).await.unwrap();
    let next_id = retry(&pool, commit, number).await;
    let mut next_plan = original.clone();
    next_plan.evaluation_attempt_id = Some(next_id);
    next_plan.attempt_number = number + 1;
    let mut tx = pool.begin().await.unwrap();
    insert_plan(&mut tx, commit, &next_plan).await.unwrap();
    tx.commit().await.unwrap();
    // This historical-read fixture represents discovery on a queued retry.
    // Insert exact immutable inventory before its outcomes; the active writer
    // correctly rejects queued attempts and must not be used to manufacture it.
    sqlx::query("INSERT INTO evaluation_attempt_inventory(evaluation_attempt_id,commit_id,attempt_number,configuration_names,selected_configuration_names,execution_mode) VALUES($1,$2,$3,ARRAY['no-result','shared'],ARRAY['no-result','shared'],'native')")
        .bind(next_id).bind(commit).bind(number + 1)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO derivations(commit_id,derivation_type,derivation_name,status_id,policy_results) VALUES($1,'nixos','shared',5,$2)")
        .bind(commit).bind(json!({"evaluation_attempt":number+1,"assigned":{
            policy.to_string():{"passed":true,"details":"new native pass"}
        }})).execute(&pool).await.unwrap();
    let old = pinned(&pool, commit, id).await.1;
    assert!(old.mock_evidence);
    assert_eq!(old.rows[0].cells, vec![EvalMatrixCell::Fail]);
    assert_eq!(
        old.rows[0].details,
        vec![Some("[MOCK] immutable old failure".into())]
    );
    assert_eq!(
        pinned(&pool, commit, next_id).await.1.rows[0].cells,
        vec![EvalMatrixCell::Pass]
    );

    // The second planned configuration has only overwritten mutable native
    // evidence. Its old attempt must not borrow the retry's pass or diagnostic.
    sqlx::query("INSERT INTO derivations(commit_id,derivation_type,derivation_name,status_id,policy_results,error_message) VALUES($1,'nixos','no-result',6,$2,'retry error')")
        .bind(commit).bind(json!({"evaluation_attempt":number+1,"evaluation_failure":true,
            "evaluation_failure_error":"retry failure"})).execute(&pool).await.unwrap();
    let (flat, old) = pinned(&pool, commit, id).await;
    assert_eq!(old.rows[1].cells, vec![EvalMatrixCell::Unknown]);
    assert_eq!(old.rows[1].details, vec![None]);
    let stale = flat.iter().find(|r| r.system_name == "no-result").unwrap();
    assert!(stale.error_message.is_none());
    assert_eq!(stale.policy_results, json!({}));
    let (flat, next) = pinned(&pool, commit, next_id).await;
    let failure = flat.iter().find(|r| r.system_name == "no-result").unwrap();
    assert_eq!(failure.eval_status, "eval_failed");
    assert_eq!(failure.error_message.as_deref(), Some("retry failure"));
    assert_eq!(next.rows[1].cells, vec![EvalMatrixCell::Unknown]);
}

#[test]
fn earlier_context_defaults_are_unknown() {
    let mut value = serde_json::to_value(tests::plan()).unwrap();
    for key in ["commit_sha", "attempt_status", "started_at", "completed_at"] {
        value.as_object_mut().unwrap().remove(key);
    }
    let plan: EvalPolicyMatrixSnapshot = serde_json::from_value(value).unwrap();
    assert!(plan.commit_sha.is_none() && plan.attempt_status.is_none());
    assert!(plan.started_at.is_none() && plan.completed_at.is_none());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn pinned_native_failure_is_preserved_until_its_marker_is_overwritten(pool: PgPool) {
    let (commit, id, number) = tests::commit(&pool).await;
    capture(
        &pool,
        commit,
        id,
        number,
        crate::queries::evaluation_inventory::InventoryExecutionMode::Native,
    )
    .await;
    sqlx::query("INSERT INTO derivations(commit_id,derivation_type,derivation_name,status_id,policy_results,error_message) VALUES($1,'nixos','shared',10,$2,'unrelated build diagnostic')")
        .bind(commit).bind(json!({"evaluation_attempt":number,"evaluation_failure":true,
            "evaluation_failure_error":"exact old Nix failure"})).execute(&pool).await.unwrap();
    retry(&pool, commit, number).await;
    let (flat, matrix) = pinned(&pool, commit, id).await;
    assert_eq!(flat[0].eval_status, "eval_failed");
    assert_eq!(
        flat[0].error_message.as_deref(),
        Some("exact old Nix failure")
    );
    assert_eq!(flat[0].policy_requirements_met, Some(false));
    assert_eq!(matrix.rows[0].cells, vec![EvalMatrixCell::Unknown]);
    for evidence in [
        json!({}),
        json!({"evaluation_attempt":number+1,"evaluation_failure":true,
        "evaluation_failure_error":"new private error"}),
    ] {
        sqlx::query("UPDATE derivations SET policy_results=$2 WHERE commit_id=$1")
            .bind(commit)
            .bind(evidence)
            .execute(&pool)
            .await
            .unwrap();
        let (flat, old) = pinned(&pool, commit, id).await;
        assert_ne!(flat[0].eval_status, "eval_failed");
        assert!(flat[0].error_message.is_none());
        assert!(flat[0].policy_requirements_met.is_none());
        assert_eq!(flat[0].policy_results, json!({}));
        assert_eq!(old.rows[0].cells, vec![EvalMatrixCell::Unknown]);
    }
}
