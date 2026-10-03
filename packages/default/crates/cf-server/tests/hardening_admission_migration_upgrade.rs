//! Proves migrations 0274-0276 upgrade a database that already contains
//! historical `hardening_scans` rows, not only a fresh empty database.
//!
//! # Regression history
//!
//! A deployed database with pre-existing `hardening_scans` rows failed to
//! apply migration 0275 with:
//!
//! ```text
//! Error: while executing migration 275
//! new row for relation "hardening_scans" violates check constraint
//! "hardening_scan_source_trigger_check"
//! ```
//!
//! The original 0275 executed `UPDATE hardening_scans SET
//! source_trigger='legacy' ...` before relaxing the 0274 CHECK constraint
//! that only permitted `'manual' | 'post_build' | 'backfill'` at that point.
//! A fresh, empty database has zero matching rows, so the `UPDATE` never hit
//! the constraint and every prior test run against a clean database passed.
//! This file reproduces the populated-database upgrade path that exposed the
//! defect, so migration 0275 must relax the constraint before writing
//! `'legacy'`.

use chrono::Utc;
use crystal_forge::queries::commits::{get_commit_by_hash, insert_commit_with_metadata};
use crystal_forge::queries::derivations::insert_derivation_for_commit;
use crystal_forge::queries::flakes::insert_flake;
use sqlx::{PgPool, migrate::Migrate};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Applies every embedded migration up to and including `version`.
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

/// Applies exactly one embedded migration by version.
///
/// Panics with the underlying SQLx error when that migration fails to apply,
/// which is how this file reproduces and then proves the fix for the
/// production 0275 failure.
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

/// Inserts one NixOS derivation for a fresh flake/commit pair.
///
/// Returns the derivation ID. Callers supply a unique `suffix` so concurrent
/// tests never collide on flake `repo_url` or `(commit_id, derivation_name)`.
async fn insert_upgrade_test_derivation(pool: &PgPool, suffix: &str) -> i32 {
    let repo_url = format!("https://example.com/hardening-upgrade-{suffix}.git");
    insert_flake(
        pool,
        &format!("flake-{suffix}"),
        &repo_url,
        "main",
        "cf_systems_only",
    )
    .await
    .expect("upgrade test flake should persist");
    let commit_hash = format!("commit-{suffix}");
    insert_commit_with_metadata(
        pool,
        &commit_hash,
        &repo_url,
        Utc::now(),
        Some("test"),
        Some("t"),
    )
    .await
    .expect("upgrade test commit should persist");
    let commit = get_commit_by_hash(pool, &commit_hash)
        .await
        .expect("upgrade test commit should load");
    let derivation =
        insert_derivation_for_commit(pool, &commit, &format!("config-{suffix}"), "nixos")
            .await
            .expect("upgrade test derivation should persist");
    derivation.id
}

/// Captures every column this upgrade must preserve unchanged for one scan.
#[derive(Debug, PartialEq, sqlx::FromRow)]
struct PreservedHardeningScan {
    derivation_id: i32,
    scheduled_at: Option<chrono::DateTime<Utc>>,
    started_at: Option<chrono::DateTime<Utc>>,
    completed_at: Option<chrono::DateTime<Utc>>,
    status: String,
    attempts: i32,
    total_services: i32,
    well_hardened_count: i32,
    moderately_hardened_count: i32,
    poorly_hardened_count: i32,
    vulnerable_count: i32,
    overall_score: Option<i32>,
    scan_duration_ms: Option<i32>,
    scan_metadata: Option<serde_json::Value>,
}

async fn read_preserved_scan(pool: &PgPool, scan_id: Uuid) -> PreservedHardeningScan {
    sqlx::query_as(
        r#"SELECT derivation_id, scheduled_at, started_at, completed_at, status, attempts,
                  total_services, well_hardened_count, moderately_hardened_count,
                  poorly_hardened_count, vulnerable_count, overall_score, scan_duration_ms,
                  scan_metadata
           FROM hardening_scans WHERE id=$1"#,
    )
    .bind(scan_id)
    .fetch_one(pool)
    .await
    .expect("preserved hardening scan columns should load")
}

/// Reads the persisted `source_trigger`/`source_build_job_id` pair for a scan.
async fn read_provenance(pool: &PgPool, scan_id: Uuid) -> (String, Option<Uuid>) {
    sqlx::query_as("SELECT source_trigger, source_build_job_id FROM hardening_scans WHERE id=$1")
        .bind(scan_id)
        .fetch_one(pool)
        .await
        .expect("hardening scan provenance should load")
}

/// Reproduces the exact production upgrade path: migrations 0274, 0275, and
/// 0276 applied in order against a database that already contains completed
/// and failed `hardening_scans` rows plus dependent `service_hardening_results`
/// rows, inserted using the pre-0274 schema (no `source_trigger` or
/// `source_build_job_id` columns exist at that point).
///
/// This is the authoritative local reproduction of the production failure. A
/// fresh, empty database cannot expose the defect this test guards against,
/// because the 0275 `UPDATE` that previously violated the 0274 CHECK
/// constraint matches zero rows on an empty table.
#[sqlx::test(migrations = false)]
async fn migration_0274_through_0276_preserves_populated_hardening_scans(pool: PgPool) {
    apply_migrations_through(&pool, 273).await;

    let derivation_id =
        insert_upgrade_test_derivation(&pool, &Uuid::new_v4().simple().to_string()).await;

    // Pre-0274 schema has neither `source_trigger` nor `source_build_job_id`,
    // so this INSERT uses only columns that exist through migration 0273.
    let completed_scan_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO hardening_scans(
               derivation_id, scheduled_at, started_at, completed_at, status,
               attempts, total_services, well_hardened_count,
               moderately_hardened_count, poorly_hardened_count,
               vulnerable_count, overall_score, scan_duration_ms, scan_metadata
           )
           VALUES ($1, now() - interval '2 hours', now() - interval '2 hours',
                   now() - interval '1 hour', 'completed', 1, 3, 2, 1, 0, 0, 87,
                   4200, '{"scanner": "pre-migration-fixture"}'::jsonb)
           RETURNING id"#,
    )
    .bind(derivation_id)
    .fetch_one(&pool)
    .await
    .expect("pre-0274 completed hardening scan fixture should insert");

    let failed_scan_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO hardening_scans(
               derivation_id, scheduled_at, started_at, completed_at, status,
               attempts, total_services, well_hardened_count,
               moderately_hardened_count, poorly_hardened_count,
               vulnerable_count, overall_score, scan_duration_ms, scan_metadata
           )
           VALUES ($1, now() - interval '4 hours', now() - interval '4 hours',
                   now() - interval '3 hours', 'failed', 2, 0, 0, 0, 0, 0, NULL,
                   NULL, '{"error": "pre-migration failure fixture"}'::jsonb)
           RETURNING id"#,
    )
    .bind(derivation_id)
    .fetch_one(&pool)
    .await
    .expect("pre-0274 failed hardening scan fixture should insert");

    let service_result_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO service_hardening_results(
               scan_id, service_name, service_type, hardening_score, risk_level,
               directives_detail, enabled_directives_count,
               disabled_directives_count, missing_directives_count
           )
           VALUES ($1, 'sshd.service', 'simple', 87, 'well_hardened',
                   '{"ProtectSystem": "strict"}'::jsonb, 12, 1, 0)
           RETURNING id"#,
    )
    .bind(completed_scan_id)
    .fetch_one(&pool)
    .await
    .expect("pre-0274 service hardening result fixture should insert");

    let scan_count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM hardening_scans")
        .fetch_one(&pool)
        .await
        .expect("hardening scan count should load before upgrade");
    let result_count_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM service_hardening_results")
            .fetch_one(&pool)
            .await
            .expect("service hardening result count should load before upgrade");
    let completed_before = read_preserved_scan(&pool, completed_scan_id).await;
    let failed_before = read_preserved_scan(&pool, failed_scan_id).await;

    // Step: apply 0274 exactly as committed (unmodified).
    apply_migration(&pool, 274).await;

    // Existing rows predate durable admission metadata, so 0274 must classify
    // them as `manual` with no build provenance.
    assert_eq!(
        read_provenance(&pool, completed_scan_id).await,
        ("manual".to_string(), None)
    );
    assert_eq!(
        read_provenance(&pool, failed_scan_id).await,
        ("manual".to_string(), None)
    );

    // This is the exact statement that the original 0275 ran before it
    // relaxed the constraint. It must fail while the 0274 constraint is still
    // installed, proving the fixture exercises the historical-row path that a
    // fresh database misses.
    let original_order_failure = sqlx::query(
        "UPDATE hardening_scans SET source_trigger='legacy' WHERE source_trigger='manual' AND source_build_job_id IS NULL",
    )
    .execute(&pool)
    .await;
    assert!(
        original_order_failure.is_err(),
        "0274's source-trigger constraint must reject legacy before 0275 relaxes it"
    );

    // Step: apply the corrected 0275. Before the fix, this line panicked with
    // the exact production error because the constraint relaxation ran after
    // the UPDATE. This call is the authoritative proof the fix is correct.
    apply_migration(&pool, 275).await;

    // 0275 must reclassify both pre-existing rows as `legacy`, because their
    // original trigger cannot be proved and `manual` would misreport them as
    // person/API-requested scans.
    assert_eq!(
        read_provenance(&pool, completed_scan_id).await,
        ("legacy".to_string(), None)
    );
    assert_eq!(
        read_provenance(&pool, failed_scan_id).await,
        ("legacy".to_string(), None)
    );

    // Step: apply 0276 (FK deferral); this must not disturb existing rows.
    apply_migration(&pool, 276).await;

    // Lossless-upgrade assertions: identical row counts, no duplication, no
    // loss, and every pre-0274 column value byte-for-byte unchanged.
    let scan_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM hardening_scans")
        .fetch_one(&pool)
        .await
        .expect("hardening scan count should load after upgrade");
    assert_eq!(
        scan_count_after, scan_count_before,
        "the upgrade must not lose or duplicate hardening_scans rows"
    );
    let result_count_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM service_hardening_results")
            .fetch_one(&pool)
            .await
            .expect("service hardening result count should load after upgrade");
    assert_eq!(
        result_count_after, result_count_before,
        "the upgrade must not lose or duplicate service_hardening_results rows"
    );
    assert_eq!(
        read_preserved_scan(&pool, completed_scan_id).await,
        completed_before,
        "the completed scan's pre-existing columns must survive the upgrade unchanged"
    );
    assert_eq!(
        read_preserved_scan(&pool, failed_scan_id).await,
        failed_before,
        "the failed scan's pre-existing columns must survive the upgrade unchanged"
    );
    let preserved_result: (Uuid, String, i32, String, serde_json::Value, i32, i32, i32) =
        sqlx::query_as(
            r#"SELECT scan_id, service_name, hardening_score, risk_level, directives_detail,
                      enabled_directives_count, disabled_directives_count, missing_directives_count
               FROM service_hardening_results WHERE id=$1"#,
        )
        .bind(service_result_id)
        .fetch_one(&pool)
        .await
        .expect("service hardening result row should still exist after the upgrade");
    assert_eq!(
        preserved_result,
        (
            completed_scan_id,
            "sshd.service".to_string(),
            87,
            "well_hardened".to_string(),
            serde_json::json!({ "ProtectSystem": "strict" }),
            12,
            1,
            0,
        ),
        "service_hardening_results must survive the upgrade unchanged"
    );

    // Final constraint must accept every documented trigger value.
    for trigger in ["manual", "legacy", "post_build", "backfill"] {
        // The durable active-scan invariant permits only one pending scan per
        // derivation. A distinct derivation isolates this source-trigger
        // assertion from that unrelated invariant.
        let trigger_derivation_id =
            insert_upgrade_test_derivation(&pool, &Uuid::new_v4().simple().to_string()).await;
        let source_build_job_id: Option<Uuid> = if matches!(trigger, "post_build" | "backfill") {
            let job_id: Uuid = sqlx::query_scalar(
                r#"INSERT INTO build_jobs(derivation_id,status,completed_at,attempt_number)
                       VALUES($1,'success',now(),1) RETURNING id"#,
            )
            .bind(trigger_derivation_id)
            .fetch_one(&pool)
            .await
            .expect("build job fixture should insert");
            Some(job_id)
        } else {
            None
        };
        let inserted = sqlx::query(
            r#"INSERT INTO hardening_scans(derivation_id, status, source_trigger, source_build_job_id)
               VALUES ($1, 'pending', $2, $3)"#,
        )
        .bind(trigger_derivation_id)
        .bind(trigger)
        .bind(source_build_job_id)
        .execute(&pool)
        .await;
        assert!(
            inserted.is_ok(),
            "the final constraint must accept source_trigger={trigger}: {inserted:?}"
        );
    }

    // An unsupported trigger value must still be rejected.
    let invalid_trigger_derivation_id =
        insert_upgrade_test_derivation(&pool, &Uuid::new_v4().simple().to_string()).await;
    let rejected = sqlx::query(
        "INSERT INTO hardening_scans(derivation_id, status, source_trigger) VALUES ($1, 'pending', 'invented')",
    )
    .bind(invalid_trigger_derivation_id)
    .execute(&pool)
    .await;
    assert!(
        rejected.is_err(),
        "an unsupported source_trigger value must be rejected"
    );

    // Both `manual` and `legacy` rows must be unable to carry build provenance.
    for trigger in ["manual", "legacy"] {
        let invalid_provenance_derivation_id =
            insert_upgrade_test_derivation(&pool, &Uuid::new_v4().simple().to_string()).await;
        let job_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO build_jobs(derivation_id,status,completed_at,attempt_number)
               VALUES($1,'success',now(),1) RETURNING id"#,
        )
        .bind(invalid_provenance_derivation_id)
        .fetch_one(&pool)
        .await
        .expect("build job fixture should insert");
        let rejected = sqlx::query(
            r#"INSERT INTO hardening_scans(derivation_id, status, source_trigger, source_build_job_id)
               VALUES ($1, 'pending', $2, $3)"#,
        )
        .bind(invalid_provenance_derivation_id)
        .bind(trigger)
        .bind(job_id)
        .execute(&pool)
        .await;
        assert!(
            rejected.is_err(),
            "source_trigger={trigger} must not be able to carry source_build_job_id"
        );
    }

    // Automatic provenance FK/uniqueness must still function: a second
    // automatic event cannot reuse the same build job.
    let automatic_derivation_id =
        insert_upgrade_test_derivation(&pool, &Uuid::new_v4().simple().to_string()).await;
    let automatic_job_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO build_jobs(derivation_id,status,completed_at,attempt_number)
           VALUES($1,'success',now(),1) RETURNING id"#,
    )
    .bind(automatic_derivation_id)
    .fetch_one(&pool)
    .await
    .expect("automatic build job fixture should insert");
    sqlx::query(
        r#"INSERT INTO hardening_scans(derivation_id, status, source_trigger, source_build_job_id)
           VALUES ($1, 'pending', 'post_build', $2)"#,
    )
    .bind(automatic_derivation_id)
    .bind(automatic_job_id)
    .execute(&pool)
    .await
    .expect("first automatic event for a build job should insert");
    let duplicate_automatic_event = sqlx::query(
        r#"INSERT INTO hardening_scans(derivation_id, status, source_trigger, source_build_job_id)
           VALUES ($1, 'pending', 'backfill', $2)"#,
    )
    .bind(automatic_derivation_id)
    .bind(automatic_job_id)
    .execute(&pool)
    .await;
    assert!(
        duplicate_automatic_event.is_err(),
        "at most one automatic hardening event may reference one build job"
    );

    // Direct removal of a build job remains prohibited while an automatic scan
    // references it. The FK becomes deferred in 0276, not weakened.
    let direct_build_job_delete = sqlx::query("DELETE FROM build_jobs WHERE id=$1")
        .bind(automatic_job_id)
        .execute(&pool)
        .await;
    assert!(
        direct_build_job_delete.is_err(),
        "a referenced build job must not be directly deletable"
    );

    // Deleting the derivation cascades to both sibling tables. The deferred FK
    // permits PostgreSQL to complete those cascades in either internal order.
    sqlx::query("DELETE FROM derivations WHERE id=$1")
        .bind(automatic_derivation_id)
        .execute(&pool)
        .await
        .expect("derivation cascade should preserve deferred FK integrity");
    let remaining_automatic_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM hardening_scans WHERE derivation_id=$1")
            .bind(automatic_derivation_id)
            .fetch_one(&pool)
            .await
            .expect("automatic hardening scan count should load after derivation cascade");
    assert_eq!(
        remaining_automatic_rows, 0,
        "derivation cascade must remove the automatic hardening scan"
    );
}
