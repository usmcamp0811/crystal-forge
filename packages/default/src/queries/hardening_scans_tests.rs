use crate::queries::commits::{get_commit_by_hash, insert_commit_with_metadata};
use crate::queries::derivations::insert_derivation_for_commit;
use crate::queries::flakes::insert_flake;
use crate::queries::hardening_scans::get_fleet_summary;
use chrono::{Duration, Utc};
use sqlx::PgPool;

async fn test_pool_from_env() -> PgPool {
    let db_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for hardening DB tests");

    PgPool::connect(&db_url)
        .await
        .expect("failed to connect to DATABASE_URL")
}

#[tokio::test]
#[ignore = "requires live database connection"]
async fn fleet_summary_uses_latest_completed_scan_per_active_system_only() {
    let pool = test_pool_from_env().await;
    let before = get_fleet_summary(&pool)
        .await
        .expect("get_fleet_summary baseline should succeed");

    let suffix = Utc::now()
        .timestamp_nanos_opt()
        .expect("timestamp_nanos should be available")
        .to_string();

    let flake_name = format!("task-276-hardening-fleet-summary-scope-{suffix}");
    let flake_url = format!("https://example.com/{flake_name}.git");
    let active_host = format!("task-276-active-host-{suffix}");
    let active_config = format!("task-276-active-config-{suffix}");
    let inactive_host = format!("task-276-inactive-host-{suffix}");
    let inactive_config = format!("task-276-inactive-config-{suffix}");
    let old_hash = format!("task276fleetold{suffix}");
    let active_hash = format!("task276fleetactive{suffix}");
    let inactive_hash = format!("task276fleetinactive{suffix}");

    let flake = insert_flake(
        &pool,
        &flake_name,
        &flake_url,
        "main",
        "cf_systems_only",
    )
    .await
    .expect("insert_flake should succeed");

    sqlx::query(
        r#"
        INSERT INTO systems (
            hostname,
            environment_id,
            is_active,
            public_key,
            flake_id,
            derivation,
            system_configuration_name,
            deployment_policy
        ) VALUES
            ($1, NULL, TRUE, $2, $3, '', $4, 'manual'),
            ($5, NULL, FALSE, $6, $3, '', $7, 'manual')
        "#,
    )
    .bind(&active_host)
    .bind("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
    .bind(flake.id)
    .bind(&active_config)
    .bind(&inactive_host)
    .bind("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=")
    .bind(&inactive_config)
    .execute(&pool)
    .await
    .expect("insert systems should succeed");

    let old_ts = Utc::now() - Duration::hours(3);
    let active_ts = Utc::now() - Duration::hours(1);
    let inactive_ts = Utc::now();

    insert_commit_with_metadata(
        &pool,
        &old_hash,
        &flake.repo_url,
        old_ts,
        Some("old active commit"),
        Some("test"),
    )
    .await
    .expect("insert old commit should succeed");
    let old_commit = get_commit_by_hash(&pool, &old_hash)
        .await
        .expect("load old commit should succeed");

    insert_commit_with_metadata(
        &pool,
        &active_hash,
        &flake.repo_url,
        active_ts,
        Some("latest active commit"),
        Some("test"),
    )
    .await
    .expect("insert active commit should succeed");
    let active_commit = get_commit_by_hash(&pool, &active_hash)
        .await
        .expect("load active commit should succeed");

    insert_commit_with_metadata(
        &pool,
        &inactive_hash,
        &flake.repo_url,
        inactive_ts,
        Some("inactive commit"),
        Some("test"),
    )
    .await
    .expect("insert inactive commit should succeed");
    let inactive_commit = get_commit_by_hash(&pool, &inactive_hash)
        .await
        .expect("load inactive commit should succeed");

    let old_active_derivation = insert_derivation_for_commit(&pool, &old_commit, &active_config, "nixos")
        .await
        .expect("insert old active derivation should succeed");
    let active_derivation = insert_derivation_for_commit(&pool, &active_commit, &active_config, "nixos")
        .await
        .expect("insert active derivation should succeed");
    let inactive_derivation = insert_derivation_for_commit(
        &pool,
        &inactive_commit,
        &inactive_config,
        "nixos",
    )
    .await
    .expect("insert inactive derivation should succeed");

    sqlx::query(
        r#"
        INSERT INTO hardening_scans (
            derivation_id,
            status,
            completed_at,
            total_services,
            well_hardened_count,
            moderately_hardened_count,
            poorly_hardened_count,
            vulnerable_count,
            overall_score
        ) VALUES
            ($1, 'completed', $2, 10, 0, 0, 2, 8, 20),
            ($3, 'completed', $4, 4, 2, 1, 1, 0, 80),
            ($5, 'completed', $6, 100, 0, 0, 0, 100, 5)
        "#,
    )
    .bind(old_active_derivation.id)
    .bind(old_ts)
    .bind(active_derivation.id)
    .bind(active_ts)
    .bind(inactive_derivation.id)
    .bind(inactive_ts)
    .execute(&pool)
    .await
    .expect("insert hardening scans should succeed");

    let summary = get_fleet_summary(&pool)
        .await
        .expect("get_fleet_summary should succeed");

    assert_eq!(summary.total_systems_scanned - before.total_systems_scanned, 1);
    assert_eq!(
        summary.total_well_hardened_services - before.total_well_hardened_services,
        2
    );
    assert_eq!(
        summary.total_moderately_hardened_services - before.total_moderately_hardened_services,
        1
    );
    assert_eq!(
        summary.total_poorly_hardened_services - before.total_poorly_hardened_services,
        1
    );
    assert_eq!(
        summary.total_vulnerable_services - before.total_vulnerable_services,
        0
    );
    assert_eq!(summary.total_services_scanned - before.total_services_scanned, 4);

    let n_before = before.total_systems_scanned as f64;
    let avg_before = before.avg_fleet_score.unwrap_or(0.0);
    let expected_after = if before.total_systems_scanned == 0 {
        80.0
    } else {
        (avg_before * n_before + 80.0) / (n_before + 1.0)
    };
    let observed_after = summary.avg_fleet_score.expect("avg_fleet_score should be present");
    assert!(
        (observed_after - expected_after).abs() < 1e-6,
        "expected avg_fleet_score {expected_after}, got {observed_after}"
    );
}
