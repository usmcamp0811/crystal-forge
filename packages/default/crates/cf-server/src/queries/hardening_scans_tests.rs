use crate::api::models::SystemCveInventorySelection;
use crate::queries::commits::{get_commit_by_hash, insert_commit_with_metadata};
use crate::queries::derivations::insert_derivation_for_commit;
use crate::queries::flakes::insert_flake;
use crate::queries::hardening_scans::{
    HardeningAttemptState, SystemHardeningTargetUnavailable, enqueue_hardening_backfill_batch,
    enqueue_post_build_hardening_scan_tx, fetch_system_hardening_inventory, get_fleet_summary,
    parse_system_hardening_selection,
};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Inserts one NixOS derivation with a realized store path for hardening tests.
///
/// Returns the derivation ID. The caller supplies a unique `config_name` so
/// concurrent tests never collide on `(commit_id, derivation_name)`.
async fn insert_hardening_test_derivation(
    pool: &PgPool,
    repo_url: &str,
    commit_hash: &str,
    config_name: &str,
    store_path: &str,
) -> i32 {
    // `insert_commit_with_metadata` requires an existing flake row matching
    // `repo_url`. Reuse a matching flake instead of inserting a duplicate.
    let flake_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM flakes WHERE repo_url=$1)")
            .bind(repo_url)
            .fetch_one(pool)
            .await
            .expect("flake existence probe should succeed");
    if !flake_exists {
        insert_flake(
            pool,
            &format!("flake-{}", Uuid::new_v4().simple()),
            repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("hardening test flake should persist");
    }
    insert_commit_with_metadata(
        pool,
        commit_hash,
        repo_url,
        Utc::now(),
        Some("test"),
        Some("t"),
    )
    .await
    .expect("hardening test commit should persist");
    let commit = get_commit_by_hash(pool, commit_hash)
        .await
        .expect("hardening test commit should load");
    let derivation = insert_derivation_for_commit(pool, &commit, config_name, "nixos")
        .await
        .expect("hardening test derivation should persist");
    sqlx::query("UPDATE derivations SET store_path=$2 WHERE id=$1")
        .bind(derivation.id)
        .bind(store_path)
        .execute(pool)
        .await
        .expect("hardening test derivation store path should persist");
    derivation.id
}

/// Inserts one `build_jobs` row for a derivation in the given terminal status.
///
/// `status` must be `success`, `failed`, or `cancelled`. Returns the job ID.
async fn insert_hardening_test_build_job(pool: &PgPool, derivation_id: i32, status: &str) -> Uuid {
    sqlx::query_scalar(
        r#"INSERT INTO build_jobs(derivation_id,status,completed_at,attempt_number)
           VALUES($1,$2,now(),1) RETURNING id"#,
    )
    .bind(derivation_id)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("hardening test build job should persist")
}

/// Reads the persisted `source_trigger`/`source_build_job_id` pair for a scan.
async fn hardening_scan_provenance(pool: &PgPool, scan_id: Uuid) -> (String, Option<Uuid>) {
    sqlx::query_as("SELECT source_trigger, source_build_job_id FROM hardening_scans WHERE id=$1")
        .bind(scan_id)
        .fetch_one(pool)
        .await
        .expect("hardening scan provenance should load")
}

/// Counts hardening scan rows for a derivation, optionally filtered by status.
async fn hardening_scan_count_for_derivation(
    pool: &PgPool,
    derivation_id: i32,
    status: Option<&str>,
) -> i64 {
    match status {
        Some(status) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM hardening_scans WHERE derivation_id=$1 AND status=$2",
        )
        .bind(derivation_id)
        .bind(status)
        .fetch_one(pool)
        .await
        .expect("hardening scan count should load"),
        None => sqlx::query_scalar("SELECT COUNT(*) FROM hardening_scans WHERE derivation_id=$1")
            .bind(derivation_id)
            .fetch_one(pool)
            .await
            .expect("hardening scan count should load"),
    }
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn post_build_enqueue_admits_one_pending_scan_for_successful_nixos_build(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/post-build-{suffix}.git");
    let derivation_id = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-{suffix}"),
        &format!("config-{suffix}"),
        &format!("/nix/store/{suffix}-system"),
    )
    .await;
    let job_id = insert_hardening_test_build_job(&pool, derivation_id, "success").await;

    let mut tx = pool.begin().await.expect("transaction should begin");
    let admitted = enqueue_post_build_hardening_scan_tx(&mut tx, derivation_id, Some(job_id), true)
        .await
        .expect("post-build enqueue should succeed");
    tx.commit().await.expect("transaction should commit");

    let scan_id = admitted.expect("a scan should be admitted for a successful NixOS build");
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, derivation_id, Some("pending")).await,
        1
    );
    let (trigger, provenance_job) = hardening_scan_provenance(&pool, scan_id).await;
    assert_eq!(trigger, "post_build");
    assert_eq!(provenance_job, Some(job_id));
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn build_provenance_blocks_direct_job_delete_but_allows_derivation_cascade(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/provenance-delete-{suffix}.git");
    let derivation_id = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-{suffix}"),
        &format!("config-{suffix}"),
        &format!("/nix/store/{suffix}-system"),
    )
    .await;
    let job_id = insert_hardening_test_build_job(&pool, derivation_id, "success").await;
    let mut admission = pool
        .begin()
        .await
        .expect("admission transaction should begin");
    enqueue_post_build_hardening_scan_tx(&mut admission, derivation_id, Some(job_id), true)
        .await
        .expect("post-build enqueue should succeed");
    admission
        .commit()
        .await
        .expect("admission transaction should commit");

    let mut direct_delete = pool.begin().await.expect("delete transaction should begin");
    sqlx::query("DELETE FROM build_jobs WHERE id=$1")
        .bind(job_id)
        .execute(&mut *direct_delete)
        .await
        .expect("the deferred constraint checks at commit");
    assert!(
        direct_delete.commit().await.is_err(),
        "a surviving hardening scan must preserve its source build job",
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM build_jobs WHERE id=$1)")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .expect("build-job existence should load"),
    );

    let mut derivation_delete = pool
        .begin()
        .await
        .expect("cascade transaction should begin");
    sqlx::query("DELETE FROM derivations WHERE id=$1")
        .bind(derivation_id)
        .execute(&mut *derivation_delete)
        .await
        .expect("derivation cascade should execute");
    derivation_delete
        .commit()
        .await
        .expect("sibling hardening and build-job cascades should commit");
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, derivation_id, None).await,
        0,
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn post_build_enqueue_duplicate_completion_admits_only_one_row(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/dup-{suffix}.git");
    let derivation_id = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-{suffix}"),
        &format!("config-{suffix}"),
        &format!("/nix/store/{suffix}-system"),
    )
    .await;
    let job_id = insert_hardening_test_build_job(&pool, derivation_id, "success").await;

    for _ in 0..3 {
        let mut tx = pool.begin().await.expect("transaction should begin");
        enqueue_post_build_hardening_scan_tx(&mut tx, derivation_id, Some(job_id), true)
            .await
            .expect("repeated post-build enqueue should not error");
        tx.commit().await.expect("transaction should commit");
    }

    assert_eq!(
        hardening_scan_count_for_derivation(&pool, derivation_id, None).await,
        1,
        "a retried completion callback must not admit a second scan for the same build"
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn post_build_enqueue_rejects_failed_and_cancelled_builds(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/failed-{suffix}.git");
    for (label, status) in [("failed", "failed"), ("cancelled", "cancelled")] {
        let derivation_id = insert_hardening_test_derivation(
            &pool,
            &repo_url,
            &format!("commit-{label}-{suffix}"),
            &format!("config-{label}-{suffix}"),
            &format!("/nix/store/{suffix}-{label}-system"),
        )
        .await;
        let job_id = insert_hardening_test_build_job(&pool, derivation_id, status).await;

        let mut tx = pool.begin().await.expect("transaction should begin");
        let admitted =
            enqueue_post_build_hardening_scan_tx(&mut tx, derivation_id, Some(job_id), true)
                .await
                .expect("enqueue should not error for a non-successful build");
        tx.commit().await.expect("transaction should commit");

        assert!(
            admitted.is_none(),
            "a {label} build must not admit an automatic hardening scan"
        );
        assert_eq!(
            hardening_scan_count_for_derivation(&pool, derivation_id, None).await,
            0
        );
    }
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn post_build_enqueue_rejects_non_nixos_derivation(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/pkg-{suffix}.git");
    insert_flake(
        &pool,
        &format!("pkg-flake-{suffix}"),
        &repo_url,
        "main",
        "cf_systems_only",
    )
    .await
    .expect("package flake should persist");
    insert_commit_with_metadata(
        &pool,
        &format!("commit-{suffix}"),
        &repo_url,
        Utc::now(),
        Some("test"),
        Some("t"),
    )
    .await
    .expect("commit should persist");
    let commit = get_commit_by_hash(&pool, &format!("commit-{suffix}"))
        .await
        .expect("commit should load");
    let derivation =
        insert_derivation_for_commit(&pool, &commit, &format!("pkg-{suffix}"), "package")
            .await
            .expect("package derivation should persist");
    sqlx::query("UPDATE derivations SET store_path=$2 WHERE id=$1")
        .bind(derivation.id)
        .bind(format!("/nix/store/{suffix}-pkg"))
        .execute(&pool)
        .await
        .expect("package store path should persist");
    let job_id = insert_hardening_test_build_job(&pool, derivation.id, "success").await;

    let mut tx = pool.begin().await.expect("transaction should begin");
    let admitted = enqueue_post_build_hardening_scan_tx(&mut tx, derivation.id, Some(job_id), true)
        .await
        .expect("enqueue should not error for a package derivation");
    tx.commit().await.expect("transaction should commit");

    assert!(
        admitted.is_none(),
        "a package build must never admit a hardening scan"
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn post_build_enqueue_respects_disabled_configuration(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/disabled-{suffix}.git");
    let derivation_id = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-{suffix}"),
        &format!("config-{suffix}"),
        &format!("/nix/store/{suffix}-system"),
    )
    .await;
    let job_id = insert_hardening_test_build_job(&pool, derivation_id, "success").await;

    let mut tx = pool.begin().await.expect("transaction should begin");
    let admitted =
        enqueue_post_build_hardening_scan_tx(&mut tx, derivation_id, Some(job_id), false)
            .await
            .expect("enqueue should not error while disabled");
    tx.commit().await.expect("transaction should commit");

    assert!(admitted.is_none());
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, derivation_id, None).await,
        0
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn post_build_enqueue_preserves_existing_active_manual_scan(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/manual-{suffix}.git");
    let derivation_id = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-{suffix}"),
        &format!("config-{suffix}"),
        &format!("/nix/store/{suffix}-system"),
    )
    .await;
    let manual_scan_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO hardening_scans(
             derivation_id,status,attempts,total_services,well_hardened_count,
             moderately_hardened_count,poorly_hardened_count,vulnerable_count,
             source_trigger)
           VALUES($1,'pending',0,0,0,0,0,0,'manual') RETURNING id"#,
    )
    .bind(derivation_id)
    .fetch_one(&pool)
    .await
    .expect("manual scan should persist");
    let job_id = insert_hardening_test_build_job(&pool, derivation_id, "success").await;

    let mut tx = pool.begin().await.expect("transaction should begin");
    let admitted = enqueue_post_build_hardening_scan_tx(&mut tx, derivation_id, Some(job_id), true)
        .await
        .expect("enqueue should not error when a manual scan is already active");
    tx.commit().await.expect("transaction should commit");

    assert!(
        admitted.is_none(),
        "an active manual scan must not be replaced or superseded"
    );
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, derivation_id, None).await,
        1
    );
    let (trigger, _) = hardening_scan_provenance(&pool, manual_scan_id).await;
    assert_eq!(trigger, "manual");
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn post_build_enqueue_concurrent_completion_admits_one_row(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/concurrent-{suffix}.git");
    let derivation_id = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-{suffix}"),
        &format!("config-{suffix}"),
        &format!("/nix/store/{suffix}-system"),
    )
    .await;
    let job_id = insert_hardening_test_build_job(&pool, derivation_id, "success").await;

    let mut handles = Vec::new();
    for _ in 0..8 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            let mut tx = pool.begin().await.expect("transaction should begin");
            let result =
                enqueue_post_build_hardening_scan_tx(&mut tx, derivation_id, Some(job_id), true)
                    .await
                    .expect("concurrent enqueue should not error");
            tx.commit().await.expect("transaction should commit");
            result
        }));
    }
    let mut admitted_count = 0;
    for handle in handles {
        if handle.await.expect("task should not panic").is_some() {
            admitted_count += 1;
        }
    }

    assert_eq!(
        admitted_count, 1,
        "exactly one concurrent completion should win admission"
    );
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, derivation_id, None).await,
        1
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn backfill_batch_admits_only_built_targets_without_evidence(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/backfill-eligible-{suffix}.git");

    let built_uncovered = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-built-{suffix}"),
        &format!("config-built-{suffix}"),
        &format!("/nix/store/{suffix}-built"),
    )
    .await;
    insert_hardening_test_build_job(&pool, built_uncovered, "success").await;

    let evaluated_never_built = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-unbuilt-{suffix}"),
        &format!("config-unbuilt-{suffix}"),
        &format!("/nix/store/{suffix}-unbuilt"),
    )
    .await;
    // No build_jobs row: evaluated but never built.

    let failed_only = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-failedonly-{suffix}"),
        &format!("config-failedonly-{suffix}"),
        &format!("/nix/store/{suffix}-failedonly"),
    )
    .await;
    insert_hardening_test_build_job(&pool, failed_only, "failed").await;

    let admitted = enqueue_hardening_backfill_batch(&pool, 10, true)
        .await
        .expect("backfill batch should succeed");
    assert!(
        admitted >= 1,
        "the built uncovered target should be admitted"
    );

    assert_eq!(
        hardening_scan_count_for_derivation(&pool, built_uncovered, Some("pending")).await,
        1
    );
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, evaluated_never_built, None).await,
        0,
        "an evaluated-but-never-built configuration must never be backfilled"
    );
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, failed_only, None).await,
        0,
        "a derivation whose only build attempts failed must never be backfilled"
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn backfill_batch_is_bounded_by_the_requested_limit(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/backfill-bound-{suffix}.git");
    for index in 0..5 {
        let derivation_id = insert_hardening_test_derivation(
            &pool,
            &repo_url,
            &format!("commit-{index}-{suffix}"),
            &format!("config-{index}-{suffix}"),
            &format!("/nix/store/{suffix}-{index}"),
        )
        .await;
        insert_hardening_test_build_job(&pool, derivation_id, "success").await;
    }

    let admitted = enqueue_hardening_backfill_batch(&pool, 2, true)
        .await
        .expect("bounded backfill batch should succeed");
    assert_eq!(admitted, 2, "one cycle must not exceed its requested limit");
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn backfill_batch_prioritizes_current_deployed_target_over_older_history(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/backfill-priority-{suffix}.git");
    let hostname = format!("backfill-host-{suffix}");
    let config_name = format!("backfill-config-{suffix}");

    let flake = insert_flake(
        &pool,
        &format!("backfill-flake-{suffix}"),
        &repo_url,
        "main",
        "cf_systems_only",
    )
    .await
    .expect("backfill flake should persist");
    sqlx::query(
        r#"INSERT INTO systems(hostname,is_active,public_key,flake_id,derivation,
             system_configuration_name,deployment_policy)
           VALUES($1,true,$2,$3,'',$4,'manual')"#,
    )
    .bind(&hostname)
    .bind("BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=")
    .bind(flake.id)
    .bind(&config_name)
    .execute(&pool)
    .await
    .expect("backfill system should persist");

    let older_hash = format!("older-{suffix}");
    let deployed_hash = format!("deployed-{suffix}");
    insert_commit_with_metadata(
        &pool,
        &older_hash,
        &repo_url,
        Utc::now() - Duration::hours(2),
        Some("older"),
        Some("t"),
    )
    .await
    .expect("older commit should persist");
    insert_commit_with_metadata(
        &pool,
        &deployed_hash,
        &repo_url,
        Utc::now() - Duration::hours(1),
        Some("deployed"),
        Some("t"),
    )
    .await
    .expect("deployed commit should persist");
    let older_commit = get_commit_by_hash(&pool, &older_hash)
        .await
        .expect("older commit should load");
    let deployed_commit = get_commit_by_hash(&pool, &deployed_hash)
        .await
        .expect("deployed commit should load");
    let older_derivation =
        insert_derivation_for_commit(&pool, &older_commit, &config_name, "nixos")
            .await
            .expect("older derivation should persist");
    let deployed_derivation =
        insert_derivation_for_commit(&pool, &deployed_commit, &config_name, "nixos")
            .await
            .expect("deployed derivation should persist");
    let older_store_path = format!("/nix/store/{suffix}-older");
    let deployed_store_path = format!("/nix/store/{suffix}-deployed");
    for (derivation_id, store_path) in [
        (older_derivation.id, &older_store_path),
        (deployed_derivation.id, &deployed_store_path),
    ] {
        sqlx::query("UPDATE derivations SET store_path=$2 WHERE id=$1")
            .bind(derivation_id)
            .bind(store_path)
            .execute(&pool)
            .await
            .expect("store path should persist");
    }
    sqlx::query(
        r#"INSERT INTO system_states(hostname,change_reason,store_path,timestamp)
           VALUES($1,'startup',$2,now())"#,
    )
    .bind(&hostname)
    .bind(&deployed_store_path)
    .execute(&pool)
    .await
    .expect("system state should persist");

    insert_hardening_test_build_job(&pool, older_derivation.id, "success").await;
    insert_hardening_test_build_job(&pool, deployed_derivation.id, "success").await;

    let admitted = enqueue_hardening_backfill_batch(&pool, 1, true)
        .await
        .expect("bounded backfill batch should succeed");
    assert_eq!(admitted, 1);

    assert_eq!(
        hardening_scan_count_for_derivation(&pool, deployed_derivation.id, Some("pending")).await,
        1,
        "the currently deployed exact target must be admitted before older history"
    );
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, older_derivation.id, None).await,
        0
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn backfill_batch_skips_target_with_missing_source_identity(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    // A package-typed derivation stands in for "no source identity": it has no
    // NixOS commit/flake lineage the backfill can use, matching the same
    // exclusion the post-build path enforces.
    let derivation = crate::queries::derivations::insert_package_derivation(
        &pool,
        &format!("pkg-{suffix}"),
        None,
        None,
    )
    .await
    .expect("standalone package derivation should persist");
    sqlx::query("UPDATE derivations SET store_path=$2 WHERE id=$1")
        .bind(derivation.id)
        .bind(format!("/nix/store/{suffix}-pkg"))
        .execute(&pool)
        .await
        .expect("package store path should persist");
    insert_hardening_test_build_job(&pool, derivation.id, "success").await;

    let admitted = enqueue_hardening_backfill_batch(&pool, 10, true)
        .await
        .expect("backfill batch should succeed");
    assert_eq!(admitted, 0);
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, derivation.id, None).await,
        0
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn backfill_batch_is_idempotent_across_cycles(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/backfill-idempotent-{suffix}.git");
    let derivation_id = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-{suffix}"),
        &format!("config-{suffix}"),
        &format!("/nix/store/{suffix}-system"),
    )
    .await;
    insert_hardening_test_build_job(&pool, derivation_id, "success").await;

    let first = enqueue_hardening_backfill_batch(&pool, 10, true)
        .await
        .expect("first backfill cycle should succeed");
    let second = enqueue_hardening_backfill_batch(&pool, 10, true)
        .await
        .expect("second backfill cycle should succeed");

    assert_eq!(first, 1);
    assert_eq!(second, 0, "a second cycle must not admit a duplicate scan");
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, derivation_id, None).await,
        1
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn backfill_batch_respects_disabled_configuration(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/backfill-disabled-{suffix}.git");
    let derivation_id = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-{suffix}"),
        &format!("config-{suffix}"),
        &format!("/nix/store/{suffix}-system"),
    )
    .await;
    insert_hardening_test_build_job(&pool, derivation_id, "success").await;

    let admitted = enqueue_hardening_backfill_batch(&pool, 10, false)
        .await
        .expect("disabled backfill should not error");
    assert_eq!(admitted, 0);
    assert_eq!(
        hardening_scan_count_for_derivation(&pool, derivation_id, None).await,
        0
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn exact_target_lifecycle_reports_never_queued_scanning_failed_and_completed(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/lifecycle-{suffix}.git");
    let hostname = format!("lifecycle-host-{suffix}");
    let config_name = format!("lifecycle-config-{suffix}");
    let flake = insert_flake(
        &pool,
        &format!("lifecycle-flake-{suffix}"),
        &repo_url,
        "main",
        "cf_systems_only",
    )
    .await
    .expect("lifecycle flake should persist");
    let system_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO systems(hostname,is_active,public_key,flake_id,derivation,
             system_configuration_name,deployment_policy)
           VALUES($1,true,$2,$3,'',$4,'manual') RETURNING id"#,
    )
    .bind(&hostname)
    .bind("CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC=")
    .bind(flake.id)
    .bind(&config_name)
    .fetch_one(&pool)
    .await
    .expect("lifecycle system should persist");

    // Case 1: never scanned.
    let derivation_id = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-{suffix}"),
        &config_name,
        &format!("/nix/store/{suffix}-system"),
    )
    .await;
    sqlx::query(
        r#"INSERT INTO system_states(hostname,change_reason,store_path,timestamp)
           VALUES($1,'startup',$2,now())"#,
    )
    .bind(&hostname)
    .bind(format!("/nix/store/{suffix}-system"))
    .execute(&pool)
    .await
    .expect("lifecycle system state should persist");

    let never_scanned =
        fetch_system_hardening_inventory(&pool, system_id, SystemCveInventorySelection::Current)
            .await
            .expect("never-scanned inventory should load");
    assert!(never_scanned.attempt.is_none());
    assert!(never_scanned.source.is_none());

    // Case 2: queued.
    let queued_scan_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO hardening_scans(
             derivation_id,status,attempts,total_services,well_hardened_count,
             moderately_hardened_count,poorly_hardened_count,vulnerable_count,
             source_trigger)
           VALUES($1,'pending',0,0,0,0,0,0,'post_build') RETURNING id"#,
    )
    .bind(derivation_id)
    .fetch_one(&pool)
    .await
    .expect("queued scan should persist");
    let queued =
        fetch_system_hardening_inventory(&pool, system_id, SystemCveInventorySelection::Current)
            .await
            .expect("queued inventory should load");
    let attempt = queued.attempt.expect("a queued attempt should be present");
    assert_eq!(attempt.scan_id, queued_scan_id);
    assert_eq!(attempt.state, HardeningAttemptState::Queued);
    assert!(attempt.state.is_active());

    // Case 3: scanning.
    sqlx::query("UPDATE hardening_scans SET status='in_progress', started_at=now() WHERE id=$1")
        .bind(queued_scan_id)
        .execute(&pool)
        .await
        .expect("scan should transition to in_progress");
    let scanning =
        fetch_system_hardening_inventory(&pool, system_id, SystemCveInventorySelection::Current)
            .await
            .expect("scanning inventory should load");
    assert_eq!(
        scanning
            .attempt
            .expect("a scanning attempt should be present")
            .state,
        HardeningAttemptState::Scanning
    );

    // Case 4: failed. Earlier completed evidence, when present, must survive.
    //
    // `created_at` is set explicitly earlier than `queued_scan_id`'s implicit
    // `created_at` (Case 2, above) so the newest-attempt ordering used by
    // `fetch_latest_hardening_attempt_tx` deterministically prefers the row
    // that was actually admitted later, independent of wall-clock timing
    // between these two statements.
    let completed_scan_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO hardening_scans(
             derivation_id,status,completed_at,created_at,total_services,overall_score,
             source_trigger)
           VALUES($1,'completed',now()-interval '2 hours',now()-interval '2 hours',1,55,
                  'manual') RETURNING id"#,
    )
    .bind(derivation_id)
    .fetch_one(&pool)
    .await
    .expect("earlier completed scan should persist");
    sqlx::query(
        r#"UPDATE hardening_scans
           SET status='failed', completed_at=now(),
               scan_metadata=jsonb_build_object('error','nix eval timed out')
           WHERE id=$1"#,
    )
    .bind(queued_scan_id)
    .execute(&pool)
    .await
    .expect("scan should transition to failed");
    let failed =
        fetch_system_hardening_inventory(&pool, system_id, SystemCveInventorySelection::Current)
            .await
            .expect("failed inventory should load");
    let failed_attempt = failed.attempt.expect("a failed attempt should be present");
    assert_eq!(failed_attempt.scan_id, queued_scan_id);
    assert_eq!(failed_attempt.state, HardeningAttemptState::Failed);
    assert_eq!(failed_attempt.error.as_deref(), Some("nix eval timed out"));
    assert_eq!(
        failed.source.as_ref().map(|scan| scan.id),
        Some(completed_scan_id),
        "a later failed attempt must not discard earlier completed evidence"
    );

    // Case 5: completed. The newest attempt becomes both the lifecycle head and
    // the completed evidence source.
    let newest_completed_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO hardening_scans(
             derivation_id,status,completed_at,total_services,overall_score,
             source_trigger)
           VALUES($1,'completed',now(),1,95,'post_build') RETURNING id"#,
    )
    .bind(derivation_id)
    .fetch_one(&pool)
    .await
    .expect("newest completed scan should persist");
    let completed =
        fetch_system_hardening_inventory(&pool, system_id, SystemCveInventorySelection::Current)
            .await
            .expect("completed inventory should load");
    assert_eq!(
        completed
            .attempt
            .expect("a completed attempt should be present")
            .state,
        HardeningAttemptState::Completed
    );
    assert_eq!(
        completed.source.as_ref().map(|scan| scan.id),
        Some(newest_completed_id)
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn exact_target_lifecycle_never_leaks_evidence_across_revisions(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let repo_url = format!("https://example.com/isolation-{suffix}.git");
    let hostname = format!("isolation-host-{suffix}");
    let config_name = format!("isolation-config-{suffix}");
    let flake = insert_flake(
        &pool,
        &format!("isolation-flake-{suffix}"),
        &repo_url,
        "main",
        "cf_systems_only",
    )
    .await
    .expect("isolation flake should persist");
    let system_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO systems(hostname,is_active,public_key,flake_id,derivation,
             system_configuration_name,deployment_policy)
           VALUES($1,true,$2,$3,'',$4,'manual') RETURNING id"#,
    )
    .bind(&hostname)
    .bind("DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD=")
    .bind(flake.id)
    .bind(&config_name)
    .fetch_one(&pool)
    .await
    .expect("isolation system should persist");

    let revision_a = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-a-{suffix}"),
        &config_name,
        &format!("/nix/store/{suffix}-a"),
    )
    .await;
    let revision_b = insert_hardening_test_derivation(
        &pool,
        &repo_url,
        &format!("commit-b-{suffix}"),
        &config_name,
        &format!("/nix/store/{suffix}-b"),
    )
    .await;

    // Revision A has a failed attempt in progress right now.
    sqlx::query(
        r#"INSERT INTO hardening_scans(
             derivation_id,status,started_at,attempts,total_services,
             well_hardened_count,moderately_hardened_count,poorly_hardened_count,
             vulnerable_count,source_trigger)
           VALUES($1,'in_progress',now(),1,0,0,0,0,0,'post_build')"#,
    )
    .bind(revision_a)
    .execute(&pool)
    .await
    .expect("revision A attempt should persist");

    // Revision B (the current derivation) is deployed now and never scanned.
    sqlx::query(
        r#"INSERT INTO system_states(hostname,change_reason,store_path,timestamp)
           VALUES($1,'startup',$2,now())"#,
    )
    .bind(&hostname)
    .bind(format!("/nix/store/{suffix}-b"))
    .execute(&pool)
    .await
    .expect("current system state should persist");

    let current =
        fetch_system_hardening_inventory(&pool, system_id, SystemCveInventorySelection::Current)
            .await
            .expect("current inventory should load");
    assert_eq!(current.derivation_id, Some(revision_b));
    assert!(
        current.attempt.is_none(),
        "revision B's lifecycle must not show revision A's in-progress attempt"
    );
    assert!(current.source.is_none());

    let historical = fetch_system_hardening_inventory(
        &pool,
        system_id,
        SystemCveInventorySelection::ExactDerivation {
            derivation_id: revision_a,
        },
    )
    .await
    .expect("revision A inventory should load");
    assert_eq!(
        historical
            .attempt
            .expect("revision A should still report its own attempt")
            .state,
        HardeningAttemptState::Scanning
    );
    assert!(historical.read_only);
}

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
