//! Isolated database regression for the deployment status view contract.
//! Run the ignored test only against a verified disposable PostgreSQL cluster.

use crystal_forge::queries::systems::{get_system_detail_by_id, list_systems_from_view};
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn status(pool: &PgPool, hostname: &str) -> (String, i64, Option<String>) {
    sqlx::query_as(
        "SELECT deployment_status, commits_behind, latest_commit_hash \
         FROM view_system_deployment_status WHERE hostname = $1",
    )
    .bind(hostname)
    .fetch_one(pool)
    .await
    .expect("deployment status")
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires an explicitly verified disposable DATABASE_URL"]
async fn running_path_prefers_eligible_identity_over_newer_failed_record(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let host = format!("running-{suffix}");
    let config = format!("config-{suffix}");
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(&host)
            .bind(format!("https://example.invalid/{suffix}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO systems (hostname, public_key, derivation, flake_id, system_configuration_name) \
         VALUES ($1, 'test-key', 'test-derivation', $2, $3)",
    )
    .bind(&host)
    .bind(flake)
    .bind(&config)
    .execute(&pool)
    .await
    .unwrap();
    let path_p = format!("/nix/store/{suffix}-p");
    let path_q = format!("/nix/store/{suffix}-q");
    for (revision, date, path, error) in [
        ("a", "2026-01-01", &path_p, None),
        ("b", "2026-01-02", &path_q, None),
        ("c", "2026-01-03", &path_p, Some("build failed")),
    ] {
        let commit: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
             VALUES ($1, $2, $3::date) RETURNING id",
        )
        .bind(flake)
        .bind(format!("{revision}-{suffix}"))
        .bind(date)
        .fetch_one(&pool)
        .await
        .unwrap();
        let derivation: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, \
             attempt_count, store_path, cf_agent_enabled, policy_requirements_met, error_message) \
             VALUES ($1, 'nixos', $2, 11, 0, $3, true, true, $4) RETURNING id",
        )
        .bind(commit)
        .bind(&config)
        .bind(path)
        .bind(error)
        .fetch_one(&pool)
        .await
        .unwrap();
        if error.is_none() {
            sqlx::query(
                "INSERT INTO cache_push_jobs (derivation_id, status, store_path) \
                 VALUES ($1, 'completed', $2)",
            )
            .bind(derivation)
            .bind(path)
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    sqlx::query(
        "INSERT INTO system_states (hostname, change_reason, store_path) \
         VALUES ($1, 'startup', $2)",
    )
    .bind(&host)
    .bind(&path_p)
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        status(&pool, &host).await,
        ("behind".into(), 1, Some(format!("b-{suffix}")))
    );
    let row = sqlx::query(
        "SELECT d.current_commit_hash, d.current_store_path, d.latest_commit_hash, \
         d.deployment_status AS view_status, l.deployment_status AS list_status, \
         t.deployment_status AS detail_status FROM view_system_deployment_status d \
         JOIN view_system_list l USING (hostname) JOIN view_system_detail t USING (hostname) \
         WHERE d.hostname = $1",
    )
    .bind(&host)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(row.len(), 1);
    assert_eq!(
        row[0].get::<String, _>("current_commit_hash"),
        format!("a-{suffix}")
    );
    assert_eq!(row[0].get::<String, _>("current_store_path"), path_p);
    assert_eq!(
        row[0].get::<String, _>("latest_commit_hash"),
        format!("b-{suffix}")
    );
    assert_eq!(row[0].get::<String, _>("view_status"), "behind");
    assert_eq!(row[0].get::<String, _>("list_status"), "behind");
    assert_eq!(row[0].get::<String, _>("detail_status"), "behind");
}

// An isolated SQLx test database is required; the package's default tests run
// without DATABASE_URL and must never create a database implicitly.
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires an explicitly verified disposable DATABASE_URL"]
async fn sledge_status_tracks_newest_deployable_build(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let host = format!("status-{suffix}");
    let config = format!("sledge-{suffix}");
    let other = format!("other-{suffix}");
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(&host)
            .bind(format!("https://example.invalid/{suffix}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    for (name, config_name) in [(&host, &config), (&other, &other)] {
        sqlx::query(
            "INSERT INTO systems (hostname, public_key, derivation, flake_id, system_configuration_name) \
             VALUES ($1, 'test-key', 'test-derivation', $2, $3)",
        )
        .bind(name)
        .bind(flake)
        .bind(config_name)
        .execute(&pool)
        .await
        .unwrap();
    }
    let a: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
         VALUES ($1, $2, '2026-01-01'::timestamptz) RETURNING id",
    )
    .bind(flake)
    .bind(format!("a-{suffix}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let b: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
         VALUES ($1, $2, '2026-01-02'::timestamptz) RETURNING id",
    )
    .bind(flake)
    .bind(format!("b-{suffix}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let c: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
         VALUES ($1, $2, '2026-01-03'::timestamptz) RETURNING id",
    )
    .bind(flake)
    .bind(format!("c-{suffix}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let path_a = format!("/nix/store/{suffix}-a");
    let path_b = format!("/nix/store/{suffix}-b");
    let a_derivation: i32 = sqlx::query_scalar(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, \
         attempt_count, store_path, cf_agent_enabled, policy_requirements_met) \
         VALUES ($1, 'nixos', $2, 11, 0, $3, true, true) RETURNING id",
    )
    .bind(a)
    .bind(&config)
    .bind(&path_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    let b_derivation: i32 = sqlx::query_scalar(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, \
         attempt_count, store_path, cf_agent_enabled, policy_requirements_met) \
         VALUES ($1, 'nixos', $2, 11, 0, $3, true, true) RETURNING id",
    )
    .bind(b)
    .bind(&config)
    .bind(&path_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO cache_push_jobs (derivation_id, status, store_path) VALUES ($1, 'completed', $2)")
        .bind(a_derivation)
        .bind(&path_a)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO system_states (hostname, change_reason, store_path, timestamp) \
         VALUES ($1, 'startup', $2, '2026-01-04'::timestamptz)",
    )
    .bind(&host)
    .bind(&path_a)
    .execute(&pool)
    .await
    .unwrap();

    // B has a built path but is cache-pending; raw HEAD C has no host build.
    assert_eq!(
        status(&pool, &host).await,
        ("up_to_date".into(), 0, Some(format!("a-{suffix}")))
    );
    assert_eq!(status(&pool, &other).await.0, "no_deployment");
    sqlx::query("INSERT INTO cache_push_jobs (derivation_id, status, store_path) VALUES ($1, 'completed', $2)")
        .bind(b_derivation)
        .bind(&path_b)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        status(&pool, &host).await,
        ("behind".into(), 1, Some(format!("b-{suffix}")))
    );
    let description: String = sqlx::query_scalar(
        "SELECT status_description FROM view_system_deployment_status WHERE hostname = $1",
    )
    .bind(&host)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!description.contains("3 commit"));

    // At the same timestamp, the larger state ID wins; duplicate paths must
    // not multiply the shared deployment row or the list/detail projections.
    sqlx::query(
        "INSERT INTO system_states (hostname, change_reason, store_path, timestamp) \
         VALUES ($1, 'startup', $2, '2026-01-04'::timestamptz)",
    )
    .bind(&host)
    .bind(&path_b)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(status(&pool, &host).await.0, "up_to_date");
    sqlx::query("INSERT INTO system_states (hostname, change_reason, store_path) VALUES ($1, 'startup', $2)")
        .bind(&other)
        .bind(format!("/nix/store/{suffix}-unmapped"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(status(&pool, &other).await.0, "unknown");

    // A second commit with the same timestamp wins the derivation ID tie.
    // It is behind until installed, but has no later timestamp to count.
    let path_b2 = format!("/nix/store/{suffix}-b2");
    let tied_commit: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
         VALUES ($1, $2, '2026-01-02'::timestamptz) RETURNING id",
    )
    .bind(flake)
    .bind(format!("d-{suffix}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let b2: i32 = sqlx::query_scalar(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, \
         attempt_count, store_path, cf_agent_enabled, policy_requirements_met) \
         VALUES ($1, 'nixos', $2, 11, 0, $3, true, true) RETURNING id",
    )
    .bind(tied_commit)
    .bind(&config)
    .bind(&path_b2)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO cache_push_jobs (derivation_id, status, store_path) VALUES ($1, 'completed', $2)")
        .bind(b2)
        .bind(&path_b2)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        status(&pool, &host).await,
        ("behind".into(), 0, Some(format!("d-{suffix}")))
    );
    sqlx::query("INSERT INTO system_states (hostname, change_reason, store_path) VALUES ($1, 'startup', $2)")
        .bind(&host)
        .bind(&path_b2)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(status(&pool, &host).await.0, "up_to_date");

    let path_c = format!("/nix/store/{suffix}-c-unpublished");
    sqlx::query("UPDATE commits SET source_archived = true WHERE id = $1")
        .bind(tied_commit)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        status(&pool, &host).await.0,
        "up_to_date",
        "archiving source cannot invalidate the running cached artifact"
    );
    sqlx::query(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, \
         attempt_count, store_path, cf_agent_enabled, policy_requirements_met) \
         VALUES ($1, 'nixos', $2, 11, 0, $3, true, false)",
    )
    .bind(c)
    .bind(&config)
    .bind(&path_c)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(status(&pool, &host).await.0, "up_to_date");
    sqlx::query("INSERT INTO system_states (hostname, change_reason, store_path) VALUES ($1, 'startup', $2)")
        .bind(&host)
        .bind(&path_c)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(status(&pool, &host).await.0, "ahead");

    let row = sqlx::query(
        "SELECT d.deployment_status AS view_status, l.deployment_status AS list_status, \
         t.deployment_status AS detail_status FROM view_system_deployment_status d \
         JOIN view_system_list l USING (hostname) JOIN view_system_detail t USING (hostname) \
         WHERE d.hostname = $1",
    )
    .bind(&host)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(row.len(), 1);
    assert_eq!(
        row[0].get::<String, _>("view_status"),
        row[0].get::<String, _>("list_status")
    );
    assert_eq!(
        row[0].get::<String, _>("view_status"),
        row[0].get::<String, _>("detail_status")
    );
    let system_id: Uuid = sqlx::query_scalar("SELECT id FROM systems WHERE hostname = $1")
        .bind(&host)
        .fetch_one(&pool)
        .await
        .unwrap();
    let detail = get_system_detail_by_id(&pool, system_id)
        .await
        .unwrap()
        .expect("system detail must decode the new status view");
    let list = list_systems_from_view(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|system| system.id == system_id)
        .expect("system list must decode the new status view");
    assert_eq!(detail.deployment_status, "ahead");
    assert_eq!(list.deployment_status, detail.deployment_status);

    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'view_system_deployment_status' \
         ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        columns,
        [
            "hostname",
            "current_store_path",
            "deployment_time",
            "current_commit_hash",
            "current_commit_timestamp",
            "latest_commit_hash",
            "latest_commit_timestamp",
            "commits_behind",
            "flake_name",
            "deployment_status",
            "status_description",
        ]
    );
    let types: Vec<String> = sqlx::query_scalar(
        "SELECT data_type::text FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'view_system_deployment_status' \
         ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        types,
        [
            "text",
            "text",
            "timestamp with time zone",
            "text",
            "timestamp with time zone",
            "text",
            "timestamp with time zone",
            "bigint",
            "text",
            "text",
            "text",
        ]
    );
}
