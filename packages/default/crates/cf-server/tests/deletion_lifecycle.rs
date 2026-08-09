use crystal_forge::queries::{
    compliance::{self, BundleDeleteOutcome},
    deployment_policies::{self, PolicyDeleteOutcome},
};
use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> PgPool {
    PgPool::connect(
        &std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
            .expect("CRYSTAL_FORGE_TEST_DATABASE_URL must name the disposable test database"),
    )
    .await
    .expect("connect to test database")
}

async fn draft_policy(pool: &PgPool) -> (Uuid, Uuid) {
    let policy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies (name, policy_type, config, enabled) VALUES ($1, 'custom_check', '{\"expression\":\"true\"}', true) RETURNING id",
    )
    .bind(format!("delete-test-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("insert draft policy");
    let version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_one(pool)
    .await
    .expect("policy draft version");
    (policy_id, version_id)
}

async fn draft_bundle(pool: &PgPool) -> (Uuid, Uuid) {
    let bundle_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundles (name, framework, version, layer) VALUES ($1, 'test', '1.0', 'fleet') RETURNING id",
    )
    .bind(format!("delete-test-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("insert draft bundle");
    let version_id: Uuid =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_one(pool)
            .await
            .expect("bundle draft version");
    (bundle_id, version_id)
}

async fn accept_policy(pool: &PgPool, policy_id: Uuid, version_id: Uuid) {
    let mut tx = pool.begin().await.expect("begin policy publication");
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("clear policy draft pointer");
    sqlx::query("UPDATE deployment_policy_versions SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP WHERE id = $1")
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .expect("accept policy version");
    sqlx::query("UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("set policy published pointer");
    tx.commit().await.expect("commit policy publication");
}

async fn accept_bundle(pool: &PgPool, bundle_id: Uuid, version_id: Uuid) {
    let mut tx = pool.begin().await.expect("begin bundle publication");
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE id = $1")
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("clear bundle draft pointer");
    sqlx::query("UPDATE compliance_bundle_versions SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP WHERE id = $1")
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .expect("accept bundle version");
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("set bundle published pointer");
    tx.commit().await.expect("commit bundle publication");
}

async fn assigned_bundle(pool: &PgPool) -> Uuid {
    let environment_id: Uuid =
        sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
            .bind(format!("delete-env-{}", Uuid::new_v4()))
            .fetch_one(pool)
            .await
            .expect("insert environment");
    let (bundle_id, _) = draft_bundle(pool).await;
    sqlx::query(
        "INSERT INTO compliance_bundle_environments (bundle_id, environment_id) VALUES ($1, $2)",
    )
    .bind(bundle_id)
    .bind(environment_id)
    .execute(pool)
    .await
    .expect("create bundle assignment");
    bundle_id
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn deletion_lifecycle_database_matrix() {
    let pool = pool().await;

    let (policy_id, policy_version_id) = draft_policy(&pool).await;
    assert_eq!(
        deployment_policies::delete_deployment_policy(&pool, &policy_id)
            .await
            .unwrap(),
        PolicyDeleteOutcome::Deleted
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM deployment_policy_versions WHERE id = $1"
        )
        .bind(policy_version_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        deployment_policies::delete_deployment_policy(&pool, &Uuid::new_v4())
            .await
            .unwrap(),
        PolicyDeleteOutcome::NotFound
    );

    let (immutable_policy_id, immutable_policy_version_id) = draft_policy(&pool).await;
    accept_policy(&pool, immutable_policy_id, immutable_policy_version_id).await;
    assert!(matches!(
        deployment_policies::delete_deployment_policy(&pool, &immutable_policy_id)
            .await
            .unwrap(),
        PolicyDeleteOutcome::BlockedByImmutableHistory { .. }
    ));

    let (member_policy_id, member_policy_version_id) = draft_policy(&pool).await;
    let (_, bundle_version_id) = draft_bundle(&pool).await;
    sqlx::query("INSERT INTO compliance_bundle_version_policies (bundle_version_id, policy_version_id, policy_order) VALUES ($1, $2, 0)").bind(bundle_version_id).bind(member_policy_version_id).execute(&pool).await.unwrap();
    assert!(matches!(
        deployment_policies::delete_deployment_policy(&pool, &member_policy_id)
            .await
            .unwrap(),
        PolicyDeleteOutcome::BlockedByReferences { .. }
    ));

    for table in [
        "compliance_assignment_additions",
        "compliance_assignment_exclusions",
        "compliance_assignment_value_overrides",
    ] {
        let (referenced_policy_id, referenced_policy_version_id) = draft_policy(&pool).await;
        let assigned_bundle_id = assigned_bundle(&pool).await;
        let assignment_id: Uuid =
            sqlx::query_scalar("SELECT id FROM compliance_bundle_assignments WHERE bundle_id = $1")
                .bind(assigned_bundle_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let assignment_version_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO compliance_bundle_assignment_versions
                (assignment_id, version_number, bundle_version_id, enforcement_mode, assignment_overlay_digest)
            SELECT id, 1, bundle_version_id, enforcement_mode, 'test'
            FROM compliance_bundle_assignments
            WHERE bundle_id = $1
            RETURNING id
            "#,
        )
        .bind(assigned_bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE bundle_id = $2",
        )
        .bind(assignment_version_id)
        .bind(assigned_bundle_id)
        .execute(&pool)
        .await
        .unwrap();
        let statement = if table == "compliance_assignment_value_overrides" {
            "INSERT INTO compliance_assignment_value_overrides (assignment_id, assignment_version_id, policy_version_id, value_path, value) VALUES ($1, $2, $3, 'enabled', 'true')".to_string()
        } else {
            format!(
                "INSERT INTO {table} (assignment_id, assignment_version_id, policy_version_id) VALUES ($1, $2, $3)"
            )
        };
        sqlx::query(&statement)
            .bind(assignment_id)
            .bind(assignment_version_id)
            .bind(referenced_policy_version_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            deployment_policies::delete_deployment_policy(&pool, &referenced_policy_id)
                .await
                .unwrap(),
            PolicyDeleteOutcome::BlockedByReferences { .. }
        ));
    }

    let (bundle_id, bundle_version_id) = draft_bundle(&pool).await;
    assert_eq!(
        compliance::delete_bundle(&pool, bundle_id).await.unwrap(),
        BundleDeleteOutcome::Deleted
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM compliance_bundle_versions WHERE id = $1"
        )
        .bind(bundle_version_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        compliance::delete_bundle(&pool, Uuid::new_v4())
            .await
            .unwrap(),
        BundleDeleteOutcome::NotFound
    );

    let (immutable_bundle_id, immutable_bundle_version_id) = draft_bundle(&pool).await;
    accept_bundle(&pool, immutable_bundle_id, immutable_bundle_version_id).await;
    assert!(matches!(
        compliance::delete_bundle(&pool, immutable_bundle_id)
            .await
            .unwrap(),
        BundleDeleteOutcome::BlockedByImmutableHistory { .. }
    ));

    let active_bundle_id = assigned_bundle(&pool).await;
    assert!(matches!(
        compliance::delete_bundle(&pool, active_bundle_id)
            .await
            .unwrap(),
        BundleDeleteOutcome::BlockedByAssignments { .. }
    ));

    let inactive_bundle_id = assigned_bundle(&pool).await;
    sqlx::query("UPDATE compliance_bundle_assignments SET active = false WHERE bundle_id = $1")
        .bind(inactive_bundle_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        compliance::delete_bundle(&pool, inactive_bundle_id)
            .await
            .unwrap(),
        BundleDeleteOutcome::BlockedByAssignments { .. }
    ));
}
