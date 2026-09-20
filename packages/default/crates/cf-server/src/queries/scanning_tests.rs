//! Live-database tests for the scanning aggregation queries.
//!
//! These require a migrated dev database and are ignored by default. Run with:
//! `DATABASE_URL=... cargo test -p crystal-forge --lib scanning_tests -- --ignored`

use crate::queries::scanning::{
    ScanSchedulePolicyRow, get_scan_activity, get_scan_deployed, get_scan_queue,
    get_scan_queue_for_system, get_scan_schedule_policy, get_scan_stats, get_scan_systems,
    update_scan_schedule_policy,
};
use futures::FutureExt;
use serial_test::serial;
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
#[serial(scan_schedule_policy)]
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

    let assertions = std::panic::AssertUnwindSafe(async {
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
    })
    .catch_unwind()
    .await;

    // Restore every persisted policy field exactly. The public update helper
    // intentionally advances `updated_at`, so it cannot perform test cleanup.
    sqlx::query(
        r#"
        UPDATE scan_schedule_policy
        SET on_build = $1,
            deployed_interval = $2,
            recent_interval = $3,
            archived_interval = $4,
            archived_enabled = $5,
            rebuild_to_scan = $6,
            updated_at = $7
        WHERE id = 1
        "#,
    )
    .bind(original.on_build)
    .bind(&original.deployed_interval)
    .bind(&original.recent_interval)
    .bind(&original.archived_interval)
    .bind(original.archived_enabled)
    .bind(original.rebuild_to_scan)
    .bind(original.updated_at)
    .execute(&pool)
    .await
    .expect("should restore exact original policy");

    if let Err(panic) = assertions {
        std::panic::resume_unwind(panic);
    }
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

async fn insert_never_scanned_system_fixture(pool: &PgPool) -> (Uuid, String, i32, i32) {
    let suffix = Uuid::new_v4().simple().to_string();
    let hostname = format!("never-scanned-{suffix}");
    let system_id = Uuid::new_v4();
    let flake_id: i32 = sqlx::query_scalar(
        "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
    )
    .bind(format!("never-scanned-{suffix}"))
    .bind(format!("https://example.test/never-scanned-{suffix}.git"))
    .fetch_one(pool)
    .await
    .expect("never-scanned flake should be inserted");
    let commit_id: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, NOW()) RETURNING id",
    )
    .bind(flake_id)
    .bind(&suffix)
    .fetch_one(pool)
    .await
    .expect("never-scanned commit should be inserted");
    let store_path = format!("/nix/store/{suffix}-never-scanned");
    let derivation_id: i32 = sqlx::query_scalar(
        r#"
        INSERT INTO derivations (
            derivation_type, derivation_name, commit_id, store_path, status_id, attempt_count
        )
        VALUES ('nixos', $1, $2, $3, 5, 0)
        RETURNING id
        "#,
    )
    .bind(&hostname)
    .bind(commit_id)
    .bind(&store_path)
    .fetch_one(pool)
    .await
    .expect("never-scanned derivation should be inserted");

    sqlx::query(
        r#"
        INSERT INTO systems (
            id, hostname, is_active, public_key, derivation,
            system_configuration_name, deployment_policy, flake_id
        )
        VALUES ($1, $2, TRUE, 'test-key', '', $2, 'manual', $3)
        "#,
    )
    .bind(system_id)
    .bind(&hostname)
    .bind(flake_id)
    .execute(pool)
    .await
    .expect("never-scanned system should be inserted");
    sqlx::query(
        "INSERT INTO system_states (hostname, store_path, change_reason, timestamp) VALUES ($1, $2, 'config_change', NOW())",
    )
    .bind(&hostname)
    .bind(&store_path)
    .execute(pool)
    .await
    .expect("never-scanned system state should be inserted");

    (system_id, hostname, derivation_id, flake_id)
}

async fn cleanup_never_scanned_system_fixture(
    pool: &PgPool,
    system_id: Uuid,
    hostname: &str,
    derivation_id: i32,
    flake_id: i32,
) {
    sqlx::query("DELETE FROM system_states WHERE hostname = $1")
        .bind(hostname)
        .execute(pool)
        .await
        .expect("never-scanned system state should be deleted");
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
    sqlx::query("DELETE FROM commits WHERE flake_id = $1")
        .bind(flake_id)
        .execute(pool)
        .await
        .expect("never-scanned commit should be deleted");
    sqlx::query("DELETE FROM flakes WHERE id = $1")
        .bind(flake_id)
        .execute(pool)
        .await
        .expect("never-scanned flake should be deleted");
}

async fn insert_waiting_stats_derivation(pool: &PgPool, name: &str) -> i32 {
    sqlx::query_scalar(
        r#"
        INSERT INTO derivations (
            derivation_type, derivation_name, derivation_path, store_path,
            status_id, completed_at, attempt_count
        )
        SELECT 'nixos', $1, $2, $3, id, NOW(), 0
        FROM derivation_statuses
        WHERE name = 'build-complete'
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(format!("/nix/store/{name}.drv"))
    .bind(format!("/nix/store/{name}"))
    .fetch_one(pool)
    .await
    .expect("waiting stats derivation should be inserted")
}

/// Ensures scan statistics include every worker-eligible waiting source and
/// move a persisted request out of that count when its execution starts.
#[tokio::test]
#[ignore = "requires live database connection"]
#[serial(scan_schedule_policy)]
async fn scan_stats_count_worker_eligible_waiting_targets() {
    let pool = test_pool_from_env().await;
    let original_policy = get_scan_schedule_policy(&pool)
        .await
        .expect("should read existing policy");
    let test_policy = ScanSchedulePolicyRow {
        on_build: true,
        deployed_interval: "1h".to_string(),
        recent_interval: "1h".to_string(),
        archived_interval: "720h".to_string(),
        archived_enabled: false,
        rebuild_to_scan: original_policy.rebuild_to_scan,
        updated_at: chrono::Utc::now(),
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let pending_id =
        insert_waiting_stats_derivation(&pool, &format!("waiting-pending-{suffix}")).await;
    let initial_id =
        insert_waiting_stats_derivation(&pool, &format!("waiting-initial-{suffix}")).await;
    let stale_id = insert_waiting_stats_derivation(&pool, &format!("waiting-stale-{suffix}")).await;
    let active_id =
        insert_waiting_stats_derivation(&pool, &format!("waiting-active-{suffix}")).await;
    let failed_id =
        insert_waiting_stats_derivation(&pool, &format!("waiting-failed-{suffix}")).await;

    let assertions = std::panic::AssertUnwindSafe(async {
        update_scan_schedule_policy(&pool, &test_policy)
            .await
            .expect("should set test policy");
        for (derivation_id, status, completed_at) in [
            (pending_id, "pending", None),
            (stale_id, "completed", Some("NOW() - INTERVAL '2 hours'")),
            (active_id, "in_progress", None),
        ] {
            let completed_at_sql = completed_at.unwrap_or("NULL");
            sqlx::query(&format!(
                "INSERT INTO cve_scans (derivation_id, scanner_name, status, completed_at) \
                 VALUES ($1, 'test', $2, {completed_at_sql})"
            ))
            .bind(derivation_id)
            .bind(status)
            .execute(&pool)
            .await
            .expect("scan fixture should be inserted");
        }
        for _ in 0..5 {
            sqlx::query(
                "INSERT INTO cve_scans (derivation_id, scanner_name, status) VALUES ($1, 'test', 'failed')",
            )
            .bind(failed_id)
            .execute(&pool)
            .await
            .expect("failed scan fixture should be inserted");
        }

        let stats = get_scan_stats(&pool).await.expect("should read scan stats");
        assert!(stats.queued >= 3, "pending, initial, and stale targets must wait");
        assert!(stats.scanning >= 1, "in-progress target must scan now");

        sqlx::query("UPDATE cve_scans SET status = 'in_progress' WHERE derivation_id = $1 AND status = 'pending'")
            .bind(pending_id)
            .execute(&pool)
            .await
            .expect("pending scan should start");
        let started = get_scan_stats(&pool).await.expect("should update scan stats");
        assert_eq!(started.queued, stats.queued - 1);
        assert_eq!(started.scanning, stats.scanning + 1);

        sqlx::query("UPDATE cve_scans SET status = 'completed', completed_at = NOW() WHERE derivation_id = $1 AND status = 'in_progress'")
            .bind(pending_id)
            .execute(&pool)
            .await
            .expect("in-progress scan should complete");
        let completed = get_scan_stats(&pool).await.expect("should update scan stats");
        assert_eq!(completed.scanning, stats.scanning);

        // The failed target has five recent failures, and the active target has
        // an active execution. Neither is eligible for the waiting backlog.
        assert!(completed.queued >= 2);
        assert_ne!(initial_id, stale_id);
    })
    .catch_unwind()
    .await;

    for derivation_id in [pending_id, initial_id, stale_id, active_id, failed_id] {
        sqlx::query("DELETE FROM cve_scans WHERE derivation_id = $1")
            .bind(derivation_id)
            .execute(&pool)
            .await
            .expect("scan fixtures should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation_id)
            .execute(&pool)
            .await
            .expect("derivation fixtures should be deleted");
    }
    update_scan_schedule_policy(&pool, &original_policy)
        .await
        .expect("should restore policy");
    assertions.expect("waiting scan assertions should not panic");
}

/// Ensures the fleet queue normalizes absent `cve_scans` values for a derivation.
#[tokio::test]
#[ignore = "requires live database connection"]
async fn scan_queue_normalizes_never_scanned_derivation() {
    let pool = test_pool_from_env().await;
    let (system_id, hostname, derivation_id, flake_id) =
        insert_never_scanned_system_fixture(&pool).await;

    let result = get_scan_queue(&pool, 500).await;
    cleanup_never_scanned_system_fixture(&pool, system_id, &hostname, derivation_id, flake_id)
        .await;
    let rows = result.expect("queue query should return a never-scanned derivation");
    let row = rows
        .into_iter()
        .find(|row| row.hostname == hostname)
        .expect("queue should include the never-scanned derivation");

    assert_eq!(row.status, "never_scanned");
    assert_eq!(row.critical_count, 0);
    assert_eq!(row.high_count, 0);
    assert_eq!(row.medium_count, 0);
    assert!(row.rescan_eligible);
}

/// Ensures the system queue normalizes absent `cve_scans` values for a derivation.
#[tokio::test]
#[ignore = "requires live database connection"]
async fn system_scan_queue_normalizes_never_scanned_derivation() {
    let pool = test_pool_from_env().await;
    let (system_id, hostname, derivation_id, flake_id) =
        insert_never_scanned_system_fixture(&pool).await;

    let result = get_scan_queue_for_system(&pool, system_id, 500).await;
    cleanup_never_scanned_system_fixture(&pool, system_id, &hostname, derivation_id, flake_id)
        .await;
    let rows = result.expect("system queue query should return a never-scanned derivation");
    let row = rows
        .into_iter()
        .find(|row| row.hostname == hostname)
        .expect("system queue should include the never-scanned derivation");

    assert_eq!(row.status, "never_scanned");
    assert_eq!(row.critical_count, 0);
    assert_eq!(row.high_count, 0);
    assert_eq!(row.medium_count, 0);
    assert!(row.is_current);
    assert!(row.rescan_eligible);
}

/// Ensures All scans retains standalone NixOS derivations with nullable commit
/// display data.
#[tokio::test]
#[ignore = "requires live database connection"]
async fn scan_queue_includes_standalone_derivation() {
    let pool = test_pool_from_env().await;
    let suffix = Uuid::new_v4().simple().to_string();
    let hostname = format!("standalone-{suffix}");
    let derivation_id: i32 = sqlx::query_scalar(
        r#"
        INSERT INTO derivations (
            derivation_type, derivation_name, store_path, status_id, attempt_count
        )
        VALUES ('nixos', $1, $2, 5, 0)
        RETURNING id
        "#,
    )
    .bind(&hostname)
    .bind(format!("/nix/store/{suffix}-standalone"))
    .fetch_one(&pool)
    .await
    .expect("standalone derivation should be inserted");
    sqlx::query(
        "INSERT INTO cve_scans (derivation_id, scanner_name, status, source_trigger) VALUES ($1, 'test', 'pending', 'manual')",
    )
    .bind(derivation_id)
    .execute(&pool)
    .await
    .expect("standalone scan should be inserted");

    let rows = get_scan_queue(&pool, 500)
        .await
        .expect("All scans should load standalone derivations");
    let row = rows
        .iter()
        .find(|row| row.derivation_id == derivation_id)
        .expect("standalone derivation should remain visible");

    assert_eq!(row.hostname, hostname);
    assert!(row.flake_name.is_none());
    assert!(row.commit_hash.is_none());
    assert!(row.rescan_eligible);

    sqlx::query("DELETE FROM cve_scans WHERE derivation_id = $1")
        .bind(derivation_id)
        .execute(&pool)
        .await
        .expect("standalone scan should be deleted");
    sqlx::query("DELETE FROM derivations WHERE id = $1")
        .bind(derivation_id)
        .execute(&pool)
        .await
        .expect("standalone derivation should be deleted");
}

/// Ensures system history stays within its flake and marks only the latest
/// reported store path as current.
#[tokio::test]
#[ignore = "requires live database connection"]
async fn system_scan_scope_uses_exact_flake_configuration_and_current_store_path() {
    let pool = test_pool_from_env().await;
    let (system_id, hostname, current_id, flake_id) =
        insert_never_scanned_system_fixture(&pool).await;
    let suffix = Uuid::new_v4().simple().to_string();
    let current_store_path: String =
        sqlx::query_scalar("SELECT store_path FROM derivations WHERE id = $1")
            .bind(current_id)
            .fetch_one(&pool)
            .await
            .expect("current store path should resolve");

    let history_commit_id: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, NOW() - INTERVAL '1 hour') RETURNING id",
    )
    .bind(flake_id)
    .bind(format!("history-{suffix}"))
    .fetch_one(&pool)
    .await
    .expect("history commit should be inserted");
    let history_id: i32 = sqlx::query_scalar(
        r#"
        INSERT INTO derivations (
            derivation_type, derivation_name, commit_id, store_path, status_id, attempt_count
        )
        VALUES ('nixos', $1, $2, $3, 5, 0)
        RETURNING id
        "#,
    )
    .bind(&hostname)
    .bind(history_commit_id)
    .bind(format!("/nix/store/{suffix}-history"))
    .fetch_one(&pool)
    .await
    .expect("history derivation should be inserted");

    let unrelated_flake_id: i32 = sqlx::query_scalar(
        "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
    )
    .bind(format!("unrelated-{suffix}"))
    .bind(format!("https://example.test/unrelated-{suffix}.git"))
    .fetch_one(&pool)
    .await
    .expect("unrelated flake should be inserted");
    let unrelated_commit_id: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, NOW() + INTERVAL '1 hour') RETURNING id",
    )
    .bind(unrelated_flake_id)
    .bind(format!("unrelated-{suffix}"))
    .fetch_one(&pool)
    .await
    .expect("unrelated commit should be inserted");
    let unrelated_id: i32 = sqlx::query_scalar(
        r#"
        INSERT INTO derivations (
            derivation_type, derivation_name, commit_id, store_path, status_id, attempt_count
        )
        VALUES ('nixos', $1, $2, $3, 5, 0)
        RETURNING id
        "#,
    )
    .bind(&hostname)
    .bind(unrelated_commit_id)
    .bind(&current_store_path)
    .fetch_one(&pool)
    .await
    .expect("unrelated derivation should be inserted");
    let unbuilt_commit_id: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, NOW() - INTERVAL '2 hours') RETURNING id",
    )
    .bind(flake_id)
    .bind(format!("unbuilt-{suffix}"))
    .fetch_one(&pool)
    .await
    .expect("unbuilt history commit should be inserted");
    let unbuilt_id: i32 = sqlx::query_scalar(
        r#"
        INSERT INTO derivations (
            derivation_type, derivation_name, commit_id, status_id, attempt_count
        )
        VALUES ('nixos', $1, $2, 5, 0)
        RETURNING id
        "#,
    )
    .bind(&hostname)
    .bind(unbuilt_commit_id)
    .fetch_one(&pool)
    .await
    .expect("unbuilt history derivation should be inserted");

    sqlx::query(
        "INSERT INTO cve_scans (derivation_id, scanner_name, status, source_trigger) VALUES ($1, 'test', 'pending', $2)",
    )
    .bind(current_id)
    .bind("manual")
    .execute(&pool)
    .await
    .expect("current scan should be inserted");
    sqlx::query(
        "INSERT INTO cve_scans (derivation_id, scanner_name, status, source_trigger) VALUES ($1, 'test', 'pending', $2)",
    )
    .bind(history_id)
    .bind("fleet")
    .execute(&pool)
    .await
    .expect("history scan should be inserted");

    let rows = get_scan_queue_for_system(&pool, system_id, 500)
        .await
        .expect("system scan history should load");
    let deployed = get_scan_deployed(&pool, 500, None)
        .await
        .expect("deployed scan rows should load");
    let current = rows
        .iter()
        .find(|row| row.derivation_id == current_id)
        .expect("current derivation should be present");
    let history = rows
        .iter()
        .find(|row| row.derivation_id == history_id)
        .expect("same-flake history should be present");
    let unbuilt = rows
        .iter()
        .find(|row| row.derivation_id == unbuilt_id)
        .expect("unbuilt same-flake history should be present");
    let system = get_scan_systems(&pool, 500)
        .await
        .expect("system summaries should load")
        .into_iter()
        .find(|row| row.system_id == system_id)
        .expect("fixture system summary should be present");

    assert!(current.is_current);
    assert_eq!(current.source_trigger.as_deref(), Some("manual"));
    assert!(!history.is_current);
    assert_eq!(history.source_trigger.as_deref(), Some("fleet"));
    assert!(current.rescan_eligible);
    assert!(history.rescan_eligible);
    assert!(!unbuilt.rescan_eligible);
    assert!(rows.iter().all(|row| row.derivation_id != unrelated_id));
    assert!(
        deployed
            .rows
            .iter()
            .any(|row| row.derivation_id == current_id),
        "the system flake's deployed derivation should be present"
    );
    assert!(
        deployed
            .rows
            .iter()
            .all(|row| row.derivation_id != unrelated_id),
        "an identical config/store path from another flake must be excluded"
    );
    assert_eq!(system.current_derivation_id, Some(current_id));

    sqlx::query("DELETE FROM cve_scans WHERE derivation_id = ANY($1)")
        .bind(&[current_id, history_id][..])
        .execute(&pool)
        .await
        .expect("scan fixtures should be deleted");
    sqlx::query("DELETE FROM derivations WHERE id = ANY($1)")
        .bind(&[history_id, unrelated_id, unbuilt_id][..])
        .execute(&pool)
        .await
        .expect("history derivations should be deleted");
    sqlx::query("DELETE FROM commits WHERE flake_id = $1")
        .bind(unrelated_flake_id)
        .execute(&pool)
        .await
        .expect("unrelated commit should be deleted");
    sqlx::query("DELETE FROM flakes WHERE id = $1")
        .bind(unrelated_flake_id)
        .execute(&pool)
        .await
        .expect("unrelated flake should be deleted");
    cleanup_never_scanned_system_fixture(&pool, system_id, &hostname, current_id, flake_id).await;
}
