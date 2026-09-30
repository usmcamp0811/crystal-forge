//! Reproduces a source insert between 0297 backfill and insert-trigger coverage.
//! All trigger toggles occur only in SQLx-created disposable per-test databases.

use sqlx::{PgPool, migrate::Migrate};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

async fn migrate_through(pool: &PgPool, version: i64) {
    let mut connection = pool.acquire().await.unwrap();
    connection.ensure_migrations_table().await.unwrap();
    for migration in MIGRATOR.iter().filter(|row| row.version <= version) {
        connection.apply(migration).await.unwrap();
    }
}

async fn migrate_one(pool: &PgPool, version: i64) -> Result<(), sqlx::migrate::MigrateError> {
    let migration = MIGRATOR.iter().find(|row| row.version == version).unwrap();
    if version == 298 {
        assert_eq!(
            migration.sql.as_ref(),
            include_str!("../migrations/0298_repair_risk_acceptance_identity_gap.sql")
        );
    }
    pool.acquire()
        .await
        .unwrap()
        .apply(migration)
        .await
        .map(|_| ())
}

fn id(suffix: u16) -> Uuid {
    Uuid::parse_str(&format!("00000000-0000-4000-8000-{suffix:012}")).unwrap()
}

async fn number(pool: &PgPool, kind: &str, source_id: Uuid) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT human_number FROM risk_acceptance_source_ids WHERE source_kind=$1 AND source_id=$2",
    )
    .bind(kind)
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

async fn assert_audited_uuid_fields(pool: &PgPool) {
    let invalid: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT COALESCE(metadata->>'source_type',''),COALESCE(metadata->>'predecessor_id',''),COALESCE(metadata->>'successor_id','') FROM admin_audit_events WHERE action='cve_acceptance_renewed' AND (COALESCE(metadata->>'source_type','') NOT IN ('host','environment') OR COALESCE(metadata->>'predecessor_id','') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' OR COALESCE(metadata->>'successor_id','') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')",
    ).fetch_all(pool).await.unwrap();
    assert!(invalid.is_empty(), "invalid audit identities: {invalid:?}");
}

async fn seed_0296(pool: &PgPool) {
    migrate_through(pool, 296).await;
    sqlx::raw_sql(include_str!("risk_acceptance_0296_fixture.sql"))
        .execute(pool)
        .await
        .unwrap();
    migrate_one(pool, 297).await.unwrap();
}

async fn disable_mapping_triggers(pool: &PgPool) {
    for (table, trigger) in [
        ("finding_waivers", "risk_acceptance_waiver_insert"),
        ("cve_system_dispositions", "risk_acceptance_host_insert"),
        (
            "cve_environment_dispositions",
            "risk_acceptance_environment_insert",
        ),
    ] {
        sqlx::query(&format!("ALTER TABLE {table} DISABLE TRIGGER {trigger}"))
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn enable_mapping_triggers(pool: &PgPool) {
    for (table, trigger) in [
        ("finding_waivers", "risk_acceptance_waiver_insert"),
        ("cve_system_dispositions", "risk_acceptance_host_insert"),
        (
            "cve_environment_dispositions",
            "risk_acceptance_environment_insert",
        ),
    ] {
        sqlx::query(&format!("ALTER TABLE {table} ENABLE TRIGGER {trigger}"))
            .execute(pool)
            .await
            .unwrap();
    }
}

/// Checks the populated 0296→0297→0298 upgrade and a new 0298 installation.
#[sqlx::test(migrations = false)]
async fn repairs_unmapped_successors_and_roots_without_renumbering(pool: PgPool) {
    seed_0296(&pool).await;
    let historical: Vec<(String, Uuid, i64)> = sqlx::query_as(
        "SELECT source_kind,source_id,human_number FROM risk_acceptance_source_ids ORDER BY source_kind,source_id",
    ).fetch_all(&pool).await.unwrap();
    assert_eq!(historical.len(), 7);
    disable_mapping_triggers(&pool).await;

    // These writes simulate the precise 0297 window; their source UUIDs and
    // authoritative predecessor links are persisted before repair begins.
    sqlx::query("INSERT INTO finding_waivers(id,finding_id,justification,policy_version_id,observation_token,observation_snapshot,created_by,predecessor_id,predecessor_updated_at,review_due_at) SELECT $1,finding_id,'third review',policy_version_id,observation_token,observation_snapshot,created_by,id,updated_at,current_date+90 FROM finding_waivers WHERE id=$2")
        .bind(id(103)).bind(id(102)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO deployment_policies(id,name,policy_type,config,enabled) VALUES($1,'ra-new-policy','custom_check','{}',false)")
        .bind(id(96)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO poam_findings(id,system_id,policy_lineage_id) VALUES($1,$2,$3)")
        .bind(id(97))
        .bind(id(93))
        .bind(id(96))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO finding_waivers(id,finding_id,justification,policy_version_id,observation_token,observation_snapshot,created_by) VALUES($1,$2,'independent approval',$3,'observed','{}',$4)")
        .bind(id(104)).bind(id(97)).bind(Uuid::new_v4()).bind(id(91))
        .execute(&pool).await.unwrap();

    for (table, scope, predecessor, successor, kind) in [
        (
            "cve_system_dispositions",
            "system_id",
            id(203),
            id(204),
            "host",
        ),
        (
            "cve_environment_dispositions",
            "environment_id",
            id(302),
            id(303),
            "environment",
        ),
    ] {
        sqlx::query(&format!("UPDATE {table} SET retired_at=now(),retired_by=$1,retirement_reason='renewed' WHERE id=$2"))
            .bind(id(91)).bind(predecessor).execute(&pool).await.unwrap();
        sqlx::query(&format!("INSERT INTO {table}(id,canonical_cve_id,canonical_package_name,{scope},state,justification,accepted_by,accepted_at) SELECT $1,canonical_cve_id,canonical_package_name,{scope},'accepted','successor',$2,now() FROM {table} WHERE id=$3"))
            .bind(successor).bind(id(91)).bind(predecessor).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO admin_audit_events(actor_user_id,action,target,metadata) VALUES($1,'cve_acceptance_renewed','test',$2)")
            .bind(id(91))
            .bind(serde_json::json!({"source_type":kind,"predecessor_id":predecessor,"successor_id":successor}))
            .execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO cve_system_dispositions(id,canonical_cve_id,canonical_package_name,system_id,state,justification,accepted_by,accepted_at) VALUES($1,'CVE-2099-12345','new-host-package',$2,'accepted','fresh',$3,now())")
        .bind(id(205)).bind(id(93)).bind(id(91)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO cve_environment_dispositions(id,canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) VALUES($1,'CVE-2099-12345','new-env-package',$2,'accepted','fresh',$3,now())")
        .bind(id(304)).bind(id(92)).bind(id(91)).execute(&pool).await.unwrap();
    for (kind, source_id) in [
        ("policy_waiver", id(103)),
        ("policy_waiver", id(104)),
        ("cve_host", id(204)),
        ("cve_host", id(205)),
        ("cve_environment", id(303)),
        ("cve_environment", id(304)),
    ] {
        assert_eq!(number(&pool, kind, source_id).await, None);
    }
    enable_mapping_triggers(&pool).await;
    assert_audited_uuid_fields(&pool).await;
    migrate_one(&pool, 298).await.unwrap();

    for (kind, source_id, original) in historical {
        assert_eq!(number(&pool, &kind, source_id).await, Some(original));
    }
    for (kind, predecessor, successor) in [
        ("policy_waiver", id(102), id(103)),
        ("cve_host", id(203), id(204)),
        ("cve_environment", id(302), id(303)),
    ] {
        assert_eq!(
            number(&pool, kind, successor).await,
            number(&pool, kind, predecessor).await
        );
        assert_ne!(predecessor, successor);
    }
    let roots: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT human_number FROM risk_acceptance_source_ids ORDER BY human_number",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(roots.len(), 6);
    for (kind, source_id) in [
        ("policy_waiver", id(104)),
        ("cve_host", id(205)),
        ("cve_environment", id(304)),
    ] {
        assert!(number(&pool, kind, source_id).await.unwrap_or_default() > 3);
    }
    let all_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM risk_acceptance_source_ids")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(all_rows, 13);
    let active_host: i64 = sqlx::query_scalar("SELECT count(*) FROM cve_system_dispositions WHERE canonical_package_name='openssl' AND retired_at IS NULL")
        .fetch_one(&pool).await.unwrap();
    let active_env: i64 = sqlx::query_scalar("SELECT count(*) FROM cve_environment_dispositions WHERE canonical_package_name='openssl' AND retired_at IS NULL")
        .fetch_one(&pool).await.unwrap();
    assert_eq!((active_host, active_env), (1, 1));
    sqlx::query("INSERT INTO cve_system_dispositions(id,canonical_cve_id,canonical_package_name,system_id,state,justification,accepted_by,accepted_at) VALUES($1,'CVE-2099-12345','after-upgrade',$2,'accepted','first visibility',$3,now())")
        .bind(id(206)).bind(id(93)).bind(id(91)).execute(&pool).await.unwrap();
    assert!(number(&pool, "cve_host", id(206)).await.unwrap() > *roots.last().unwrap());
}

/// Refuses to guess a chain from a CVE/scope match without an audit event.
#[sqlx::test(migrations = false)]
async fn missing_cve_renewal_audit_aborts_without_partial_identity(pool: PgPool) {
    seed_0296(&pool).await;
    disable_mapping_triggers(&pool).await;
    sqlx::query("UPDATE cve_system_dispositions SET retired_at=now(),retired_by=$1,retirement_reason='renewed' WHERE id=$2")
        .bind(id(91)).bind(id(203)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO cve_system_dispositions(id,canonical_cve_id,canonical_package_name,system_id,state,justification,accepted_by,accepted_at) VALUES($1,'CVE-2099-12345','openssl',$2,'accepted','same tuple',$3,now())")
        .bind(id(204)).bind(id(93)).bind(id(91)).execute(&pool).await.unwrap();
    enable_mapping_triggers(&pool).await;
    assert_audited_uuid_fields(&pool).await;
    let before: Vec<(String,Uuid,i64)> = sqlx::query_as("SELECT source_kind,source_id,human_number FROM risk_acceptance_source_ids ORDER BY source_kind,source_id")
        .fetch_all(&pool).await.unwrap();
    let error = migrate_one(&pool, 298).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Missing authoritative historical CVE renewal lineage"),
        "{error}"
    );
    let after: Vec<(String,Uuid,i64)> = sqlx::query_as("SELECT source_kind,source_id,human_number FROM risk_acceptance_source_ids ORDER BY source_kind,source_id")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(before, after);
    assert_eq!(number(&pool, "cve_host", id(204)).await, None);
}

/// A duplicate audit edge cannot select a successor by event order.
#[sqlx::test(migrations = false)]
async fn ambiguous_cve_renewal_audit_aborts_upgrade(pool: PgPool) {
    seed_0296(&pool).await;
    sqlx::query("INSERT INTO admin_audit_events(actor_user_id,action,target,metadata) SELECT actor_user_id,action,target,metadata FROM admin_audit_events WHERE action='cve_acceptance_renewed' AND metadata->>'predecessor_id'=$1 LIMIT 1")
        .bind(id(201).to_string()).execute(&pool).await.unwrap();
    let original: Vec<(String,Uuid,i64)> = sqlx::query_as("SELECT source_kind,source_id,human_number FROM risk_acceptance_source_ids ORDER BY source_kind,source_id")
        .fetch_all(&pool).await.unwrap();
    let error = migrate_one(&pool, 298).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Ambiguous or missing historical acceptance renewal source"),
        "{error}"
    );
    let after: Vec<(String,Uuid,i64)> = sqlx::query_as("SELECT source_kind,source_id,human_number FROM risk_acceptance_source_ids ORDER BY source_kind,source_id")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(original, after);
}

/// Fresh installations must retain the insert-trigger first-visibility contract.
#[sqlx::test(migrations = "./migrations")]
async fn fresh_0298_creates_identity_on_all_three_source_inserts(pool: PgPool) {
    sqlx::query("INSERT INTO users(id,username,first_name,last_name,email) VALUES($1,'ra-fresh','Risk','Tester','ra-fresh@example.invalid')")
        .bind(id(91)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO environments(id,name) VALUES($1,'ra-fresh-environment')")
        .bind(id(92))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO systems(id,hostname,public_key,derivation,environment_id) VALUES($1,'ra-fresh-host','ra-key','ra-key',$2)")
        .bind(id(93)).bind(id(92)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO deployment_policies(id,name,policy_type,config,enabled) VALUES($1,'ra-fresh-policy','custom_check','{}',false)")
        .bind(id(94)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO poam_findings(id,system_id,policy_lineage_id) VALUES($1,$2,$3)")
        .bind(id(95))
        .bind(id(93))
        .bind(id(94))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO finding_waivers(id,finding_id,justification,policy_version_id,observation_token,observation_snapshot,created_by) VALUES($1,$2,'initial',$3,'observed','{}',$4)")
        .bind(id(101)).bind(id(95)).bind(Uuid::new_v4()).bind(id(91))
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO finding_waivers(id,finding_id,justification,policy_version_id,observation_token,observation_snapshot,created_by,predecessor_id,predecessor_updated_at,review_due_at) SELECT $1,finding_id,'renewed',policy_version_id,observation_token,observation_snapshot,created_by,id,updated_at,current_date+90 FROM finding_waivers WHERE id=$2")
        .bind(id(102)).bind(id(101)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO cves(id) VALUES('CVE-2099-12345')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO cve_system_dispositions(id,canonical_cve_id,canonical_package_name,system_id,state,justification,accepted_by,accepted_at) VALUES($1,'CVE-2099-12345','openssl',$2,'accepted','host',$3,now())")
        .bind(id(201)).bind(id(93)).bind(id(91)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO cve_environment_dispositions(id,canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) VALUES($1,'CVE-2099-12345','openssl',$2,'accepted','environment',$3,now())")
        .bind(id(301)).bind(id(92)).bind(id(91)).execute(&pool).await.unwrap();
    for (kind, source_id) in [
        ("policy_waiver", id(101)),
        ("policy_waiver", id(102)),
        ("cve_host", id(201)),
        ("cve_environment", id(301)),
    ] {
        assert!(number(&pool, kind, source_id).await.is_some());
    }
    assert_eq!(
        number(&pool, "policy_waiver", id(101)).await,
        number(&pool, "policy_waiver", id(102)).await
    );
    assert_ne!(
        number(&pool, "policy_waiver", id(101)).await,
        number(&pool, "cve_host", id(201)).await
    );
    assert_ne!(
        number(&pool, "cve_host", id(201)).await,
        number(&pool, "cve_environment", id(301)).await
    );
}

/// No two unrelated roots may share a human number after repair.
#[sqlx::test(migrations = false)]
async fn existing_identity_collision_aborts_without_renumbering(pool: PgPool) {
    seed_0296(&pool).await;
    let waiver_number = number(&pool, "policy_waiver", id(101)).await.unwrap();
    sqlx::query(
        "UPDATE risk_acceptance_source_ids SET human_number=$1 WHERE source_kind='cve_host'",
    )
    .bind(waiver_number)
    .execute(&pool)
    .await
    .unwrap();
    let original: Vec<(String,Uuid,i64)> = sqlx::query_as("SELECT source_kind,source_id,human_number FROM risk_acceptance_source_ids ORDER BY source_kind,source_id")
        .fetch_all(&pool).await.unwrap();
    let error = migrate_one(&pool, 298).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Conflicting existing risk acceptance chain numbers"),
        "{error}"
    );
    let after: Vec<(String,Uuid,i64)> = sqlx::query_as("SELECT source_kind,source_id,human_number FROM risk_acceptance_source_ids ORDER BY source_kind,source_id")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(original, after);
}

/// A disabled source trigger cannot silently reopen the identity gap.
#[sqlx::test(migrations = false)]
async fn disabled_insert_trigger_rejects_upgrade(pool: PgPool) {
    seed_0296(&pool).await;
    sqlx::query("ALTER TABLE cve_system_dispositions DISABLE TRIGGER risk_acceptance_host_insert")
        .execute(&pool)
        .await
        .unwrap();
    let error = migrate_one(&pool, 298).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("source insert trigger coverage is incomplete"),
        "{error}"
    );
    sqlx::query("ALTER TABLE cve_system_dispositions ENABLE TRIGGER risk_acceptance_host_insert")
        .execute(&pool)
        .await
        .unwrap();
    migrate_one(&pool, 298).await.unwrap();
}

/// The migration's source-table lock must exclude a second writer's INSERT.
#[sqlx::test(migrations = false)]
async fn repair_lock_mode_blocks_concurrent_source_insert(pool: PgPool) {
    seed_0296(&pool).await;
    let mut migration = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE finding_waivers, cve_system_dispositions, cve_environment_dispositions IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *migration).await.unwrap();
    let writer_pool = pool.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let mut writer = tokio::spawn(async move {
        let mut writer_connection = writer_pool.acquire().await.unwrap();
        started_tx.send(()).unwrap();
        sqlx::query("INSERT INTO cve_system_dispositions(id,canonical_cve_id,canonical_package_name,system_id,state,justification,accepted_by,accepted_at) VALUES($1,'CVE-2099-12345','parallel-source',$2,'accepted','writer',$3,now())")
            .bind(id(207)).bind(id(93)).bind(id(91)).execute(&mut *writer_connection).await
    });
    started_rx.await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(150), &mut writer)
            .await
            .is_err(),
        "a concurrent source INSERT bypassed the repair lock"
    );
    assert_eq!(number(&pool, "cve_host", id(207)).await, None);
    migration.commit().await.unwrap();
    writer.await.unwrap().unwrap();
    assert!(number(&pool, "cve_host", id(207)).await.is_some());
}
