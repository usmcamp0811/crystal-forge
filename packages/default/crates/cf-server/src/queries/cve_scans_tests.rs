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
async fn derivation_publication_locks_first_seen_cve_before_lower_levels() {
    let Some(pool) = test_pool_from_env().await else {
        return;
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let target = insert_derivation(
        &pool,
        None,
        &format!("first-cve-lock-target-{suffix}"),
        "nixos",
    )
    .await
    .expect("insert lock-order target");
    let cve_id = format!("CVE-2099-{}", &suffix[..8]);
    let mut publisher = pool.begin().await.expect("begin publisher transaction");
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        &mut publisher,
        target.id,
        std::slice::from_ref(&cve_id),
    )
    .await
    .expect("lock first-publication CVE union");

    let mut contender = pool.begin().await.expect("begin lock contender");
    sqlx::query("SET LOCAL lock_timeout='100ms'")
        .execute(&mut *contender)
        .await
        .expect("set bounded lock timeout");
    let error = sqlx::query("SELECT lock_poam_cve_key($1)")
        .bind(&cve_id)
        .execute(&mut *contender)
        .await
        .expect_err("first-seen canonical CVE lock must already be held");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("55P03")
    );
    publisher.rollback().await.expect("release publisher locks");
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
    let cve_id = "CVE-2098-44004";

    let vulnix_results = vec![VulnixEntry {
        name: entry_name.to_string(),
        pname: "task-261-overlong-version-package".to_string(),
        version: long_version.clone(),
        affected_by: vec![cve_id.to_string()],
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

    let observed_version: String = sqlx::query_scalar(
        r#"SELECT observation.observed_package_version
           FROM cve_scan_vulnerability_observations observation
           WHERE observation.scan_id=$1
             AND observation.canonical_cve_id=$2"#,
    )
    .bind(claim.scan_id)
    .bind(cve_id)
    .fetch_one(&pool)
    .await
    .expect("should fetch immutable scanner version evidence");
    assert_eq!(observed_version, long_version);
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
    let cve_id = format!("CVE-2099-{:08}", Uuid::new_v4().as_u128() % 100_000_000);
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
    let pkg_derivation_path = format!("/nix/store/{pkg_name}.drv");
    let pkg_derivation_id: i32 = sqlx::query_scalar(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, pname, version, derivation_path, status_id, attempt_count) \
         VALUES (NULL, 'package', $1, 'test-pkg', '1.0.0', $2, 11, 0) RETURNING id",
    )
    .bind(&pkg_name)
    .bind(&pkg_derivation_path)
    .fetch_one(&pool)
    .await
    .expect("insert package derivation");

    let cve_id = format!("CVE-2099-{:08}", Uuid::new_v4().as_u128() % 100_000_000);
    let mut cvss = HashMap::new();
    cvss.insert(cve_id.clone(), 9.8f32);

    let vulnix_results = vec![VulnixEntry {
        name: pkg_name.clone(),
        pname: "test-pkg".to_string(),
        version: "1.0.0".to_string(),
        affected_by: vec![cve_id.clone()],
        whitelisted: vec![],
        derivation: pkg_derivation_path,
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

/// Proves that version-1 evidence is canonical, occurrence-scoped, immutable,
/// and independent from later compatibility upserts.
#[tokio::test]
async fn save_scan_results_persists_immutable_exact_cve_occurrences() {
    let Some(pool) = test_pool_from_env().await else {
        return;
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let target = insert_derivation(&pool, None, &format!("exact-cve-target-{suffix}"), "nixos")
        .await
        .expect("exact CVE target should be inserted");
    let package_name = format!("exact-cve-package-{suffix}");
    let derivation_path = format!("/nix/store/{suffix}-{package_name}.drv");
    let raw_cve_id = "  cve-2099-0042  ";
    let canonical_cve_id = "CVE-2099-0042";

    let CreateCveScanOutcome::Created(first_claim) =
        create_cve_scan(&pool, target.id, "vulnix", Some("test".to_string()))
            .await
            .expect("first exact CVE scan should be created")
    else {
        panic!("new target should receive an execution claim");
    };
    let legacy_default: i32 =
        sqlx::query_scalar("SELECT evidence_schema_version FROM cve_scans WHERE id = $1")
            .bind(first_claim.scan_id)
            .fetch_one(&pool)
            .await
            .expect("new incomplete scan version should resolve");
    assert_eq!(legacy_default, 0);

    let duplicate_entries = vec![
        VulnixEntry {
            name: package_name.clone(),
            pname: "  openssl  ".to_string(),
            version: "3.0.0".to_string(),
            affected_by: vec![raw_cve_id.to_string(), raw_cve_id.to_string()],
            whitelisted: vec![],
            derivation: derivation_path.clone(),
            cvssv3_basescore: HashMap::from([(raw_cve_id.to_string(), 8.1)]),
        },
        VulnixEntry {
            name: package_name.clone(),
            pname: "openssl".to_string(),
            version: "3.0.0".to_string(),
            affected_by: vec![],
            whitelisted: vec![canonical_cve_id.to_string()],
            derivation: derivation_path.clone(),
            cvssv3_basescore: HashMap::new(),
        },
    ];
    save_scan_results_with_store_path_override(
        &pool,
        first_claim.scan_id,
        &duplicate_entries,
        Some(5),
        Some(&format!("/nix/store/{suffix}-package")),
        first_claim.execution_id,
    )
    .await
    .expect("canonical duplicate observations should persist once");

    let first_observation: (i32, i64, String, String, String, bool) = sqlx::query_as(
        r#"
        SELECT scan.evidence_schema_version,
               COUNT(*) OVER (),
               observation.canonical_cve_id,
               observation.canonical_package_name,
               observation.observed_derivation_path,
               observation.is_whitelisted
        FROM cve_scans scan
        JOIN cve_scan_vulnerability_observations observation
          ON observation.scan_id = scan.id
        WHERE scan.id = $1
        "#,
    )
    .bind(first_claim.scan_id)
    .fetch_one(&pool)
    .await
    .expect("first exact observation should resolve");
    assert_eq!(first_observation.0, 1);
    assert_eq!(first_observation.1, 1, "duplicate occurrence must collapse");
    assert_eq!(first_observation.2, canonical_cve_id);
    assert_eq!(first_observation.3, "openssl");
    assert_eq!(first_observation.4, derivation_path);
    assert!(first_observation.5, "whitelist evidence must win");

    assert!(
        sqlx::query(
            r#"
            INSERT INTO cve_scan_vulnerability_observations (
                scan_id, canonical_cve_id,
                canonical_package_name, observed_package_name,
                observed_package_version, observed_derivation_path,
                is_whitelisted, detection_method
            )
            SELECT scan_id, canonical_cve_id,
                   canonical_package_name, observed_package_name,
                   observed_package_version, observed_derivation_path || '-late',
                   is_whitelisted, detection_method
            FROM cve_scan_vulnerability_observations
            WHERE scan_id = $1
            "#,
        )
        .bind(first_claim.scan_id)
        .execute(&pool)
        .await
        .is_err(),
        "a completed version-1 scan must reject appended occurrences"
    );

    assert!(
        sqlx::query(
            "UPDATE cve_scan_vulnerability_observations SET is_whitelisted = FALSE WHERE scan_id = $1",
        )
        .bind(first_claim.scan_id)
        .execute(&pool)
        .await
        .is_err(),
        "completed scan evidence must reject updates"
    );
    assert!(
        sqlx::query("DELETE FROM cve_scan_vulnerability_observations WHERE scan_id = $1")
            .bind(first_claim.scan_id)
            .execute(&pool)
            .await
            .is_err(),
        "completed scan evidence must reject deletes"
    );

    let CreateCveScanOutcome::Created(second_claim) =
        create_cve_scan(&pool, target.id, "vulnix", Some("test".to_string()))
            .await
            .expect("later exact CVE scan should be created")
    else {
        panic!("completed first scan must release active uniqueness");
    };
    let later_entries = vec![VulnixEntry {
        name: package_name.clone(),
        pname: "openssl".to_string(),
        version: "3.0.1".to_string(),
        affected_by: vec![canonical_cve_id.to_string()],
        whitelisted: vec![],
        derivation: derivation_path.clone(),
        cvssv3_basescore: HashMap::from([(canonical_cve_id.to_string(), 8.1)]),
    }];
    save_scan_results_with_store_path_override(
        &pool,
        second_claim.scan_id,
        &later_entries,
        Some(5),
        Some(&format!("/nix/store/{suffix}-package")),
        second_claim.execution_id,
    )
    .await
    .expect("later exact CVE scan should persist independently");

    let memberships: Vec<(Uuid, String, bool)> = sqlx::query_as(
        r#"
        SELECT scan_id, observed_package_version, is_whitelisted
        FROM cve_scan_vulnerability_observations
        WHERE scan_id = ANY($1)
        ORDER BY scan_id
        "#,
    )
    .bind(vec![first_claim.scan_id, second_claim.scan_id])
    .fetch_all(&pool)
    .await
    .expect("per-scan memberships should resolve");
    assert_eq!(memberships.len(), 2);
    let first = memberships
        .iter()
        .find(|row| row.0 == first_claim.scan_id)
        .expect("first scan membership must remain");
    assert_eq!(first.1, "3.0.0");
    assert!(
        first.2,
        "later compatibility writes must not rewrite history"
    );
    let second = memberships
        .iter()
        .find(|row| row.0 == second_claim.scan_id)
        .expect("second scan membership must exist");
    assert_eq!(second.1, "3.0.1");
    assert!(!second.2);

    let legacy_scan_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO cve_scans (derivation_id, scanner_name, status, completed_at)
        VALUES ($1, 'legacy-test', 'completed', NOW())
        RETURNING id
        "#,
    )
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("legacy-style completed scan should insert");
    let legacy_version: i32 =
        sqlx::query_scalar("SELECT evidence_schema_version FROM cve_scans WHERE id = $1")
            .bind(legacy_scan_id)
            .fetch_one(&pool)
            .await
            .expect("legacy scan version should resolve");
    assert_eq!(legacy_version, 0, "legacy scans must not be inferred");
}

/// Proves validation and owner fencing roll back every provisional output row
/// and leave the active scan unsealed.
#[tokio::test]
async fn exact_cve_writer_errors_leave_version_zero_without_output() {
    let Some(pool) = test_pool_from_env().await else {
        return;
    };
    let suffix = Uuid::new_v4().simple().to_string();

    let malformed_target = insert_derivation(
        &pool,
        None,
        &format!("malformed-exact-cve-target-{suffix}"),
        "nixos",
    )
    .await
    .expect("malformed target should be inserted");
    let CreateCveScanOutcome::Created(malformed_claim) = create_cve_scan(
        &pool,
        malformed_target.id,
        "vulnix",
        Some("test".to_string()),
    )
    .await
    .expect("malformed scan should be created") else {
        panic!("new malformed target should receive a claim");
    };
    let malformed_package = format!("malformed-exact-cve-package-{suffix}");
    let malformed_entries = vec![VulnixEntry {
        name: malformed_package.clone(),
        pname: "  ".to_string(),
        version: String::new(),
        affected_by: vec!["not-a-cve".to_string()],
        whitelisted: vec![],
        derivation: "  ".to_string(),
        cvssv3_basescore: HashMap::new(),
    }];
    assert!(
        save_scan_results_with_store_path_override(
            &pool,
            malformed_claim.scan_id,
            &malformed_entries,
            Some(1),
            Some("/nix/store/not-used"),
            malformed_claim.execution_id,
        )
        .await
        .is_err(),
        "ambiguous scanner identity must be rejected"
    );
    let malformed_state: (String, i32, i64, i64) = sqlx::query_as(
        r#"
        SELECT status, evidence_schema_version,
               (SELECT COUNT(*) FROM scan_packages WHERE scan_id = scan.id),
               (SELECT COUNT(*) FROM cve_scan_vulnerability_observations
                WHERE scan_id = scan.id)
        FROM cve_scans scan WHERE id = $1
        "#,
    )
    .bind(malformed_claim.scan_id)
    .fetch_one(&pool)
    .await
    .expect("malformed scan state should resolve");
    assert_eq!(malformed_state, ("in_progress".to_string(), 0, 0, 0));
    let malformed_package_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM derivations WHERE derivation_name = $1")
            .bind(&malformed_package)
            .fetch_one(&pool)
            .await
            .expect("malformed package output should be countable");
    assert_eq!(malformed_package_count, 0);

    let stale_target = insert_derivation(
        &pool,
        None,
        &format!("stale-exact-cve-target-{suffix}"),
        "nixos",
    )
    .await
    .expect("stale-owner target should be inserted");
    let CreateCveScanOutcome::Created(stale_claim) =
        create_cve_scan(&pool, stale_target.id, "vulnix", Some("test".to_string()))
            .await
            .expect("stale-owner scan should be created")
    else {
        panic!("new stale-owner target should receive a claim");
    };
    let successor_execution_id = Uuid::new_v4();
    sqlx::query(
        r#"
        UPDATE cve_scans
        SET scan_metadata = scan_metadata || jsonb_build_object(
            'execution_id', $2::uuid,
            'execution_heartbeat_at', NOW()
        )
        WHERE id = $1
        "#,
    )
    .bind(stale_claim.scan_id)
    .bind(successor_execution_id)
    .execute(&pool)
    .await
    .expect("test should rotate execution ownership");
    let stale_package = format!("stale-exact-cve-package-{suffix}");
    let stale_cve = "CVE-2099-1042";
    let stale_entries = vec![VulnixEntry {
        name: stale_package.clone(),
        pname: "openssl".to_string(),
        version: "3.0.2".to_string(),
        affected_by: vec![stale_cve.to_string()],
        whitelisted: vec![],
        derivation: format!("/nix/store/{suffix}-{stale_package}.drv"),
        cvssv3_basescore: HashMap::from([(stale_cve.to_string(), 8.2)]),
    }];
    assert!(
        save_scan_results_with_store_path_override(
            &pool,
            stale_claim.scan_id,
            &stale_entries,
            Some(1),
            Some(&format!("/nix/store/{suffix}-stale-package")),
            stale_claim.execution_id,
        )
        .await
        .is_err(),
        "stale owner must fail the final completion CAS"
    );
    let stale_state: (String, i32, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT status, evidence_schema_version,
               (SELECT COUNT(*) FROM scan_packages WHERE scan_id = scan.id),
               (SELECT COUNT(*) FROM cve_scan_vulnerability_observations
                WHERE scan_id = scan.id),
               (SELECT COUNT(*) FROM cves WHERE id = $2)
        FROM cve_scans scan WHERE id = $1
        "#,
    )
    .bind(stale_claim.scan_id)
    .bind(stale_cve)
    .fetch_one(&pool)
    .await
    .expect("stale-owner scan state should resolve");
    assert_eq!(stale_state, ("in_progress".to_string(), 0, 0, 0, 0));
    let stale_package_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM derivations WHERE derivation_name = $1")
            .bind(&stale_package)
            .fetch_one(&pool)
            .await
            .expect("stale package output should be countable");
    assert_eq!(stale_package_count, 0);
}

/// Proves unreferenced sealed evidence remains reclaimable by normal cleanup.
#[tokio::test]
async fn exact_cve_clean_scan_is_sealed_and_reclaimable() {
    let Some(pool) = test_pool_from_env().await else {
        return;
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let target = insert_derivation(
        &pool,
        None,
        &format!("clean-exact-cve-target-{suffix}"),
        "nixos",
    )
    .await
    .expect("clean target should be inserted");
    let CreateCveScanOutcome::Created(claim) =
        create_cve_scan(&pool, target.id, "vulnix", Some("test".to_string()))
            .await
            .expect("clean scan should be created")
    else {
        panic!("new clean target should receive a claim");
    };
    let clean_results = Vec::new();
    save_scan_results_with_store_path_override(
        &pool,
        claim.scan_id,
        &clean_results,
        Some(1),
        None,
        claim.execution_id,
    )
    .await
    .expect("clean scan should seal successfully");
    let state: (String, i32, i64) = sqlx::query_as(
        r#"
        SELECT status, evidence_schema_version,
               (SELECT COUNT(*) FROM cve_scan_vulnerability_observations
                WHERE scan_id = scan.id)
        FROM cve_scans scan WHERE id = $1
        "#,
    )
    .bind(claim.scan_id)
    .fetch_one(&pool)
    .await
    .expect("clean scan state should resolve");
    assert_eq!(state, ("completed".to_string(), 1, 0));
    sqlx::query("DELETE FROM cve_scans WHERE id = $1")
        .bind(claim.scan_id)
        .execute(&pool)
        .await
        .expect("unreferenced sealed scan should be reclaimable");
    sqlx::query("DELETE FROM derivations WHERE id = $1")
        .bind(target.id)
        .execute(&pool)
        .await
        .expect("unreferenced scan source should be reclaimable");
}
