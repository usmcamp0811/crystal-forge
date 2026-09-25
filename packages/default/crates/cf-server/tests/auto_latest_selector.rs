//! Isolated PostgreSQL regression tests for auto-latest artifact selection.
//! Run ignored tests only with a verified disposable DATABASE_URL.

use crystal_forge::queries::derivations::get_latest_deployable_targets_for_flake_hosts;
use crystal_forge::queries::systems::resolve_system_deployment_target;
use sqlx::PgPool;
use uuid::Uuid;

async fn commit(pool: &PgPool, flake: i32, suffix: &str, day: i32) -> (i32, String) {
    let hash = format!("{suffix}-{day}");
    let id = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
         VALUES ($1, $2, '2026-01-01'::timestamptz + $3 * interval '1 day') RETURNING id",
    )
    .bind(flake)
    .bind(&hash)
    .bind(day)
    .fetch_one(pool)
    .await
    .expect("insert commit");
    (id, hash)
}

async fn derivation(
    pool: &PgPool,
    commit: i32,
    config: &str,
    path: Option<&str>,
    agent: bool,
    policy: bool,
    status: i32,
    kind: &str,
) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, \
         attempt_count, store_path, cf_agent_enabled, policy_requirements_met) \
         VALUES ($1, $2, $3, $4, 0, $5, $6, $7) RETURNING id",
    )
    .bind(commit)
    .bind(kind)
    .bind(config)
    .bind(status)
    .bind(path)
    .bind(agent)
    .bind(policy)
    .fetch_one(pool)
    .await
    .expect("insert derivation")
}

async fn cache_job(pool: &PgPool, derivation: i32, status: &str) {
    sqlx::query(
        "INSERT INTO cache_push_jobs (derivation_id, status, store_path) \
         SELECT d.id, $2, d.store_path FROM derivations d WHERE d.id = $1",
    )
    .bind(derivation)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert cache push job");
}

async fn selected(pool: &PgPool, flake: i32, configs: &[String]) -> Vec<(String, i32)> {
    let mut rows: Vec<_> = get_latest_deployable_targets_for_flake_hosts(pool, flake, configs)
        .await
        .expect("select targets")
        .into_iter()
        .map(|target| (target.hostname, target.derivation_id))
        .collect();
    rows.sort();
    rows
}

// SQLx creates a migrated, disposable test database only when explicitly run
// with --ignored and a verified isolated DATABASE_URL.
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified isolated DB"]
async fn selects_newest_deployable_per_effective_config(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let host = format!("host-{suffix}");
    let config = format!("config-{suffix}");
    let other = format!("other-{suffix}");
    let never = format!("never-{suffix}");
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(&suffix)
            .bind(format!("https://example.invalid/{suffix}"))
            .fetch_one(&pool)
            .await
            .expect("insert flake");
    let system_id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems (hostname, public_key, derivation, flake_id, system_configuration_name) \
         VALUES ($1, 'test-key', 'test-derivation', $2, $3) RETURNING id",
    )
    .bind(&host)
    .bind(flake)
    .bind(&config)
    .fetch_one(&pool)
    .await
    .expect("insert system with effective config distinct from hostname");
    let other_system: Uuid = sqlx::query_scalar(
        "INSERT INTO systems (hostname, public_key, derivation, flake_id, system_configuration_name) \
         VALUES ($1, 'other-key', 'test-derivation', $2, $3) RETURNING id",
    )
    .bind(format!("other-host-{suffix}"))
    .bind(flake)
    .bind(&other)
    .fetch_one(&pool)
    .await
    .expect("insert second system sharing the flake");
    let names = vec![config.clone(), other.clone(), never.clone()];
    assert!(selected(&pool, flake, &names).await.is_empty());

    let (a, hash_a) = commit(&pool, flake, &suffix, 0).await;
    let path_a = format!("/nix/store/{suffix}-a");
    let first = derivation(&pool, a, &config, Some(&path_a), true, true, 11, "nixos").await;
    // A nullable derivation_target is not an eligibility failure.
    cache_job(&pool, first, "completed").await;
    assert_eq!(
        selected(&pool, flake, &names).await,
        [(config.clone(), first)]
    );
    assert_eq!(
        resolve_system_deployment_target(&pool, system_id, &hash_a)
            .await
            .unwrap(),
        Some(path_a.clone())
    );

    let (_failed_eval, _) = commit(&pool, flake, &suffix, 1).await;
    assert_eq!(
        selected(&pool, flake, &names).await,
        [(config.clone(), first)]
    );
    let (failed_build, _) = commit(&pool, flake, &suffix, 2).await;
    derivation(&pool, failed_build, &config, None, true, true, 12, "nixos").await;
    let (pending_build, _) = commit(&pool, flake, &suffix, 3).await;
    derivation(&pool, pending_build, &config, None, true, true, 7, "nixos").await;
    let (missing_host, hash_other) = commit(&pool, flake, &suffix, 4).await;
    let path_other = format!("/nix/store/{suffix}-other");
    let other_id = derivation(
        &pool,
        missing_host,
        &other,
        Some(&path_other),
        true,
        true,
        11,
        "nixos",
    )
    .await;
    cache_job(&pool, other_id, "completed").await;
    assert_eq!(
        selected(&pool, flake, &names).await,
        [(config.clone(), first), (other.clone(), other_id)]
    );
    assert_eq!(
        resolve_system_deployment_target(&pool, other_system, &hash_other)
            .await
            .unwrap(),
        Some(path_other)
    );
    assert_eq!(
        resolve_system_deployment_target(&pool, system_id, &hash_other)
            .await
            .unwrap(),
        None
    );

    let (pending_cache, hash_pending) = commit(&pool, flake, &suffix, 5).await;
    let path_next = format!("/nix/store/{suffix}-next");
    let next = derivation(
        &pool,
        pending_cache,
        &config,
        Some(&path_next),
        true,
        true,
        11,
        "nixos",
    )
    .await;
    cache_job(&pool, next, "pending").await;
    cache_job(&pool, next, "failed").await;
    assert_eq!(
        selected(&pool, flake, &names).await,
        [(config.clone(), first), (other.clone(), other_id)]
    );
    assert_eq!(
        resolve_system_deployment_target(&pool, system_id, &hash_pending)
            .await
            .unwrap(),
        None
    );
    cache_job(&pool, next, "completed").await;
    assert_eq!(
        selected(&pool, flake, &names).await,
        [(config.clone(), next), (other.clone(), other_id)]
    );
    assert_eq!(
        resolve_system_deployment_target(&pool, system_id, &hash_pending)
            .await
            .unwrap(),
        Some(path_next)
    );
    assert_eq!(
        resolve_system_deployment_target(&pool, system_id, &hash_a)
            .await
            .unwrap(),
        Some(path_a)
    );

    for (index, (label, path, agent, policy, kind)) in [
        ("policy", Some("/nix/store/policy"), true, false, "nixos"),
        ("agent", Some("/nix/store/agent"), false, true, "nixos"),
        ("blank", Some("   "), true, true, "nixos"),
        ("null", None, true, true, "nixos"),
        (
            "wrong-type",
            Some("/nix/store/wrong-type"),
            true,
            true,
            "package",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let (ineligible, hash_ineligible) = commit(&pool, flake, &suffix, 6 + index as i32).await;
        let id = derivation(&pool, ineligible, &config, path, agent, policy, 11, kind).await;
        cache_job(&pool, id, "completed").await;
        assert_eq!(
            selected(&pool, flake, &names).await,
            [(config.clone(), next), (other.clone(), other_id)],
            "{label}"
        );
        assert_eq!(
            resolve_system_deployment_target(&pool, system_id, &hash_ineligible)
                .await
                .unwrap(),
            None,
            "{label}"
        );
    }

    // Completed_at outranks ID within a tied commit timestamp; ID breaks
    // ties when both completion timestamps are equal (including NULL).
    let (older_commit, _) = commit(&pool, flake, &format!("{suffix}-older"), 5).await;
    let (newer_commit, hash_tied) = commit(&pool, flake, &format!("{suffix}-newer"), 5).await;
    let older = derivation(
        &pool,
        older_commit,
        &config,
        Some("/nix/store/older"),
        true,
        true,
        11,
        "nixos",
    )
    .await;
    let newer = derivation(
        &pool,
        newer_commit,
        &config,
        Some("/nix/store/newer"),
        true,
        true,
        11,
        "nixos",
    )
    .await;
    cache_job(&pool, older, "completed").await;
    cache_job(&pool, newer, "completed").await;
    assert_eq!(
        selected(&pool, flake, &names).await,
        [(config.clone(), newer), (other.clone(), other_id)]
    );
    sqlx::query("UPDATE derivations SET completed_at = '2026-02-01'::timestamptz WHERE id = $1")
        .bind(older)
        .execute(&pool)
        .await
        .expect("set completion time");
    assert_eq!(
        selected(&pool, flake, &names).await,
        [(config.clone(), older), (other.clone(), other_id)]
    );
    sqlx::query("UPDATE derivations SET completed_at = '2026-02-01'::timestamptz WHERE id = $1")
        .bind(newer)
        .execute(&pool)
        .await
        .expect("tie completion times");
    assert_eq!(
        selected(&pool, flake, &names).await,
        [(config.clone(), newer), (other.clone(), other_id)]
    );
    assert_eq!(
        resolve_system_deployment_target(&pool, system_id, &hash_tied)
            .await
            .unwrap(),
        Some("/nix/store/newer".into())
    );
    sqlx::query("UPDATE commits SET source_archived = true WHERE id = $1")
        .bind(newer_commit)
        .execute(&pool)
        .await
        .expect("archive retained commit source");
    assert_eq!(
        selected(&pool, flake, &names).await,
        [(config.clone(), newer), (other.clone(), other_id)],
        "archiving source must not discard the retained cached artifact"
    );
    assert_eq!(
        resolve_system_deployment_target(&pool, system_id, &hash_tied)
            .await
            .unwrap(),
        None,
        "manual explicit commit lookup retains its source-archive restriction"
    );
    assert!(selected(&pool, flake, &[]).await.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified isolated DB"]
async fn mismatched_or_missing_cache_and_derivation_error_cannot_mask_older_build(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let config = format!("sledge-{suffix}");
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(&config)
            .bind(format!("https://example.invalid/{suffix}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    let system: Uuid = sqlx::query_scalar(
        "INSERT INTO systems (hostname, public_key, derivation, flake_id, system_configuration_name) \
         VALUES ($1, 'test-key', 'test-derivation', $2, $1) RETURNING id",
    )
    .bind(&config)
    .bind(flake)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (a, _) = commit(&pool, flake, &suffix, 0).await;
    let (b, hash_b) = commit(&pool, flake, &suffix, 1).await;
    let path_a = format!("/nix/store/{suffix}-a");
    let path_b = format!("/nix/store/{suffix}-b");
    let older = derivation(&pool, a, &config, Some(&path_a), true, true, 11, "nixos").await;
    let newer = derivation(&pool, b, &config, Some(&path_b), true, true, 11, "nixos").await;
    cache_job(&pool, older, "completed").await;
    let new_push: i32 = sqlx::query_scalar(
        "INSERT INTO cache_push_jobs (derivation_id, status, store_path) \
         VALUES ($1, 'completed', NULL) RETURNING id",
    )
    .bind(newer)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO system_states (hostname, store_path, change_reason) VALUES ($1, $2, 'startup')",
    )
    .bind(&config)
    .bind(&path_a)
    .execute(&pool)
    .await
    .unwrap();
    for store_path in [None, Some("/nix/store/mismatched-cache-output")] {
        sqlx::query("UPDATE cache_push_jobs SET store_path = $2 WHERE id = $1")
            .bind(new_push)
            .bind(store_path)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            selected(&pool, flake, std::slice::from_ref(&config)).await,
            [(config.clone(), older)]
        );
        let status: String = sqlx::query_scalar(
            "SELECT deployment_status FROM view_system_deployment_status WHERE hostname = $1",
        )
        .bind(&config)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "up_to_date");
        assert_eq!(
            resolve_system_deployment_target(&pool, system, &hash_b)
                .await
                .unwrap(),
            None
        );
    }
    sqlx::query("UPDATE cache_push_jobs SET store_path = $2 WHERE id = $1")
        .bind(new_push)
        .bind(&path_b)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE derivations SET error_message = 'build error' WHERE id = $1")
        .bind(newer)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        selected(&pool, flake, std::slice::from_ref(&config)).await,
        [(config.clone(), older)]
    );
    let status: String = sqlx::query_scalar(
        "SELECT deployment_status FROM view_system_deployment_status WHERE hostname = $1",
    )
    .bind(&config)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "up_to_date");
    assert_eq!(
        resolve_system_deployment_target(&pool, system, &hash_b)
            .await
            .unwrap(),
        None
    );
    sqlx::query("UPDATE derivations SET error_message = NULL WHERE id = $1")
        .bind(newer)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        selected(&pool, flake, std::slice::from_ref(&config)).await,
        [(config.clone(), newer)]
    );
    assert_eq!(
        resolve_system_deployment_target(&pool, system, &hash_b)
            .await
            .unwrap(),
        Some(path_b.clone())
    );
    let status: String = sqlx::query_scalar(
        "SELECT deployment_status FROM view_system_deployment_status WHERE hostname = $1",
    )
    .bind(&config)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "behind");
    sqlx::query("INSERT INTO system_states (hostname, store_path, change_reason) VALUES ($1, $2, 'config_change')")
        .bind(&config)
        .bind(&path_b)
        .execute(&pool)
        .await
        .unwrap();
    let status: String = sqlx::query_scalar(
        "SELECT deployment_status FROM view_system_deployment_status WHERE hostname = $1",
    )
    .bind(&config)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "up_to_date");
}
