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

async fn insert_environment_and_system(pool: &PgPool) -> (Uuid, Uuid, String) {
    let env_id = Uuid::new_v4();
    let system_id = Uuid::new_v4();
    let hostname = format!("host-{}", system_id.simple().to_string()[..12].to_string());

    sqlx::query("INSERT INTO environments (id, name, is_active) VALUES ($1, $2, TRUE)")
        .bind(env_id)
        .bind(format!(
            "env-{}",
            env_id.simple().to_string()[..8].to_string()
        ))
        .execute(pool)
        .await
        .expect("insert environment");

    sqlx::query(
        "INSERT INTO systems (id, hostname, environment_id, is_active, public_key, derivation) \
         VALUES ($1, $2, $3, TRUE, 'test-key', 'test-derivation')",
    )
    .bind(system_id)
    .bind(&hostname)
    .bind(env_id)
    .execute(pool)
    .await
    .expect("insert system");

    (env_id, system_id, hostname)
}

#[tokio::test]
async fn save_scan_results_truncates_overlong_package_version() {
    let Some(pool) = test_pool_from_env().await else {
        return;
    };

    let target = insert_derivation(&pool, None, "task-261-cve-truncation-target", "nixos")
        .await
        .expect("should insert target derivation");

    let claim = create_cve_scan(&pool, target.id, "vulnix", Some("test".to_string()))
        .await
        .expect("should create cve scan");
    let CreateCveScanOutcome::Created(claim) = claim else {
        panic!("test derivation should receive a new execution claim");
    };

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
        claim.scan_id,
        &vulnix_results,
        Some(123),
        Some("/nix/store/fakehash-task-261-overlong-version-package"),
        claim.execution_id,
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

    let CreateCveScanOutcome::Created(first_claim) = first else {
        unreachable!("the first atomic claim was asserted to be newly created");
    };
    let (stored_execution_id, has_started_at, has_heartbeat_at): (Uuid, bool, bool) =
        sqlx::query_as(
            r#"
            SELECT
                (scan_metadata ->> 'execution_id')::uuid,
                scan_metadata ? 'execution_started_at',
                scan_metadata ? 'execution_heartbeat_at'
            FROM cve_scans
            WHERE id = $1
            "#,
        )
        .bind(first_claim.scan_id)
        .fetch_one(&pool)
        .await
        .expect("created scan lease metadata should resolve");
    assert_eq!(stored_execution_id, first_claim.execution_id);
    assert!(has_started_at);
    assert!(has_heartbeat_at);

    let active_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM cve_scans WHERE derivation_id = $1 AND status IN ('pending', 'in_progress')",
    )
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("should count active scans");

    assert_eq!(active_count, 1, "only one active scan row should exist");

    sqlx::query("DELETE FROM cve_scans WHERE derivation_id = $1")
        .bind(target.id)
        .execute(&pool)
        .await
        .expect("active-scan fixture should be deleted");
    sqlx::query("DELETE FROM derivations WHERE id = $1")
        .bind(target.id)
        .execute(&pool)
        .await
        .expect("active-scan derivation should be deleted");
}

/// Ensures duplicate vulnix observations do not send duplicate package/CVE
/// conflict keys to the bulk `package_vulnerabilities` upsert.
#[tokio::test]
async fn save_scan_results_merges_duplicate_package_cve_observations() {
    let Some(pool) = test_pool_from_env().await else {
        return;
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let target = insert_derivation(
        &pool,
        None,
        &format!("duplicate-vulnix-target-{suffix}"),
        "nixos",
    )
    .await
    .expect("target derivation should be inserted");
    let CreateCveScanOutcome::Created(claim) =
        create_cve_scan(&pool, target.id, "vulnix", Some("test".to_string()))
            .await
            .expect("scan should be created")
    else {
        panic!("new target should receive an execution claim");
    };

    let package_name = format!("duplicate-vulnix-package-{suffix}");
    let cve_id = format!("CVE-2099-{}", &suffix[..8]);
    let entries = vec![
        VulnixEntry {
            name: package_name.clone(),
            pname: package_name.clone(),
            version: "1.0.0".to_string(),
            affected_by: vec![cve_id.clone(), cve_id.clone()],
            whitelisted: vec![],
            derivation: format!("/nix/store/{suffix}-{package_name}.drv"),
            cvssv3_basescore: HashMap::from([(cve_id.clone(), 9.8)]),
        },
        VulnixEntry {
            name: package_name.clone(),
            pname: package_name.clone(),
            version: "1.0.0".to_string(),
            affected_by: vec![cve_id.clone()],
            whitelisted: vec![cve_id.clone()],
            derivation: format!("/nix/store/{suffix}-{package_name}.drv"),
            cvssv3_basescore: HashMap::from([(cve_id.clone(), 9.8)]),
        },
    ];

    save_scan_results_with_store_path_override(
        &pool,
        claim.scan_id,
        &entries,
        Some(123),
        Some(&format!("/nix/store/{suffix}-target")),
        claim.execution_id,
    )
    .await
    .expect("duplicate observations must not trigger an upsert cardinality error");

    let (status, row_count, is_whitelisted): (String, i64, bool) = sqlx::query_as(
        r#"
        SELECT
            (SELECT status FROM cve_scans WHERE id = $1),
            COUNT(*),
            BOOL_AND(pv.is_whitelisted)
        FROM package_vulnerabilities pv
        JOIN derivations d ON d.id = pv.derivation_id
        WHERE d.derivation_name = $2 AND pv.cve_id = $3
        "#,
    )
    .bind(claim.scan_id)
    .bind(&package_name)
    .bind(&cve_id)
    .fetch_one(&pool)
    .await
    .expect("merged package/CVE row should resolve");
    assert_eq!(status, "completed");
    assert_eq!(row_count, 1, "one package/CVE conflict key must persist");
    assert!(
        is_whitelisted,
        "whitelist evidence must win when observations disagree"
    );

    let _ = sqlx::query(
        "DELETE FROM attention_occurrences WHERE category = 'cves' AND subject_id = $1",
    )
    .bind(&cve_id)
    .execute(&pool)
    .await;
    let _ = sqlx::query("DELETE FROM package_vulnerabilities WHERE cve_id = $1")
        .bind(&cve_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM cves WHERE id = $1")
        .bind(&cve_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM scan_packages WHERE scan_id = $1")
        .bind(claim.scan_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM cve_scans WHERE id = $1")
        .bind(claim.scan_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM derivations WHERE derivation_name = $1 OR id = $2")
        .bind(&package_name)
        .bind(target.id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn save_scan_results_sets_fleet_relevant_since_atomically_with_cve_attention() {
    // Regression test / crash-boundary test for round 17 issue 2:
    // `save_scan_results` must persist `cves.fleet_relevant_since` and open the
    // CVE attention occurrence inside the same transaction as the scan state
    // transition, so a crash between the scan commit and a separate attention
    // step cannot leave the CVE with a recorded scan but no episode timestamp.
    let Some(pool) = test_pool_from_env().await else {
        return;
    };

    let (_env_id, _system_id, hostname) = insert_environment_and_system(&pool).await;

    // NixOS derivation matching the system hostname, build-complete.
    let nixos_derivation_id: i32 = sqlx::query_scalar(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, attempt_count) \
         VALUES (NULL, 'nixos', $1, 10, 0) RETURNING id",
    )
    .bind(&hostname)
    .fetch_one(&pool)
    .await
    .expect("insert nixos derivation");

    let claim = create_cve_scan(
        &pool,
        nixos_derivation_id,
        "vulnix",
        Some("test".to_string()),
    )
    .await
    .expect("create scan");
    let CreateCveScanOutcome::Created(claim) = claim else {
        panic!("test derivation should receive a new execution claim");
    };

    // Package derivation, build-complete.
    let pkg_name = format!(
        "test-pkg-{}",
        Uuid::new_v4().simple().to_string()[..8].to_string()
    );
    let pkg_derivation_id: i32 = sqlx::query_scalar(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, pname, version, status_id, attempt_count) \
         VALUES (NULL, 'package', $1, 'test-pkg', '1.0.0', 11, 0) RETURNING id",
    )
    .bind(&pkg_name)
    .fetch_one(&pool)
    .await
    .expect("insert package derivation");

    let short = Uuid::new_v4().simple().to_string();
    let cve_id = format!("CVE-{}-{}", &short[..4], &short[4..8]);
    let mut cvss = HashMap::new();
    cvss.insert(cve_id.clone(), 9.8f32);

    let vulnix_results = vec![VulnixEntry {
        name: pkg_name.clone(),
        pname: "test-pkg".to_string(),
        version: "1.0.0".to_string(),
        affected_by: vec![cve_id.clone()],
        whitelisted: vec![],
        derivation: format!("/nix/store/{pkg_name}.drv"),
        cvssv3_basescore: cvss,
    }];

    save_scan_results_with_store_path_override(
        &pool,
        claim.scan_id,
        &vulnix_results,
        Some(123),
        Some("/nix/store/fake-atomic-path"),
        claim.execution_id,
    )
    .await
    .expect("save_scan_results should succeed");

    let fleet_relevant_since: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT fleet_relevant_since FROM cves WHERE id = $1")
            .bind(&cve_id)
            .fetch_one(&pool)
            .await
            .expect("fetch cves.fleet_relevant_since");
    assert!(
        fleet_relevant_since.is_some(),
        "fleet_relevant_since must be set atomically with the scan results"
    );

    let open_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM attention_occurrences \
         WHERE category = 'cves' AND subject_id = $1 AND resolved_at IS NULL",
    )
    .bind(&cve_id)
    .fetch_one(&pool)
    .await
    .expect("count CVE attention occurrences");
    assert_eq!(
        open_count, 1,
        "exactly one open CVE attention occurrence must exist after the scan"
    );

    // Cleanup.
    let _ = sqlx::query(
        "DELETE FROM attention_occurrences WHERE category = 'cves' AND subject_id = $1",
    )
    .bind(&cve_id)
    .execute(&pool)
    .await;
    let _ = sqlx::query("DELETE FROM package_vulnerabilities WHERE cve_id = $1")
        .bind(&cve_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM cves WHERE id = $1")
        .bind(&cve_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM scan_packages WHERE scan_id = $1")
        .bind(claim.scan_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM cve_scans WHERE id = $1")
        .bind(claim.scan_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM derivations WHERE id = ANY($1)")
        .bind(vec![nixos_derivation_id, pkg_derivation_id])
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM systems WHERE hostname = $1")
        .bind(&hostname)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM environments WHERE name LIKE 'env-%'")
        .execute(&pool)
        .await;
}
