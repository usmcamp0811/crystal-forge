use crystal_forge::queries::build_jobs::{
    BuildJobInsertOutcome, create_build_job_for_derivation_tx,
    fail_expired_dependency_plans_without_recovery,
};
use crystal_forge::queries::commits::fetch_eval_dependency_breakdown;
use crystal_forge::queries::derivations::{
    DependencyBuildPlanWriteOutcome, complete_dependency_build_plan, fail_dependency_build_plan,
    insert_derivation_with_target, mark_dependency_build_plan_calculating,
};
use sqlx::PgPool;
use uuid::Uuid;

struct PlanFixture {
    commit_id: i32,
    derivation_id: i32,
    derivation_path: String,
    attempt: i32,
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
            build_preparation_state = 'pending'
        WHERE id = $1
        "#,
    )
    .bind(derivation.id)
    .bind(&derivation_path)
    .execute(pool)
    .await
    .expect("make derivation build eligible");

    PlanFixture {
        commit_id: commit.id,
        derivation_id: derivation.id,
        derivation_path,
        attempt,
    }
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
