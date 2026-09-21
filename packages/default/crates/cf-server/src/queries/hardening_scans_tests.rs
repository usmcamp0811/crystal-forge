use crate::api::models::SystemCveInventorySelection;
use crate::queries::commits::{get_commit_by_hash, insert_commit_with_metadata};
use crate::queries::derivations::insert_derivation_for_commit;
use crate::queries::flakes::insert_flake;
use crate::queries::hardening_scans::{
    SystemHardeningTargetUnavailable, fetch_system_hardening_inventory, get_fleet_summary,
    parse_system_hardening_selection,
};
use chrono::{Duration, Utc};
use sqlx::PgPool;

async fn test_pool_from_env() -> PgPool {
    let db_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for hardening DB tests");

    PgPool::connect(&db_url)
        .await
        .expect("failed to connect to DATABASE_URL")
}

#[test]
fn hardening_inventory_selection_parser_matches_cve_target_contract() {
    let retained_id = uuid::Uuid::new_v4();
    assert_eq!(
        parse_system_hardening_selection(None, None),
        Ok(SystemCveInventorySelection::Current)
    );
    assert_eq!(
        parse_system_hardening_selection(
            Some("retained_generation"),
            Some(&retained_id.to_string())
        ),
        Ok(SystemCveInventorySelection::RetainedGeneration {
            generation_snapshot_id: retained_id
        })
    );
    assert_eq!(
        parse_system_hardening_selection(Some("exact_derivation"), Some("42")),
        Ok(SystemCveInventorySelection::ExactDerivation { derivation_id: 42 })
    );
}

#[test]
fn hardening_inventory_selection_parser_rejects_ambiguous_targets() {
    assert_eq!(
        parse_system_hardening_selection(Some("current"), Some("1")),
        Err("current target must not include target_id")
    );
    assert_eq!(
        parse_system_hardening_selection(Some("retained_generation"), None),
        Err("historical target requires target_id")
    );
    assert_eq!(
        parse_system_hardening_selection(Some("exact_derivation"), Some("0")),
        Err("target_id must be a positive derivation integer")
    );
    assert_eq!(
        parse_system_hardening_selection(Some("unknown"), None),
        Err("target must be current, retained_generation, or exact_derivation")
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn hardening_inventory_is_exact_target_bound_and_never_falls_back(pool: PgPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let flake_name = format!("hardening-inventory-{suffix}");
    let flake_url = format!("https://example.com/{flake_name}.git");
    let hostname = format!("hardening-host-{suffix}");
    let config_name = format!("hardening-config-{suffix}");
    let flake = insert_flake(&pool, &flake_name, &flake_url, "main", "cf_systems_only")
        .await
        .expect("hardening inventory flake should persist");
    let system_id: uuid::Uuid = sqlx::query_scalar(
        r#"INSERT INTO systems(
             hostname,is_active,public_key,flake_id,derivation,
             system_configuration_name,deployment_policy)
           VALUES($1,true,$2,$3,'',$4,'manual') RETURNING id"#,
    )
    .bind(&hostname)
    .bind("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
    .bind(flake.id)
    .bind(&config_name)
    .fetch_one(&pool)
    .await
    .expect("hardening inventory system should persist");

    let old_hash = format!("hardening-old-{suffix}");
    let current_hash = format!("hardening-current-{suffix}");
    insert_commit_with_metadata(
        &pool,
        &old_hash,
        &flake.repo_url,
        Utc::now() - Duration::hours(2),
        Some("old hardening inventory commit"),
        Some("test"),
    )
    .await
    .expect("old commit should persist");
    insert_commit_with_metadata(
        &pool,
        &current_hash,
        &flake.repo_url,
        Utc::now() - Duration::hours(1),
        Some("current hardening inventory commit"),
        Some("test"),
    )
    .await
    .expect("current commit should persist");
    let old_commit = get_commit_by_hash(&pool, &old_hash)
        .await
        .expect("old commit should load");
    let current_commit = get_commit_by_hash(&pool, &current_hash)
        .await
        .expect("current commit should load");
    let old_derivation = insert_derivation_for_commit(&pool, &old_commit, &config_name, "nixos")
        .await
        .expect("old derivation should persist");
    let current_derivation =
        insert_derivation_for_commit(&pool, &current_commit, &config_name, "nixos")
            .await
            .expect("current derivation should persist");
    let foreign_derivation = insert_derivation_for_commit(
        &pool,
        &current_commit,
        &format!("foreign-config-{suffix}"),
        "nixos",
    )
    .await
    .expect("foreign configuration derivation should persist");
    let old_store_path = format!("/nix/store/{suffix}-old-system");
    let current_store_path = format!("/nix/store/{suffix}-current-system");
    sqlx::query("UPDATE derivations SET store_path=$2 WHERE id=$1")
        .bind(old_derivation.id)
        .bind(&old_store_path)
        .execute(&pool)
        .await
        .expect("old store path should persist");
    sqlx::query("UPDATE derivations SET store_path=$2 WHERE id=$1")
        .bind(current_derivation.id)
        .bind(&current_store_path)
        .execute(&pool)
        .await
        .expect("current store path should persist");

    let old_scan: uuid::Uuid = sqlx::query_scalar(
        r#"INSERT INTO hardening_scans(
             derivation_id,status,completed_at,total_services,overall_score)
           VALUES($1,'completed',now()-interval '1 hour',1,10) RETURNING id"#,
    )
    .bind(old_derivation.id)
    .fetch_one(&pool)
    .await
    .expect("old hardening scan should persist");
    let current_scan: uuid::Uuid = sqlx::query_scalar(
        r#"INSERT INTO hardening_scans(
             derivation_id,status,completed_at,total_services,overall_score)
           VALUES($1,'completed',now(),1,90) RETURNING id"#,
    )
    .bind(current_derivation.id)
    .fetch_one(&pool)
    .await
    .expect("current hardening scan should persist");
    for (scan_id, service_name, score, risk) in [
        (old_scan, "old.service", 10, "vulnerable"),
        (current_scan, "current.service", 90, "well_hardened"),
    ] {
        sqlx::query(
            r#"INSERT INTO service_hardening_results(
                 scan_id,service_name,hardening_score,risk_level,directives_detail)
               VALUES($1,$2,$3,$4,'{}'::jsonb)"#,
        )
        .bind(scan_id)
        .bind(service_name)
        .bind(score)
        .bind(risk)
        .execute(&pool)
        .await
        .expect("hardening service row should persist");
    }
    sqlx::query(
        r#"INSERT INTO system_states(hostname,change_reason,store_path,timestamp)
           VALUES($1,'startup',$2,now()-interval '1 hour'),
                 ($1,'startup',$3,now())"#,
    )
    .bind(&hostname)
    .bind(&old_store_path)
    .bind(&current_store_path)
    .execute(&pool)
    .await
    .expect("hardening system states should persist");

    let current =
        fetch_system_hardening_inventory(&pool, system_id, SystemCveInventorySelection::Current)
            .await
            .expect("current hardening inventory should load");
    assert_eq!(current.derivation_id, Some(current_derivation.id));
    assert_eq!(
        current.source.as_ref().map(|scan| scan.id),
        Some(current_scan)
    );
    assert_eq!(current.services[0].service_name, "current.service");
    assert!(!current.read_only);

    let historical = fetch_system_hardening_inventory(
        &pool,
        system_id,
        SystemCveInventorySelection::ExactDerivation {
            derivation_id: old_derivation.id,
        },
    )
    .await
    .expect("owned historical hardening inventory should load");
    assert_eq!(
        historical.source.as_ref().map(|scan| scan.id),
        Some(old_scan)
    );
    assert_eq!(historical.services[0].service_name, "old.service");
    assert!(historical.read_only);

    let foreign = fetch_system_hardening_inventory(
        &pool,
        system_id,
        SystemCveInventorySelection::ExactDerivation {
            derivation_id: foreign_derivation.id,
        },
    )
    .await
    .expect_err("foreign derivation identity must fail");
    assert!(
        foreign
            .downcast_ref::<SystemHardeningTargetUnavailable>()
            .is_some()
    );

    sqlx::query(
        r#"INSERT INTO system_states(hostname,change_reason,store_path,timestamp)
           VALUES($1,'startup',$2,now()+interval '1 minute')"#,
    )
    .bind(&hostname)
    .bind(format!("/nix/store/{suffix}-unresolved-system"))
    .execute(&pool)
    .await
    .expect("new unresolved state should persist");
    let unresolved =
        fetch_system_hardening_inventory(&pool, system_id, SystemCveInventorySelection::Current)
            .await
            .expect("unresolved current inventory should remain a valid empty response");
    assert!(unresolved.derivation_id.is_none());
    assert!(unresolved.source.is_none());
    assert!(unresolved.services.is_empty());
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

    let flake = insert_flake(&pool, &flake_name, &flake_url, "main", "cf_systems_only")
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

    let old_active_derivation =
        insert_derivation_for_commit(&pool, &old_commit, &active_config, "nixos")
            .await
            .expect("insert old active derivation should succeed");
    let active_derivation =
        insert_derivation_for_commit(&pool, &active_commit, &active_config, "nixos")
            .await
            .expect("insert active derivation should succeed");
    let inactive_derivation =
        insert_derivation_for_commit(&pool, &inactive_commit, &inactive_config, "nixos")
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

    assert_eq!(
        summary.total_systems_scanned - before.total_systems_scanned,
        1
    );
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
    assert_eq!(
        summary.total_services_scanned - before.total_services_scanned,
        4
    );

    let n_before = before.total_systems_scanned as f64;
    let avg_before = before.avg_fleet_score.unwrap_or(0.0);
    let expected_after = if before.total_systems_scanned == 0 {
        80.0
    } else {
        (avg_before * n_before + 80.0) / (n_before + 1.0)
    };
    let observed_after = summary
        .avg_fleet_score
        .expect("avg_fleet_score should be present");
    assert!(
        (observed_after - expected_after).abs() < 1e-6,
        "expected avg_fleet_score {expected_after}, got {observed_after}"
    );
}
