use crystal_forge::queries::build_jobs::{
    BuildJobInsertOutcome, create_build_job_for_derivation_tx,
    fail_expired_dependency_plans_without_recovery, mark_job_failed,
};
use crystal_forge::queries::build_reservations::{claim_next_derivation, create_reservation};
use crystal_forge::queries::builders::{claim_next_job_atomic, requeue_orphaned_building_jobs};
use crystal_forge::queries::commits::{fetch_eval_dependency_breakdown, reset_commit_evaluation};
use crystal_forge::queries::derivations::{
    DependencyBuildPlanWriteOutcome, complete_dependency_build_plan, fail_dependency_build_plan,
    insert_derivation_with_target, mark_dependency_build_plan_calculating,
};
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crystal_forge::models::builders::RemoteBuildExecutionStrategy;
use crystal_forge::models::deployment_policies::{PoliciesByConfiguration, PolicyCheckResult};
use crystal_forge::models::evaluate_with_policies::{
    ConfirmedSystemFailure, EvaluationFinalizeOutcome, EvaluationPlan, SuccessfulSystemResult,
    finalize_evaluation_attempt, prepare_mock_evaluation_dependency_plans,
};
use crystal_forge::queries::commits::{
    EvalCompleteOutcome, EvalStartOutcome, cancel_commit_evaluation,
    force_cancel_commit_evaluation_attempt, mark_commit_evaluation_complete,
    mark_commit_evaluation_started, reset_stuck_commit_evaluations,
};

struct PlanFixture {
    commit_id: i32,
    derivation_id: i32,
    derivation_path: String,
    attempt: i32,
    attempt_id: Uuid,
}

fn evaluation_plan(system_name: &str, derivation_path: &str) -> EvaluationPlan {
    EvaluationPlan {
        results: Vec::new(),
        policy_checks: vec![PolicyCheckResult {
            system_name: system_name.to_string(),
            cf_agent_enabled: Some(true),
            assigned_results: BTreeMap::new(),
            has_required_packages: None,
            custom_checks: HashMap::new(),
            meets_requirements: true,
            warnings: Vec::new(),
            failed_policies: Vec::new(),
            cve_checks: Vec::new(),
        }],
        successful_systems: vec![SuccessfulSystemResult {
            system_name: system_name.to_string(),
            derivation_target: system_name.to_string(),
            drv_path: derivation_path.to_string(),
            expected_store_path: None,
            cf_agent_enabled: Some(true),
            build_eligible: true,
        }],
        confirmed_failures: Vec::new(),
        had_system_eval_errors: false,
    }
}

async fn prepare_and_finalize_mock_plan(
    pool: &PgPool,
    fixture: &PlanFixture,
    attempt: i32,
) -> EvaluationFinalizeOutcome {
    let plan = evaluation_plan("test-system", &fixture.derivation_path);
    prepare_mock_evaluation_dependency_plans(
        pool,
        fixture.commit_id,
        attempt,
        &plan,
        &PoliciesByConfiguration::new(),
        &crystal_forge::config::BuildConfig::default(),
    )
    .await
    .expect("prepare deterministic attempt plans");
    finalize_evaluation_attempt(pool, fixture.commit_id, attempt, &plan)
        .await
        .expect("finalize prepared attempt")
}

async fn start_replacement(pool: &PgPool, commit_id: i32) -> i32 {
    reset_commit_evaluation(pool, commit_id)
        .await
        .expect("reset evaluation");
    match mark_commit_evaluation_started(pool, commit_id)
        .await
        .expect("start replacement")
    {
        EvalStartOutcome::Started { attempt } => attempt,
        EvalStartOutcome::NoLongerPending => panic!("replacement attempt was not pending"),
    }
}

async fn plan_fixture(pool: &PgPool) -> PlanFixture {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/dependency-plan-{suffix}.git");
    crystal_forge::queries::flakes::insert_flake(
        pool,
        &format!("dependency-plan-{suffix}"),
        &repo_url,
        "main",
        "all_configs",
    )
    .await
    .expect("insert flake");
    crystal_forge::queries::commits::insert_commit_with_metadata(
        pool,
        &suffix,
        &repo_url,
        chrono::Utc::now(),
        Some("dependency plan lifecycle"),
        Some("test"),
    )
    .await
    .expect("insert commit");
    let commit = crystal_forge::queries::commits::get_commit_by_hash(pool, &suffix)
        .await
        .expect("load commit");
    let attempt = 1;
    let attempt_id: Uuid = sqlx::query_scalar(
        r#"
        UPDATE evaluation_attempts
        SET status = 'in_progress', dependency_plan_barrier = 'planning'
        WHERE commit_id = $1 AND attempt_number = $2
        RETURNING id
        "#,
    )
    .bind(commit.id)
    .bind(attempt)
    .fetch_one(pool)
    .await
    .expect("start durable attempt barrier");
    sqlx::query(
        r#"
        UPDATE commits
        SET evaluation_status = 'in_progress',
            evaluation_attempt_count = $2,
            cancellation_requested = FALSE
        WHERE id = $1
        "#,
    )
    .bind(commit.id)
    .bind(attempt)
    .execute(pool)
    .await
    .expect("start evaluation attempt");

    let derivation = insert_derivation_with_target(
        pool,
        Some(&commit),
        "test-system",
        "nixos",
        Some("test-system"),
        Some(true),
    )
    .await
    .expect("insert derivation");
    let derivation_path = format!("/nix/store/{suffix}-test-system.drv");
    sqlx::query(
        r#"
        UPDATE derivations
        SET status_id = 5,
            derivation_path = $2,
            cf_agent_enabled = TRUE,
            policy_requirements_met = TRUE,
            build_preparation_state = 'pending',
            evaluation_attempt_id = $3
        WHERE id = $1
        "#,
    )
    .bind(derivation.id)
    .bind(&derivation_path)
    .bind(attempt_id)
    .execute(pool)
    .await
    .expect("make derivation build eligible");

    PlanFixture {
        commit_id: commit.id,
        derivation_id: derivation.id,
        derivation_path,
        attempt,
        attempt_id,
    }
}

async fn release_fixture(pool: &PgPool, fixture: &PlanFixture) {
    let mut tx = pool.begin().await.expect("begin barrier release");
    sqlx::query(
        "UPDATE evaluation_attempts SET status = 'complete', dependency_plan_barrier = 'ready' WHERE id = $1",
    )
    .bind(fixture.attempt_id)
    .execute(&mut *tx)
    .await
    .expect("release attempt barrier");
    sqlx::query("UPDATE commits SET evaluation_status = 'complete' WHERE id = $1")
        .bind(fixture.commit_id)
        .execute(&mut *tx)
        .await
        .expect("complete commit");
    tx.commit().await.expect("commit barrier release");
}

#[sqlx::test(migrations = "./migrations")]
async fn build_activation_waits_for_a_generation_bound_terminal_plan(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;

    let mut tx = pool.begin().await.expect("begin pre-plan activation");
    let before_plan = create_build_job_for_derivation_tx(&mut tx, fixture.derivation_id, 0)
        .await
        .expect("pre-plan activation query");
    tx.rollback().await.expect("rollback pre-plan activation");
    assert!(before_plan.is_none());

    let generation = mark_dependency_build_plan_calculating(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
    )
    .await
    .expect("start plan")
    .expect("current derivation should accept planning");

    let mut tx = pool.begin().await.expect("begin calculating activation");
    let while_calculating =
        create_build_job_for_derivation_tx(&mut tx, fixture.derivation_id, generation.0)
            .await
            .expect("calculating activation query");
    tx.rollback()
        .await
        .expect("rollback calculating activation");
    assert!(while_calculating.is_none());

    assert_eq!(
        complete_dependency_build_plan(
            &pool,
            fixture.derivation_id,
            &fixture.derivation_path,
            fixture.commit_id,
            fixture.attempt,
            generation,
            12,
            0,
        )
        .await
        .expect("complete plan"),
        DependencyBuildPlanWriteOutcome::Applied,
    );

    let mut tx = pool.begin().await.expect("begin pre-release activation");
    let before_release =
        create_build_job_for_derivation_tx(&mut tx, fixture.derivation_id, generation.0)
            .await
            .expect("pre-release activation query");
    tx.rollback()
        .await
        .expect("rollback pre-release activation");
    assert!(before_release.is_none());

    release_fixture(&pool, &fixture).await;

    let mut tx = pool.begin().await.expect("begin terminal activation");
    let terminal = create_build_job_for_derivation_tx(&mut tx, fixture.derivation_id, generation.0)
        .await
        .expect("terminal activation query");
    tx.commit().await.expect("commit terminal activation");
    assert!(matches!(
        terminal,
        Some(BuildJobInsertOutcome::Inserted { .. })
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn expired_graph_only_plan_becomes_terminal_without_activating_a_build(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    sqlx::query("UPDATE derivations SET build_preparation_state = 'not_required' WHERE id = $1")
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .expect("mark graph-only derivation");
    mark_dependency_build_plan_calculating(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
    )
    .await
    .expect("start graph-only plan")
    .expect("graph-only plan generation");
    sqlx::query(
        "UPDATE derivations SET dependency_build_plan_lease_expires_at = NOW() - INTERVAL '1 second' WHERE id = $1",
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await
    .expect("expire graph-only plan");

    assert_eq!(
        fail_expired_dependency_plans_without_recovery(&pool)
            .await
            .expect("terminate expired graph-only plan"),
        1
    );
    let state: (String, Option<i32>, Option<i32>, bool, i64) = sqlx::query_as(
        r#"
        SELECT dependency_build_plan_status,
               dependency_derivation_count,
               dependency_build_count,
               dependency_build_plan_lease_expires_at IS NULL,
               (SELECT COUNT(*) FROM build_jobs WHERE derivation_id = d.id)::BIGINT
        FROM derivations d
        WHERE id = $1
        "#,
    )
    .bind(fixture.derivation_id)
    .fetch_one(&pool)
    .await
    .expect("load recovered graph-only state");
    assert_eq!(state, ("failed".to_string(), None, None, true, 0));
}

#[sqlx::test(migrations = "./migrations")]
async fn expired_plan_for_an_existing_build_job_becomes_terminal(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    let initial_generation = mark_dependency_build_plan_calculating(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
    )
    .await
    .expect("start initial plan")
    .expect("initial plan generation");
    assert_eq!(
        complete_dependency_build_plan(
            &pool,
            fixture.derivation_id,
            &fixture.derivation_path,
            fixture.commit_id,
            fixture.attempt,
            initial_generation,
            12,
            1,
        )
        .await
        .expect("complete initial plan"),
        DependencyBuildPlanWriteOutcome::Applied,
    );
    release_fixture(&pool, &fixture).await;
    let mut tx = pool.begin().await.expect("begin queued-job insertion");
    assert!(matches!(
        create_build_job_for_derivation_tx(&mut tx, fixture.derivation_id, initial_generation.0,)
            .await
            .expect("insert queued job"),
        Some(BuildJobInsertOutcome::Inserted { .. })
    ));
    tx.commit().await.expect("commit queued job");

    mark_dependency_build_plan_calculating(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
    )
    .await
    .expect("start replacement plan")
    .expect("replacement plan generation");
    sqlx::query("UPDATE build_jobs SET status = 'building' WHERE derivation_id = $1")
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .expect("simulate builder claim during replanning");
    sqlx::query(
        "UPDATE derivations SET dependency_build_plan_lease_expires_at = NOW() - INTERVAL '1 second' WHERE id = $1",
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await
    .expect("expire replacement plan");

    assert_eq!(
        fail_expired_dependency_plans_without_recovery(&pool)
            .await
            .expect("terminate expired queued-job plan"),
        1
    );
    let state: (String, String, bool) = sqlx::query_as(
        r#"
        SELECT d.dependency_build_plan_status,
               bj.status,
               d.dependency_build_plan_lease_expires_at IS NULL
        FROM derivations d
        JOIN build_jobs bj ON bj.derivation_id = d.id
        WHERE d.id = $1
        "#,
    )
    .bind(fixture.derivation_id)
    .fetch_one(&pool)
    .await
    .expect("load recovered queued-job state");
    assert_eq!(state, ("failed".to_string(), "building".to_string(), true));
}

#[sqlx::test(migrations = "./migrations")]
async fn stale_plan_generation_cannot_overwrite_a_newer_terminal_result(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    let old_generation = mark_dependency_build_plan_calculating(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
    )
    .await
    .expect("start old plan")
    .expect("old plan generation");

    sqlx::query(
        "UPDATE derivations SET dependency_build_plan_lease_expires_at = NOW() - INTERVAL '1 second' WHERE id = $1",
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await
    .expect("expire old plan lease");
    let new_generation = mark_dependency_build_plan_calculating(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
    )
    .await
    .expect("start replacement plan")
    .expect("replacement plan generation");
    assert!(new_generation.0 > old_generation.0);

    assert_eq!(
        complete_dependency_build_plan(
            &pool,
            fixture.derivation_id,
            &fixture.derivation_path,
            fixture.commit_id,
            fixture.attempt,
            new_generation,
            20,
            3,
        )
        .await
        .expect("complete replacement plan"),
        DependencyBuildPlanWriteOutcome::Applied,
    );
    assert_eq!(
        fail_dependency_build_plan(
            &pool,
            fixture.derivation_id,
            &fixture.derivation_path,
            fixture.commit_id,
            fixture.attempt,
            old_generation,
        )
        .await
        .expect("attempt stale failure"),
        DependencyBuildPlanWriteOutcome::Stale,
    );

    let persisted: (String, Option<i32>, Option<i32>, i64) = sqlx::query_as(
        r#"
        SELECT dependency_build_plan_status,
               dependency_derivation_count,
               dependency_build_count,
               dependency_build_plan_generation
        FROM derivations
        WHERE id = $1
        "#,
    )
    .bind(fixture.derivation_id)
    .fetch_one(&pool)
    .await
    .expect("load terminal plan");
    assert_eq!(
        persisted,
        ("complete".to_string(), Some(20), Some(3), new_generation.0)
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn failed_plan_is_terminal_but_never_becomes_a_zero_count(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    let generation = mark_dependency_build_plan_calculating(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
    )
    .await
    .expect("start plan")
    .expect("plan generation");
    assert_eq!(
        fail_dependency_build_plan(
            &pool,
            fixture.derivation_id,
            &fixture.derivation_path,
            fixture.commit_id,
            fixture.attempt,
            generation,
        )
        .await
        .expect("persist failed plan"),
        DependencyBuildPlanWriteOutcome::Applied,
    );

    let persisted: (String, Option<i32>, Option<i32>) = sqlx::query_as(
        r#"
        SELECT dependency_build_plan_status,
               dependency_derivation_count,
               dependency_build_count
        FROM derivations
        WHERE id = $1
        "#,
    )
    .bind(fixture.derivation_id)
    .fetch_one(&pool)
    .await
    .expect("load failed plan");
    assert_eq!(persisted, ("failed".to_string(), None, None));

    release_fixture(&pool, &fixture).await;

    let mut tx = pool.begin().await.expect("begin failed-plan activation");
    let activation =
        create_build_job_for_derivation_tx(&mut tx, fixture.derivation_id, generation.0)
            .await
            .expect("failed-plan activation query");
    tx.commit().await.expect("commit failed-plan activation");
    assert!(matches!(
        activation,
        Some(BuildJobInsertOutcome::Inserted { .. })
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn database_trigger_blocks_claim_and_reservation_until_attempt_release(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    let generation = mark_dependency_build_plan_calculating(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
    )
    .await
    .expect("start plan")
    .expect("plan generation");
    fail_dependency_build_plan(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
        generation,
    )
    .await
    .expect("terminal failed plan");

    let queued_insert = sqlx::query(
        "INSERT INTO build_jobs (derivation_id, status, queue_position) VALUES ($1, 'queued', 1)",
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await;
    assert!(
        queued_insert.is_err(),
        "queued insertion bypassed planning barrier"
    );

    let reservation = sqlx::query(
        "INSERT INTO build_reservations (worker_id, derivation_id) VALUES ('barrier-test', $1)",
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await;
    assert!(
        reservation.is_err(),
        "reservation bypassed planning barrier"
    );

    release_fixture(&pool, &fixture).await;
    let blocked = plan_fixture(&pool).await;
    sqlx::query(
        "INSERT INTO build_jobs (derivation_id, status, queue_position) VALUES ($1, 'queued', 1)",
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await
    .expect("released queued insertion");
    sqlx::query("UPDATE build_jobs SET status = 'building' WHERE derivation_id = $1")
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .expect("released claim");
    let job_reassignment =
        sqlx::query("UPDATE build_jobs SET derivation_id = $2 WHERE derivation_id = $1")
            .bind(fixture.derivation_id)
            .bind(blocked.derivation_id)
            .execute(&pool)
            .await;
    assert!(
        job_reassignment.is_err(),
        "build job derivation identity must be immutable"
    );
    let reservation_id: i32 = sqlx::query_scalar(
        "INSERT INTO build_reservations (worker_id, derivation_id) VALUES ('identity-test', $1) RETURNING id",
    )
    .bind(fixture.derivation_id)
    .fetch_one(&pool)
    .await
    .expect("insert released reservation");
    let reservation_reassignment =
        sqlx::query("UPDATE build_reservations SET derivation_id = $2 WHERE id = $1")
            .bind(reservation_id)
            .bind(blocked.derivation_id)
            .execute(&pool)
            .await;
    assert!(
        reservation_reassignment.is_err(),
        "reservation derivation identity must be immutable"
    );
    let reset_error = reset_commit_evaluation(&pool, fixture.commit_id)
        .await
        .expect_err("active building job must reject reset");
    assert!(reset_error.to_string().contains("active build job"));
}

#[sqlx::test(migrations = "./migrations")]
async fn committed_ready_triggers_do_not_wait_for_a_commit_row_lock(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    assert!(matches!(
        prepare_and_finalize_mock_plan(&pool, &fixture, fixture.attempt).await,
        EvaluationFinalizeOutcome::Completed { .. }
    ));

    let mut commit_lock = pool.begin().await.expect("begin commit-row lock");
    sqlx::query("SELECT id FROM commits WHERE id = $1 FOR UPDATE")
        .bind(fixture.commit_id)
        .fetch_one(&mut *commit_lock)
        .await
        .expect("lock released commit row");

    let unlocked_pool = pool.clone();
    let derivation_id = fixture.derivation_id;
    tokio::time::timeout(std::time::Duration::from_secs(2), async move {
        sqlx::query("UPDATE build_jobs SET status = 'building' WHERE derivation_id = $1")
            .bind(derivation_id)
            .execute(&unlocked_pool)
            .await
            .expect("committed ready job transition");
        sqlx::query(
            "INSERT INTO build_reservations (worker_id, derivation_id) VALUES ('snapshot-ready', $1)",
        )
        .bind(derivation_id)
        .execute(&unlocked_pool)
        .await
        .expect("committed ready reservation");
    })
    .await
    .expect("barrier triggers must not request the locked commit row");

    commit_lock
        .rollback()
        .await
        .expect("release commit-row lock");
}

#[sqlx::test(migrations = "./migrations")]
async fn production_claim_strategies_retry_and_requeue_follow_attempt_barrier(pool: PgPool) {
    for strategy in [
        RemoteBuildExecutionStrategy::ServerDerivation,
        RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
    ] {
        let fixture = plan_fixture(&pool).await;
        assert!(matches!(
            prepare_and_finalize_mock_plan(&pool, &fixture, fixture.attempt).await,
            EvaluationFinalizeOutcome::Completed { .. }
        ));
        let builder_id: Uuid = sqlx::query_scalar(
            "INSERT INTO builders (name, public_key, status, arch) VALUES ($1, 'test-key', 'active', 'x86_64-linux') RETURNING id",
        )
        .bind(format!("barrier-builder-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .expect("insert production claim builder");

        let claimed = claim_next_job_atomic(&pool, &builder_id, 1, &[], strategy, None)
            .await
            .expect("claim released job")
            .expect("released job is claimable");
        sqlx::query("UPDATE builders SET status = 'offline' WHERE id = $1")
            .bind(builder_id)
            .execute(&pool)
            .await
            .expect("make claimed job orphaned");
        let recovered = requeue_orphaned_building_jobs(&pool)
            .await
            .expect("requeue orphan through production recovery API");
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id, claimed.id);
        sqlx::query("UPDATE builders SET status = 'active' WHERE id = $1")
            .bind(builder_id)
            .execute(&pool)
            .await
            .expect("reactivate production claim builder");
        let reclaimed = claim_next_job_atomic(&pool, &builder_id, 1, &[], strategy, None)
            .await
            .expect("reclaim recovered job")
            .expect("orphan requeue is claimable");
        mark_job_failed(&pool, reclaimed.id, "retryable test failure", None)
            .await
            .expect("requeue retryable build failure");

        let replacement = start_replacement(&pool, fixture.commit_id).await;
        assert!(
            claim_next_job_atomic(&pool, &builder_id, 1, &[], strategy, None)
                .await
                .expect("query claim during replacement planning")
                .is_none(),
            "replacement planning must hold retryable jobs"
        );
        assert!(matches!(
            prepare_and_finalize_mock_plan(&pool, &fixture, replacement).await,
            EvaluationFinalizeOutcome::Completed { .. }
        ));
        let retried = claim_next_job_atomic(&pool, &builder_id, 1, &[], strategy, None)
            .await
            .expect("claim retry after replacement release")
            .expect("released retry is claimable");
        sqlx::query("UPDATE build_jobs SET status = 'success', completed_at = NOW() WHERE id = $1")
            .bind(retried.id)
            .execute(&pool)
            .await
            .expect("complete claimed retry");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn production_reservation_apis_hold_planning_and_release_terminal_attempt(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    assert!(
        create_reservation(&pool, "direct-reservation", fixture.derivation_id, None)
            .await
            .is_err(),
        "planning attempt must reject direct production reservations"
    );
    assert!(
        claim_next_derivation(&pool, "claim-reservation")
            .await
            .expect("query legacy claim during planning")
            .is_none(),
        "planning attempt must not expose legacy claims"
    );

    assert!(matches!(
        prepare_and_finalize_mock_plan(&pool, &fixture, fixture.attempt).await,
        EvaluationFinalizeOutcome::Completed { .. }
    ));
    let reservation_id =
        create_reservation(&pool, "direct-reservation", fixture.derivation_id, None)
            .await
            .expect("released attempt permits direct production reservation");
    assert!(reservation_id > 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn active_legacy_reservation_rejects_evaluation_reset(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    release_fixture(&pool, &fixture).await;
    sqlx::query(
        "INSERT INTO build_reservations (worker_id, derivation_id) VALUES ('reset-test', $1)",
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await
    .expect("released reservation");

    let reset_error = reset_commit_evaluation(&pool, fixture.commit_id)
        .await
        .expect_err("active reservation must reject reset");
    assert!(reset_error.to_string().contains("active build reservation"));
}

#[sqlx::test(migrations = "./migrations")]
async fn reset_supersedes_the_current_queued_attempt(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    sqlx::query("UPDATE evaluation_attempts SET status = 'queued' WHERE id = $1")
        .bind(fixture.attempt_id)
        .execute(&pool)
        .await
        .expect("return current attempt to queued");
    sqlx::query("UPDATE commits SET evaluation_status = 'pending' WHERE id = $1")
        .bind(fixture.commit_id)
        .execute(&pool)
        .await
        .expect("return commit to pending");

    reset_commit_evaluation(&pool, fixture.commit_id)
        .await
        .expect("reset queued attempt");

    let attempts: Vec<(Uuid, i32, String, String, Option<Uuid>)> = sqlx::query_as(
        r#"
        SELECT id, attempt_number, status, dependency_plan_barrier, parent_attempt_id
        FROM evaluation_attempts
        WHERE commit_id = $1
        ORDER BY attempt_number
        "#,
    )
    .bind(fixture.commit_id)
    .fetch_all(&pool)
    .await
    .expect("load reset attempt lineage");
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].0, fixture.attempt_id);
    assert_eq!(attempts[0].2, "cancelled");
    assert_eq!(attempts[0].3, "cancelled");
    assert_eq!(attempts[1].1, fixture.attempt + 1);
    assert_eq!(attempts[1].2, "queued");
    assert_eq!(attempts[1].4, Some(fixture.attempt_id));
}

#[sqlx::test(migrations = "./migrations")]
async fn production_preparation_retags_same_path_existing_jobs(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    let initial = prepare_and_finalize_mock_plan(&pool, &fixture, fixture.attempt).await;
    assert!(matches!(
        initial,
        EvaluationFinalizeOutcome::Completed { .. }
    ));

    for existing_status in ["queued", "success", "failed"] {
        sqlx::query(
            "UPDATE build_jobs SET status = $2, completed_at = CASE WHEN $2 IN ('success', 'failed') THEN NOW() ELSE NULL END WHERE derivation_id = $1",
        )
        .bind(fixture.derivation_id)
        .bind(existing_status)
        .execute(&pool)
        .await
        .expect("set existing job status");

        let attempt = start_replacement(&pool, fixture.commit_id).await;
        let outcome = prepare_and_finalize_mock_plan(&pool, &fixture, attempt).await;
        assert!(
            matches!(outcome, EvaluationFinalizeOutcome::Completed { .. }),
            "same-path {existing_status} job must not break exact population"
        );
        let state: (Uuid, String, i32, i32) = sqlx::query_as(
            r#"
            SELECT d.evaluation_attempt_id,
                   d.dependency_build_plan_status,
                   ea.dependency_plan_expected_count,
                   ea.dependency_plan_terminal_count
            FROM derivations d
            JOIN evaluation_attempts ea ON ea.id = d.evaluation_attempt_id
            WHERE d.id = $1
            "#,
        )
        .bind(fixture.derivation_id)
        .fetch_one(&pool)
        .await
        .expect("read replacement attempt tag");
        let current_attempt_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM evaluation_attempts WHERE commit_id = $1 AND attempt_number = $2",
        )
        .bind(fixture.commit_id)
        .bind(attempt)
        .fetch_one(&pool)
        .await
        .expect("read current attempt id");
        assert_eq!(state, (current_attempt_id, "complete".to_string(), 1, 1));
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_cross_commit_path_conflict_fails_only_the_affected_plan(pool: PgPool) {
    let first = plan_fixture(&pool).await;
    assert!(matches!(
        prepare_and_finalize_mock_plan(&pool, &first, first.attempt).await,
        EvaluationFinalizeOutcome::Completed { .. }
    ));

    let mut second = plan_fixture(&pool).await;
    second.derivation_path = first.derivation_path.clone();
    let unrelated_path = format!("/nix/store/{}-unrelated.drv", Uuid::new_v4().simple());
    let mut second_plan = evaluation_plan("test-system", &second.derivation_path);
    let unrelated_plan = evaluation_plan("unrelated-system", &unrelated_path);
    second_plan
        .policy_checks
        .extend(unrelated_plan.policy_checks);
    second_plan
        .successful_systems
        .extend(unrelated_plan.successful_systems);

    prepare_mock_evaluation_dependency_plans(
        &pool,
        second.commit_id,
        second.attempt,
        &second_plan,
        &PoliciesByConfiguration::new(),
        &crystal_forge::config::BuildConfig::default(),
    )
    .await
    .expect("prepare release-A compatibility plans");
    assert!(matches!(
        finalize_evaluation_attempt(&pool, second.commit_id, second.attempt, &second_plan)
            .await
            .expect("finalize release-A compatibility plans"),
        EvaluationFinalizeOutcome::Completed { .. }
    ));

    let path_owner: (i64, i32) = sqlx::query_as(
        r#"
        SELECT COUNT(*)::BIGINT, MIN(commit_id)
        FROM derivations
        WHERE derivation_path = $1
        "#,
    )
    .bind(&first.derivation_path)
    .fetch_one(&pool)
    .await
    .expect("load legacy shared-path owner");
    assert_eq!(path_owner, (1, first.commit_id));

    let conflicted: (
        Option<String>,
        String,
        Option<i32>,
        Option<i32>,
        i64,
        String,
        String,
        Uuid,
        i64,
    ) = sqlx::query_as(
        r#"
            SELECT d.derivation_path,
                   d.dependency_build_plan_status,
                   d.dependency_derivation_count,
                   d.dependency_build_count,
                   d.dependency_build_plan_generation,
                   d.build_preparation_state,
                   d.error_message,
                   d.evaluation_attempt_id,
                   (SELECT COUNT(*) FROM build_jobs bj WHERE bj.derivation_id = d.id)::BIGINT
            FROM derivations d
            WHERE d.commit_id = $1
              AND d.derivation_name = 'test-system'
              AND d.derivation_type = 'nixos'
            "#,
    )
    .bind(second.commit_id)
    .fetch_one(&pool)
    .await
    .expect("load compatibility-failed current system");
    assert_eq!(conflicted.0, None);
    assert_eq!(conflicted.1, "failed");
    assert_eq!(conflicted.2, None);
    assert_eq!(conflicted.3, None);
    assert_eq!(conflicted.4, 1);
    assert_eq!(conflicted.5, "not_required");
    assert!(conflicted.6.contains("legacy global path constraint"));
    assert_eq!(conflicted.7, second.attempt_id);
    assert_eq!(conflicted.8, 0, "the conflicted system must not activate");

    let unrelated: (String, String, i32, i32, i64) = sqlx::query_as(
        r#"
        SELECT d.derivation_path,
               d.dependency_build_plan_status,
               d.dependency_derivation_count,
               d.dependency_build_count,
               (SELECT COUNT(*) FROM build_jobs bj WHERE bj.derivation_id = d.id)::BIGINT
        FROM derivations d
        WHERE d.commit_id = $1
          AND d.derivation_name = 'unrelated-system'
          AND d.derivation_type = 'nixos'
        "#,
    )
    .bind(second.commit_id)
    .fetch_one(&pool)
    .await
    .expect("load unrelated completed system");
    assert_eq!(unrelated, (unrelated_path, "complete".to_string(), 0, 0, 1));

    let released: (String, String) = sqlx::query_as(
        r#"
        SELECT c.evaluation_status, ea.dependency_plan_barrier
        FROM commits c
        JOIN evaluation_attempts ea ON ea.id = $2
        WHERE c.id = $1
        "#,
    )
    .bind(second.commit_id)
    .bind(second.attempt_id)
    .fetch_one(&pool)
    .await
    .expect("load released compatibility attempt");
    assert_eq!(released, ("complete".to_string(), "ready".to_string()));
}

#[sqlx::test(migrations = "./migrations")]
async fn startup_recovery_uses_fresh_attempt_when_tagged_system_disappears(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    sqlx::query("UPDATE evaluation_attempts SET dependency_plan_expected_count = 1 WHERE id = $1")
        .bind(fixture.attempt_id)
        .execute(&pool)
        .await
        .expect("record pre-crash expected population");

    reset_stuck_commit_evaluations(&pool)
        .await
        .expect("recover crashed evaluation");
    let attempt = match mark_commit_evaluation_started(&pool, fixture.commit_id)
        .await
        .expect("start post-restart attempt")
    {
        EvalStartOutcome::Started { attempt } => attempt,
        EvalStartOutcome::NoLongerPending => panic!("fresh startup attempt was not queued"),
    };
    assert_ne!(attempt, fixture.attempt);

    let empty_plan = EvaluationPlan {
        results: Vec::new(),
        policy_checks: Vec::new(),
        successful_systems: Vec::new(),
        confirmed_failures: Vec::new(),
        had_system_eval_errors: true,
    };
    prepare_mock_evaluation_dependency_plans(
        &pool,
        fixture.commit_id,
        attempt,
        &empty_plan,
        &PoliciesByConfiguration::new(),
        &crystal_forge::config::BuildConfig::default(),
    )
    .await
    .expect("prepare empty post-restart population");
    let outcome = finalize_evaluation_attempt(&pool, fixture.commit_id, attempt, &empty_plan)
        .await
        .expect("finalize post-restart population");
    assert!(matches!(
        outcome,
        EvaluationFinalizeOutcome::Completed { .. }
    ));

    let attempts: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT attempt_number, status, dependency_plan_barrier FROM evaluation_attempts WHERE commit_id = $1 ORDER BY attempt_number",
    )
    .bind(fixture.commit_id)
    .fetch_all(&pool)
    .await
    .expect("read startup attempt lineage");
    assert_eq!(
        attempts,
        vec![
            (
                fixture.attempt,
                "failed".to_string(),
                "cancelled".to_string()
            ),
            (attempt, "complete".to_string(), "ready".to_string()),
        ]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn startup_recovery_replaces_inconsistent_attempt_state_exactly_once(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    sqlx::query(
        r#"
        UPDATE evaluation_attempts
        SET status = 'complete',
            dependency_plan_barrier = 'ready',
            completed_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(fixture.attempt_id)
    .execute(&pool)
    .await
    .expect("make current attempt inconsistent with in-progress commit");
    let stale_queued_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO evaluation_attempts (
            commit_id, parent_attempt_id, root_attempt_id, attempt_number, status
        )
        VALUES ($1, $2, $2, 2, 'queued')
        RETURNING id
        "#,
    )
    .bind(fixture.commit_id)
    .bind(fixture.attempt_id)
    .fetch_one(&pool)
    .await
    .expect("insert inconsistent queued attempt");

    reset_stuck_commit_evaluations(&pool)
        .await
        .expect("recover inconsistent attempt state");

    let active: Vec<(Uuid, i32, String, Option<Uuid>)> = sqlx::query_as(
        r#"
        SELECT id, attempt_number, status, parent_attempt_id
        FROM evaluation_attempts
        WHERE commit_id = $1 AND status IN ('queued', 'in_progress')
        "#,
    )
    .bind(fixture.commit_id)
    .fetch_all(&pool)
    .await
    .expect("load active replacement attempts");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].1, 3);
    assert_eq!(active[0].2, "queued");
    assert_eq!(active[0].3, Some(stale_queued_id));
    let stale_status: String =
        sqlx::query_scalar("SELECT status FROM evaluation_attempts WHERE id = $1")
            .bind(stale_queued_id)
            .fetch_one(&pool)
            .await
            .expect("load superseded queued attempt");
    assert_eq!(stale_status, "cancelled");
}

#[sqlx::test(migrations = "./migrations")]
async fn post_migration_legacy_attempt_queues_in_progress_but_claims_only_when_complete(
    pool: PgPool,
) {
    let fixture = plan_fixture(&pool).await;
    sqlx::query(
        r#"
        UPDATE evaluation_attempts
        SET status = 'complete',
            dependency_plan_barrier = 'ready',
            completed_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(fixture.attempt_id)
    .execute(&pool)
    .await
    .expect("close fixture attempt before legacy server starts its attempt");
    let legacy_attempt: (Uuid, String) = sqlx::query_as(
        r#"
        INSERT INTO evaluation_attempts (commit_id, attempt_number, status, started_at)
        VALUES ($1, 2, 'in_progress', NOW())
        RETURNING id, dependency_plan_barrier
        "#,
    )
    .bind(fixture.commit_id)
    .fetch_one(&pool)
    .await
    .expect("legacy server inserts attempt with migration default");
    assert_eq!(legacy_attempt.1, "legacy_released");
    sqlx::query(
        r#"
        UPDATE derivations
        SET evaluation_attempt_id = NULL,
            dependency_build_plan_status = 'complete',
            dependency_build_plan_generation = 1,
            dependency_derivation_count = 1,
            dependency_build_count = 0
        WHERE id = $1
        "#,
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await
    .expect("write legacy terminal derivation");
    sqlx::query(
        "UPDATE commits SET evaluation_status = 'in_progress', evaluation_attempt_count = 2 WHERE id = $1",
    )
    .bind(fixture.commit_id)
    .execute(&pool)
    .await
    .expect("select in-progress legacy current attempt");
    let legacy_state: (String, String) = sqlx::query_as(
        r#"
        SELECT c.evaluation_status, ea.status
        FROM commits c
        JOIN evaluation_attempts ea
          ON ea.commit_id = c.id
         AND ea.attempt_number = c.evaluation_attempt_count
        WHERE c.id = $1
        "#,
    )
    .bind(fixture.commit_id)
    .fetch_one(&pool)
    .await
    .expect("read in-progress legacy state");
    assert_eq!(
        legacy_state,
        ("in_progress".to_string(), "in_progress".to_string())
    );
    sqlx::query(
        "INSERT INTO build_jobs (derivation_id, status, queue_position) VALUES ($1, 'queued', 1)",
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await
    .expect("old server can insert a queued job while evaluating");
    let queued_status: String =
        sqlx::query_scalar("SELECT status FROM build_jobs WHERE derivation_id = $1")
            .bind(fixture.derivation_id)
            .fetch_one(&pool)
            .await
            .expect("read legacy queued job");
    assert_eq!(queued_status, "queued");
    let held = sqlx::query("UPDATE build_jobs SET status = 'building' WHERE derivation_id = $1")
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await;
    assert!(
        held.is_err(),
        "legacy in-progress job must not become building"
    );

    sqlx::query(
        "UPDATE evaluation_attempts SET status = 'complete', completed_at = NOW() WHERE id = $1",
    )
    .bind(legacy_attempt.0)
    .execute(&pool)
    .await
    .expect("complete legacy attempt");
    sqlx::query("UPDATE commits SET evaluation_status = 'complete' WHERE id = $1")
        .bind(fixture.commit_id)
        .execute(&pool)
        .await
        .expect("complete legacy commit");
    sqlx::query("UPDATE build_jobs SET status = 'building' WHERE derivation_id = $1")
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .expect("completed legacy job becomes claimable");
}

#[sqlx::test(migrations = "./migrations")]
async fn synthetic_failure_preserves_held_job_for_later_same_path_re_evaluation(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    assert!(matches!(
        prepare_and_finalize_mock_plan(&pool, &fixture, fixture.attempt).await,
        EvaluationFinalizeOutcome::Completed { .. }
    ));

    let failed_attempt = start_replacement(&pool, fixture.commit_id).await;
    let failed_plan = EvaluationPlan {
        results: Vec::new(),
        policy_checks: Vec::new(),
        successful_systems: Vec::new(),
        confirmed_failures: vec![ConfirmedSystemFailure {
            system_name: "test-system".to_string(),
            derivation_target: "test-system".to_string(),
            error: "replacement evaluation failed".to_string(),
        }],
        had_system_eval_errors: true,
    };
    prepare_mock_evaluation_dependency_plans(
        &pool,
        fixture.commit_id,
        failed_attempt,
        &failed_plan,
        &PoliciesByConfiguration::new(),
        &crystal_forge::config::BuildConfig::default(),
    )
    .await
    .expect("prepare failed replacement population");
    finalize_evaluation_attempt(&pool, fixture.commit_id, failed_attempt, &failed_plan)
        .await
        .expect("finalize failed replacement");

    let preserved: (i32, Option<String>, String, i64, Uuid) = sqlx::query_as(
        r#"
        SELECT status_id,
               derivation_path,
               dependency_build_plan_status,
               dependency_build_plan_generation,
               evaluation_attempt_id
        FROM derivations
        WHERE id = $1
        "#,
    )
    .bind(fixture.derivation_id)
    .fetch_one(&pool)
    .await
    .expect("load retained job-owned derivation");
    assert_eq!(preserved.0, 6);
    assert_eq!(
        preserved.1.as_deref(),
        Some(fixture.derivation_path.as_str())
    );
    assert_eq!(preserved.2, "complete");
    assert!(preserved.3 > 0);
    assert_eq!(preserved.4, fixture.attempt_id);
    assert!(
        sqlx::query("UPDATE build_jobs SET status = 'building' WHERE derivation_id = $1")
            .bind(fixture.derivation_id)
            .execute(&pool)
            .await
            .is_err(),
        "failed replacement must hold the retained queued job"
    );

    let successful_attempt = start_replacement(&pool, fixture.commit_id).await;
    assert!(matches!(
        prepare_and_finalize_mock_plan(&pool, &fixture, successful_attempt).await,
        EvaluationFinalizeOutcome::Completed { .. }
    ));
    let current_attempt_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM evaluation_attempts WHERE commit_id = $1 AND attempt_number = $2",
    )
    .bind(fixture.commit_id)
    .bind(successful_attempt)
    .fetch_one(&pool)
    .await
    .expect("load successful replacement attempt");
    let retagged: (i32, String, Uuid) = sqlx::query_as(
        "SELECT status_id, dependency_build_plan_status, evaluation_attempt_id FROM derivations WHERE id = $1",
    )
    .bind(fixture.derivation_id)
    .fetch_one(&pool)
    .await
    .expect("load retagged derivation");
    assert_eq!(retagged, (5, "complete".to_string(), current_attempt_id));
    sqlx::query("UPDATE build_jobs SET status = 'building' WHERE derivation_id = $1")
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .expect("successful replacement releases retained job");
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_completion_api_rejects_planning_barrier(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    assert_eq!(
        mark_commit_evaluation_complete(&pool, fixture.commit_id, fixture.attempt)
            .await
            .expect("planning completion attempt"),
        EvalCompleteOutcome::SupersededOrCancelled
    );
    let state: (String, String) = sqlx::query_as(
        r#"
        SELECT c.evaluation_status, ea.dependency_plan_barrier
        FROM commits c
        JOIN evaluation_attempts ea ON ea.id = $2
        WHERE c.id = $1
        "#,
    )
    .bind(fixture.commit_id)
    .bind(fixture.attempt_id)
    .fetch_one(&pool)
    .await
    .expect("load guarded completion state");
    assert_eq!(state, ("in_progress".to_string(), "planning".to_string()));

    sqlx::query(
        r#"
        UPDATE evaluation_attempts
        SET dependency_plan_barrier = 'ready',
            dependency_plan_expected_count = 0,
            dependency_plan_terminal_count = 0
        WHERE id = $1
        "#,
    )
    .bind(fixture.attempt_id)
    .execute(&pool)
    .await
    .expect("release empty legacy completion barrier");
    assert_eq!(
        mark_commit_evaluation_complete(&pool, fixture.commit_id, fixture.attempt)
            .await
            .expect("ready completion attempt"),
        EvalCompleteOutcome::Completed
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn cancellation_paths_close_attempt_barriers(pool: PgPool) {
    let pending = plan_fixture(&pool).await;
    reset_commit_evaluation(&pool, pending.commit_id)
        .await
        .expect("create pending attempt");
    cancel_commit_evaluation(&pool, pending.commit_id)
        .await
        .expect("cancel pending attempt");

    let forced = plan_fixture(&pool).await;
    sqlx::query(
        "UPDATE commits SET evaluation_status = 'cancelling', cancellation_requested = TRUE WHERE id = $1",
    )
    .bind(forced.commit_id)
    .execute(&pool)
    .await
    .expect("request force cancellation");
    assert!(
        force_cancel_commit_evaluation_attempt(&pool, forced.commit_id, forced.attempt_id)
            .await
            .expect("force cancel attempt")
    );

    let startup = plan_fixture(&pool).await;
    sqlx::query(
        "UPDATE commits SET evaluation_status = 'cancelling', cancellation_requested = TRUE WHERE id = $1",
    )
    .bind(startup.commit_id)
    .execute(&pool)
    .await
    .expect("stage startup cancellation");
    reset_stuck_commit_evaluations(&pool)
        .await
        .expect("finalize startup cancellation");

    for commit_id in [pending.commit_id, forced.commit_id, startup.commit_id] {
        let barriers: Vec<(String, i32, i32)> = sqlx::query_as(
            r#"
            SELECT dependency_plan_barrier,
                   dependency_plan_expected_count,
                   dependency_plan_terminal_count
            FROM evaluation_attempts
            WHERE commit_id = $1 AND status = 'cancelled'
            ORDER BY attempt_number DESC
            "#,
        )
        .bind(commit_id)
        .fetch_all(&pool)
        .await
        .expect("read cancelled barriers");
        assert!(!barriers.is_empty());
        assert!(barriers.iter().all(|(barrier, expected, terminal)| {
            barrier == "cancelled" && terminal <= expected
        }));
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn database_barrier_holds_manually_staged_replacement(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    let generation = mark_dependency_build_plan_calculating(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
    )
    .await
    .expect("start initial plan")
    .expect("initial plan generation");
    complete_dependency_build_plan(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
        generation,
        4,
        0,
    )
    .await
    .expect("complete initial plan");
    release_fixture(&pool, &fixture).await;
    let mut tx = pool.begin().await.expect("begin queued job insertion");
    create_build_job_for_derivation_tx(&mut tx, fixture.derivation_id, generation.0)
        .await
        .expect("insert initial queued job")
        .expect("eligible initial queued job");
    tx.commit().await.expect("commit initial queued job");

    reset_commit_evaluation(&pool, fixture.commit_id)
        .await
        .expect("replace completed evaluation");
    let held_claim = sqlx::query(
        "UPDATE build_jobs SET status = 'building' WHERE derivation_id = $1 AND status = 'queued'",
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await;
    assert!(
        held_claim.is_err(),
        "replacement planning must hold queued jobs"
    );
    let queued_status: String =
        sqlx::query_scalar("SELECT status FROM build_jobs WHERE derivation_id = $1")
            .bind(fixture.derivation_id)
            .fetch_one(&pool)
            .await
            .expect("load held job");
    assert_eq!(queued_status, "queued");

    let (replacement_id, replacement_attempt): (Uuid, i32) = sqlx::query_as(
        "SELECT id, attempt_number FROM evaluation_attempts WHERE commit_id = $1 ORDER BY attempt_number DESC LIMIT 1",
    )
    .bind(fixture.commit_id)
    .fetch_one(&pool)
    .await
    .expect("load replacement attempt");
    let mut tx = pool.begin().await.expect("begin replacement release");
    sqlx::query(
        r#"
        UPDATE commits
        SET evaluation_status = 'complete', evaluation_attempt_count = $2
        WHERE id = $1
        "#,
    )
    .bind(fixture.commit_id)
    .bind(replacement_attempt)
    .execute(&mut *tx)
    .await
    .expect("complete replacement commit");
    sqlx::query(
        r#"
        UPDATE evaluation_attempts
        SET status = 'complete',
            dependency_plan_barrier = 'ready',
            dependency_plan_expected_count = 1,
            dependency_plan_terminal_count = 1
        WHERE id = $1
        "#,
    )
    .bind(replacement_id)
    .execute(&mut *tx)
    .await
    .expect("release replacement barrier");
    sqlx::query("UPDATE derivations SET evaluation_attempt_id = $2 WHERE id = $1")
        .bind(fixture.derivation_id)
        .bind(replacement_id)
        .execute(&mut *tx)
        .await
        .expect("tag replacement derivation");
    tx.commit().await.expect("commit replacement release");

    sqlx::query(
        "UPDATE build_jobs SET status = 'building' WHERE derivation_id = $1 AND status = 'queued'",
    )
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await
    .expect("released replacement job becomes claimable");
    let building_status: String =
        sqlx::query_scalar("SELECT status FROM build_jobs WHERE derivation_id = $1")
            .bind(fixture.derivation_id)
            .fetch_one(&pool)
            .await
            .expect("load reopened job");
    assert_eq!(building_status, "building");
}

#[sqlx::test(migrations = "./migrations")]
async fn graph_query_uses_explicit_dependency_count_not_legacy_closure_total(pool: PgPool) {
    let fixture = plan_fixture(&pool).await;
    let generation = mark_dependency_build_plan_calculating(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
    )
    .await
    .expect("start plan")
    .expect("plan generation");
    complete_dependency_build_plan(
        &pool,
        fixture.derivation_id,
        &fixture.derivation_path,
        fixture.commit_id,
        fixture.attempt,
        generation,
        17,
        0,
    )
    .await
    .expect("complete zero-work plan");
    sqlx::query("UPDATE derivations SET closure_total = 999 WHERE id = $1")
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .expect("write conflicting legacy total");

    let rows = fetch_eval_dependency_breakdown(&pool, fixture.commit_id)
        .await
        .expect("fetch dependency graph");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].dependency_derivation_count, Some(17));
    assert_eq!(rows[0].dependency_build_count, Some(0));
    assert_eq!(rows[0].build_plan_status, "complete");
}
