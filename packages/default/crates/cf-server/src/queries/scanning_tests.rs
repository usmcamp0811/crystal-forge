//! Live-database tests for the scanning aggregation queries.
//!
//! These require a migrated dev database and are ignored by default. Run with:
//! `DATABASE_URL=... cargo test -p crystal-forge --lib scanning_tests -- --ignored`

use crate::queries::scanning::{
    ScanSchedulePolicyRow, get_scan_activity, get_scan_deployed, get_scan_queue,
    get_scan_queue_for_system, get_scan_schedule_policy, get_scan_stats, get_scan_systems,
    update_scan_schedule_policy,
};
use sqlx::PgPool;
use uuid::Uuid;

async fn test_pool_from_env() -> PgPool {
    let db_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for scanning tests");
    PgPool::connect(&db_url)
        .await
        .expect("failed to connect to DATABASE_URL")
}

#[tokio::test]
#[ignore = "requires live database connection"]
async fn schedule_policy_round_trips() {
    let pool = test_pool_from_env().await;

    // Capture the current policy so we can restore it afterward.
    let original = get_scan_schedule_policy(&pool)
        .await
        .expect("should read existing policy");

    let updated = ScanSchedulePolicyRow {
        on_build: !original.on_build,
        deployed_interval: "6h".to_string(),
        recent_interval: "12h".to_string(),
        archived_interval: "720h".to_string(),
        archived_enabled: !original.archived_enabled,
        rebuild_to_scan: !original.rebuild_to_scan,
        updated_at: chrono::Utc::now(),
    };

    update_scan_schedule_policy(&pool, &updated)
        .await
        .expect("should update policy");

    let read_back = get_scan_schedule_policy(&pool)
        .await
        .expect("should read updated policy");

    assert_eq!(read_back.on_build, updated.on_build);
    assert_eq!(read_back.deployed_interval, "6h");
    assert_eq!(read_back.recent_interval, "12h");
    assert_eq!(read_back.archived_interval, "720h");
    assert_eq!(read_back.archived_enabled, updated.archived_enabled);
    assert_eq!(read_back.rebuild_to_scan, updated.rebuild_to_scan);

    // Restore the original policy to avoid leaking state across test runs.
    update_scan_schedule_policy(&pool, &original)
        .await
        .expect("should restore original policy");
}

#[tokio::test]
#[ignore = "requires live database connection"]
async fn stats_aggregation_is_internally_consistent() {
    let pool = test_pool_from_env().await;

    let stats = get_scan_stats(&pool).await.expect("stats query should run");

    // All counts are non-negative and coverage is a percentage.
    assert!(stats.scanning >= 0);
    assert!(stats.queued >= 0);
    assert!(stats.stale >= 0);
    assert!(stats.never_scanned >= 0);
    assert!(stats.failed >= 0);
    assert!((0..=100).contains(&stats.coverage_percent));
}

#[tokio::test]
#[ignore = "requires live database connection"]
async fn list_queries_respect_limit_and_run() {
    let pool = test_pool_from_env().await;

    let queue = get_scan_queue(&pool, 5)
        .await
        .expect("queue query should run");
    assert!(queue.len() <= 5);

    let systems = get_scan_systems(&pool, 5)
        .await
        .expect("systems query should run");
    assert!(systems.len() <= 5);

    let activity = get_scan_activity(&pool, 5)
        .await
        .expect("activity query should run");
    assert!(activity.len() <= 5);
}

#[tokio::test]
#[ignore = "requires live database connection"]
async fn deployed_query_runs_against_current_schema() {
    let pool = test_pool_from_env().await;

    let result = get_scan_deployed(&pool, 5, None)
        .await
        .expect("deployed query should run");

    assert!(result.rows.len() <= 5);
    assert!(result.total >= result.rows.len() as i64);
}

async fn insert_never_scanned_system_fixture(pool: &PgPool) -> (Uuid, String, i32) {
    let suffix = Uuid::new_v4().simple().to_string();
    let hostname = format!("never-scanned-{suffix}");
    let system_id = Uuid::new_v4();
    let derivation_id: i32 = sqlx::query_scalar(
        r#"
        INSERT INTO derivations (derivation_type, derivation_name, status_id, attempt_count)
        VALUES ('nixos', $1, 5, 0)
        RETURNING id
        "#,
    )
    .bind(&hostname)
    .fetch_one(pool)
    .await
    .expect("never-scanned derivation should be inserted");

    sqlx::query(
        r#"
        INSERT INTO systems (
            id, hostname, is_active, public_key, derivation,
            system_configuration_name, deployment_policy
        )
        VALUES ($1, $2, TRUE, 'test-key', '', $2, 'manual')
        "#,
    )
    .bind(system_id)
    .bind(&hostname)
    .execute(pool)
    .await
    .expect("never-scanned system should be inserted");

    (system_id, hostname, derivation_id)
}

async fn cleanup_never_scanned_system_fixture(pool: &PgPool, system_id: Uuid, derivation_id: i32) {
    sqlx::query("DELETE FROM systems WHERE id = $1")
        .bind(system_id)
        .execute(pool)
        .await
        .expect("never-scanned system should be deleted");
    sqlx::query("DELETE FROM derivations WHERE id = $1")
        .bind(derivation_id)
        .execute(pool)
        .await
        .expect("never-scanned derivation should be deleted");
}

/// Ensures the fleet queue normalizes absent `cve_scans` values for a derivation.
#[tokio::test]
#[ignore = "requires live database connection"]
async fn scan_queue_normalizes_never_scanned_derivation() {
    let pool = test_pool_from_env().await;
    let (system_id, hostname, derivation_id) = insert_never_scanned_system_fixture(&pool).await;

    let result = get_scan_queue(&pool, 500).await;
    cleanup_never_scanned_system_fixture(&pool, system_id, derivation_id).await;
    let rows = result.expect("queue query should return a never-scanned derivation");
    let row = rows
        .into_iter()
        .find(|row| row.hostname == hostname)
        .expect("queue should include the never-scanned derivation");

    assert_eq!(row.status, "never_scanned");
    assert_eq!(row.critical_count, 0);
    assert_eq!(row.high_count, 0);
    assert_eq!(row.medium_count, 0);
}

/// Ensures the system queue normalizes absent `cve_scans` values for a derivation.
#[tokio::test]
#[ignore = "requires live database connection"]
async fn system_scan_queue_normalizes_never_scanned_derivation() {
    let pool = test_pool_from_env().await;
    let (system_id, hostname, derivation_id) = insert_never_scanned_system_fixture(&pool).await;

    let result = get_scan_queue_for_system(&pool, system_id, 500).await;
    cleanup_never_scanned_system_fixture(&pool, system_id, derivation_id).await;
    let rows = result.expect("system queue query should return a never-scanned derivation");
    let row = rows
        .into_iter()
        .find(|row| row.hostname == hostname)
        .expect("system queue should include the never-scanned derivation");

    assert_eq!(row.status, "never_scanned");
    assert_eq!(row.critical_count, 0);
    assert_eq!(row.high_count, 0);
    assert_eq!(row.medium_count, 0);
}
