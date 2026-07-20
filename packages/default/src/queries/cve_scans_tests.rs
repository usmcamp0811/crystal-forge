use crate::queries::cve_scans::{
    CreateCveScanOutcome, create_cve_scan, save_scan_results_with_store_path_override,
};
use crate::queries::derivations::insert_derivation;
use crate::vulnix::vulnix_parser::VulnixEntry;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

async fn test_pool_from_env() -> Option<PgPool> {
    let Ok(db_url) = std::env::var("DATABASE_URL") else {
        return None;
    };

    Some(
        PgPool::connect(&db_url)
            .await
            .expect("failed to connect to DATABASE_URL"),
    )
}

#[tokio::test]
async fn save_scan_results_truncates_overlong_package_version() {
    let Some(pool) = test_pool_from_env().await else {
        return;
    };

    let target = insert_derivation(&pool, None, "task-261-cve-truncation-target", "nixos")
        .await
        .expect("should insert target derivation");

    let scan_id = create_cve_scan(&pool, target.id, "vulnix", Some("test".to_string()))
        .await
        .expect("should create cve scan")
        .id();

    let long_version = "a".repeat(140);
    let expected_version: String = long_version.chars().take(100).collect();
    let entry_name = "task-261-overlong-version-package";

    let vulnix_results = vec![VulnixEntry {
        name: entry_name.to_string(),
        pname: "task-261-overlong-version-package".to_string(),
        version: long_version,
        affected_by: vec![],
        whitelisted: vec![],
        derivation: "/nix/store/fakehash-task-261-overlong-version-package.drv".to_string(),
        cvssv3_basescore: HashMap::new(),
    }];

    save_scan_results_with_store_path_override(
        &pool,
        scan_id,
        &vulnix_results,
        Some(123),
        Some("/nix/store/fakehash-task-261-overlong-version-package"),
    )
    .await
    .expect("save_scan_results should succeed for overlong version");

    let stored_version = sqlx::query_scalar::<_, String>(
        "SELECT version FROM derivations WHERE commit_id IS NULL AND derivation_type = 'package' AND derivation_name = $1",
    )
    .bind(entry_name)
    .fetch_one(&pool)
    .await
    .expect("should fetch stored package version");

    assert_eq!(stored_version.len(), 100);
    assert_eq!(stored_version, expected_version);
}

#[tokio::test]
async fn create_cve_scan_reuses_existing_active_scan() {
    let Some(pool) = test_pool_from_env().await else {
        return;
    };
    let derivation_name = format!("task-396-atomic-claim-{}", Uuid::new_v4());

    let target = insert_derivation(&pool, None, &derivation_name, "nixos")
        .await
        .expect("should insert target derivation");

    let first = create_cve_scan(&pool, target.id, "vulnix", Some("test".to_string()))
        .await
        .expect("first claim should succeed");
    let second = create_cve_scan(&pool, target.id, "vulnix", Some("test".to_string()))
        .await
        .expect("second claim should return existing active scan");

    assert!(matches!(first, CreateCveScanOutcome::Created(_)));
    assert!(matches!(second, CreateCveScanOutcome::Existing(_)));
    assert_eq!(first.id(), second.id());

    let active_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM cve_scans WHERE derivation_id = $1 AND status IN ('pending', 'in_progress')",
    )
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("should count active scans");

    assert_eq!(active_count, 1, "only one active scan row should exist");
}
