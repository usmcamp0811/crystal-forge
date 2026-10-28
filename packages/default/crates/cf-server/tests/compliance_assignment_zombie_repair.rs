//! Upgrade regression tests for incomplete compliance assignment lineages.

use sqlx::{PgPool, migrate::Migrate};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

async fn apply_migrations_through(pool: &PgPool, version: i64) {
    let mut connection = pool.acquire().await.expect("acquire migration connection");
    connection
        .ensure_migrations_table()
        .await
        .expect("create migrations table");
    for migration in MIGRATOR
        .iter()
        .filter(|migration| migration.version <= version)
    {
        connection
            .apply(migration)
            .await
            .unwrap_or_else(|error| panic!("apply migration {}: {error}", migration.version));
    }
}

async fn apply_migration(pool: &PgPool, version: i64) {
    let migration = MIGRATOR
        .iter()
        .find(|migration| migration.version == version)
        .unwrap_or_else(|| panic!("migration {version} is not embedded"));
    let mut connection = pool.acquire().await.expect("acquire migration connection");
    connection
        .apply(migration)
        .await
        .unwrap_or_else(|error| panic!("apply migration {version}: {error}"));
}

#[sqlx::test(migrations = false)]
async fn migration_0257_removes_legacy_writer_and_repairs_incomplete_lineages(pool: PgPool) {
    apply_migrations_through(&pool, 256).await;

    let bundle_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundles(name,framework,version,layer,owner) \
         VALUES($1,'NIST','1.0','fleet','TASK-440') RETURNING id",
    )
    .bind(format!("task440-zombie-{}", Uuid::new_v4().simple()))
    .fetch_one(&pool)
    .await
    .expect("insert bundle");
    let bundle_version_id: Uuid =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_one(&pool)
            .await
            .expect("load draft version");
    let environment_id: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("task440-zombie-{}", Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .expect("insert environment");

    // Reproduce the production producer before applying the repair migration.
    sqlx::query(
        "INSERT INTO compliance_bundle_environments(bundle_id, environment_id) VALUES($1, $2)",
    )
    .bind(bundle_id)
    .bind(environment_id)
    .execute(&pool)
    .await
    .expect("insert legacy environment membership");
    let zombie_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM compliance_bundle_assignments \
         WHERE bundle_id = $1 AND environment_id = $2 AND active \
           AND current_version_id IS NULL",
    )
    .bind(bundle_id)
    .bind(environment_id)
    .fetch_one(&pool)
    .await
    .expect("legacy trigger must reproduce incomplete lineage");

    // Preserve a complete lineage across the repair to prove that 0257 changes
    // only incomplete active rows.
    let healthy_environment_id: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("task440-healthy-{}", Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .expect("insert healthy environment");
    let healthy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_assignments( \
             bundle_id,bundle_version_id,scope_type,environment_id, \
             enforcement_mode,assignment_overlay_digest) \
         VALUES($1,$2,'environment',$3,'report_only','task440-healthy') RETURNING id",
    )
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(healthy_environment_id)
    .fetch_one(&pool)
    .await
    .expect("insert healthy lineage");
    let healthy_version_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_assignment_versions( \
             assignment_id,version_number,bundle_version_id,enforcement_mode, \
             assignment_overlay_digest) \
         VALUES($1,1,$2,'report_only','task440-healthy') RETURNING id",
    )
    .bind(healthy_id)
    .bind(bundle_version_id)
    .fetch_one(&pool)
    .await
    .expect("insert healthy assignment version");
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id=$2 WHERE id=$1")
        .bind(healthy_id)
        .bind(healthy_version_id)
        .execute(&pool)
        .await
        .expect("select healthy assignment version");

    apply_migration(&pool, 257).await;

    let repaired: (bool, Option<Uuid>) = sqlx::query_as(
        "SELECT active, current_version_id FROM compliance_bundle_assignments WHERE id = $1",
    )
    .bind(zombie_id)
    .fetch_one(&pool)
    .await
    .expect("load repaired lineage");
    assert_eq!(repaired, (false, None));
    let healthy_after: (bool, Option<Uuid>, String) = sqlx::query_as(
        "SELECT active,current_version_id,enforcement_mode \
         FROM compliance_bundle_assignments WHERE id=$1",
    )
    .bind(healthy_id)
    .fetch_one(&pool)
    .await
    .expect("load preserved healthy lineage");
    assert_eq!(
        healthy_after,
        (true, Some(healthy_version_id), "report_only".to_string())
    );
    let healthy_version_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM compliance_bundle_assignment_versions \
         WHERE assignment_id=$1 AND id=$2 AND version_number=1",
    )
    .bind(healthy_id)
    .bind(healthy_version_id)
    .fetch_one(&pool)
    .await
    .expect("count preserved healthy history");
    assert_eq!(healthy_version_count, 1);
    let active_zombies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM compliance_bundle_assignments \
         WHERE active AND current_version_id IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("count active incomplete lineages");
    assert_eq!(active_zombies, 0);

    let legacy_objects: i64 = sqlx::query_scalar(
        "SELECT \
           (SELECT count(*) FROM pg_trigger \
            WHERE tgname IN ('trigger_sync_bundle_env_assignment_insert', \
                             'trigger_sync_bundle_env_assignment_delete') AND NOT tgisinternal) + \
           (SELECT count(*) FROM pg_proc \
            WHERE proname IN ('sync_bundle_env_assignment_insert', \
                              'sync_bundle_env_assignment_delete')) + \
           (SELECT count(*) FROM pg_indexes \
            WHERE indexname IN ('compliance_bundle_assignments_environment_unique', \
                                'compliance_bundle_assignments_system_unique'))",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect removed legacy objects");
    assert_eq!(legacy_objects, 0);

    let second_environment_id: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("task440-no-writer-{}", Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .expect("insert second environment");
    sqlx::query(
        "INSERT INTO compliance_bundle_environments(bundle_id, environment_id) VALUES($1, $2)",
    )
    .bind(bundle_id)
    .bind(second_environment_id)
    .execute(&pool)
    .await
    .expect("insert membership after repair");
    let implicit_assignments: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM compliance_bundle_assignments WHERE environment_id = $1",
    )
    .bind(second_environment_id)
    .fetch_one(&pool)
    .await
    .expect("count implicit assignments");
    assert_eq!(implicit_assignments, 0);

    // A healthy explicit replacement can reuse the same bundle version and
    // target. The inactive incomplete lineage and obsolete pre-lineage index do
    // not block it.
    let mut replacement_tx = pool.begin().await.expect("begin replacement");
    let replacement_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_assignments( \
             bundle_id,bundle_version_id,scope_type,environment_id, \
             enforcement_mode,assignment_overlay_digest) \
         VALUES($1,$2,'environment',$3,'enforce','task440-replacement') RETURNING id",
    )
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(environment_id)
    .fetch_one(&mut *replacement_tx)
    .await
    .expect("insert replacement lineage");
    let replacement_version_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_assignment_versions( \
             assignment_id,version_number,bundle_version_id,enforcement_mode, \
             assignment_overlay_digest) \
         VALUES($1,1,$2,'enforce','task440-replacement') RETURNING id",
    )
    .bind(replacement_id)
    .bind(bundle_version_id)
    .fetch_one(&mut *replacement_tx)
    .await
    .expect("insert replacement version");
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id=$2 WHERE id=$1")
        .bind(replacement_id)
        .bind(replacement_version_id)
        .execute(&mut *replacement_tx)
        .await
        .expect("select replacement version");
    replacement_tx.commit().await.expect("commit replacement");

    let authoritative_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT assignment.id FROM compliance_bundle_assignments assignment \
         JOIN compliance_bundle_assignment_versions version \
           ON version.id = assignment.current_version_id \
          AND version.assignment_id = assignment.id \
         WHERE assignment.bundle_id=$1 AND assignment.environment_id=$2 \
           AND assignment.active",
    )
    .bind(bundle_id)
    .bind(environment_id)
    .fetch_all(&pool)
    .await
    .expect("load authoritative assignments");
    assert_eq!(authoritative_ids, vec![replacement_id]);
}
