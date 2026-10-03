//! Isolated PostgreSQL plan measurement for auto-latest and deployment views.
//! Run the ignored test only against the disposable task cluster on port 35457.

use crystal_forge::queries::derivations::get_latest_deployable_targets_for_flake_hosts;
use sqlx::PgPool;
use uuid::Uuid;

// Contract copy of the SQLx query in queries/derivations.rs:
// get_latest_deployable_targets_for_flake_hosts. Keep the predicates, ordering,
// projections, and bind positions in sync when changing the production query.
const BATCHED_SELECTOR_SQL: &str = r#"
         WITH per_host AS (
           SELECT
             d.derivation_name AS hostname,
             d.id              AS derivation_id,
             d.store_path,
             f.repo_url        AS repo_url,
             c.git_commit_hash AS commit_hash,
             c.flake_id,
             c.commit_timestamp,
             (SELECT MAX(cpj.completed_at)
                FROM cache_push_jobs cpj
               WHERE cpj.derivation_id = d.id
                  AND cpj.status = 'completed'
                  AND cpj.store_path = d.store_path) AS last_cache_completed_at,
             ROW_NUMBER() OVER (
               PARTITION BY d.derivation_name
               ORDER BY c.commit_timestamp DESC,
                        d.completed_at DESC NULLS LAST,
                        d.id DESC
             ) AS rn
           FROM derivations d
           JOIN commits c
             ON d.commit_id = c.id
           JOIN flakes f
             ON c.flake_id = f.id
           WHERE c.flake_id = $1
             AND d.derivation_type = 'nixos'
             AND d.derivation_name = ANY($2::text[])
             AND d.store_path IS NOT NULL
             AND BTRIM(d.store_path) <> ''
              AND d.cf_agent_enabled IS TRUE
              AND d.policy_requirements_met IS TRUE
              AND d.error_message IS NULL
              AND EXISTS (
                SELECT 1 FROM cache_push_jobs cpj
                WHERE cpj.derivation_id = d.id
                  AND cpj.status = 'completed'
                  AND cpj.store_path = d.store_path
              )
         )
         SELECT
           hostname,
           derivation_id,
           store_path,
           last_cache_completed_at,
           repo_url,
           commit_hash,
           EXISTS (
             SELECT 1 FROM commits newer
             WHERE newer.flake_id = per_host.flake_id
               AND newer.commit_timestamp > per_host.commit_timestamp
           ) AS "newer_raw_commit_exists!"
         FROM per_host
         WHERE rn = 1
"#;

async fn plan(
    pool: &PgPool,
    label: &str,
    sql: &str,
    flake: Option<i32>,
    configs: &[String],
    host: &str,
) -> Vec<String> {
    let explain = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT) {sql}");
    let lines: Vec<String> = match flake {
        Some(flake) => sqlx::query_scalar(&explain)
            .bind(flake)
            .bind(configs)
            .fetch_all(pool)
            .await
            .expect("selector explain"),
        None if sql.contains("$1") => sqlx::query_scalar(&explain)
            .bind(host)
            .fetch_all(pool)
            .await
            .expect("single-host explain"),
        None => sqlx::query_scalar(&explain)
            .fetch_all(pool)
            .await
            .expect("full-view explain"),
    };
    println!("\n{label} EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT):");
    for line in &lines {
        println!("{line}");
    }
    lines
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PostgreSQL at 127.0.0.1:35457"]
async fn measures_auto_latest_and_system_views(pool: PgPool) {
    // SQLx creates a separate migrated test database on this server. Fail closed
    // before fixture writes if DATABASE_URL was pointed at another cluster.
    let identity: (String, String, Option<String>) = sqlx::query_as(
        "SELECT current_setting('data_directory'), current_setting('port'), inet_server_addr()::text",
    )
    .fetch_one(&pool)
    .await
    .expect("server identity");
    assert_eq!(identity.0, "/tmp/opencode/cf322-pg-35457");
    assert_eq!(identity.1, "35457");
    assert_eq!(identity.2.as_deref(), Some("127.0.0.1/32"));

    let suffix = Uuid::new_v4().simple().to_string();
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(&suffix)
            .bind(format!("https://example.invalid/{suffix}"))
            .fetch_one(&pool)
            .await
            .expect("flake");
    let configs: Vec<String> = (0..6).map(|n| format!("config-{n}-{suffix}")).collect();
    let commits = sqlx::query(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
         SELECT $1, $2 || '-' || n::text, '2026-01-01'::timestamptz + n * interval '1 hour' \
         FROM generate_series(0, 239) n",
    )
    .bind(flake)
    .bind(&suffix)
    .execute(&pool)
    .await
    .expect("240 commits");
    assert_eq!(commits.rows_affected(), 240);

    let built = sqlx::query(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, \
             attempt_count, store_path, cf_agent_enabled, policy_requirements_met, completed_at) \
         SELECT c.id, 'nixos', config.name, 11, 0, \
                '/nix/store/' || $2 || '-' || c.id::text || '-' || config.name, \
                true, true, c.commit_timestamp + interval '5 minutes' \
         FROM commits c CROSS JOIN unnest($3::text[]) config(name) \
         WHERE c.flake_id = $1",
    )
    .bind(flake)
    .bind(&suffix)
    .bind(&configs)
    .execute(&pool)
    .await
    .expect("nixos builds");
    assert_eq!(built.rows_affected(), 1440);
    let published = sqlx::query(
        "INSERT INTO cache_push_jobs (derivation_id, status, store_path, completed_at) \
          SELECT d.id, 'completed', d.store_path, c.commit_timestamp + interval '10 minutes' \
         FROM derivations d JOIN commits c ON c.id = d.commit_id \
         WHERE c.flake_id = $1 AND (EXTRACT(EPOCH FROM (c.commit_timestamp - '2026-01-01'::timestamptz))::int / 3600) % 3 = 0",
    )
    .bind(flake)
    .execute(&pool)
    .await
    .expect("completed cache pushes for subset");
    assert_eq!(published.rows_affected(), 480);

    let mut hosts = Vec::new();
    for n in 0..12 {
        let host = format!("host-{n}-{suffix}");
        let config = &configs[n % configs.len()];
        sqlx::query(
            "INSERT INTO systems (hostname, public_key, derivation, flake_id, \
             system_configuration_name, is_active) \
             VALUES ($1, 'test-key', 'test-derivation', $2, $3, true)",
        )
        .bind(&host)
        .bind(flake)
        .bind(config)
        .execute(&pool)
        .await
        .expect("active system");
        // Two state rows per host exercise latest-state selection and fanout.
        for offset in [0_i64, if n % 2 == 0 { 237 } else { 234 }] {
            let path: String = sqlx::query_scalar(
                "SELECT d.store_path FROM derivations d JOIN commits c ON c.id = d.commit_id \
                 WHERE c.flake_id = $1 AND d.derivation_name = $2 \
                   AND c.commit_timestamp = '2026-01-01'::timestamptz + $3 * interval '1 hour'",
            )
            .bind(flake)
            .bind(config)
            .bind(offset)
            .fetch_one(&pool)
            .await
            .expect("built path");
            sqlx::query(
                "INSERT INTO system_states (hostname, change_reason, store_path, timestamp) \
                 VALUES ($1, 'startup', $2, '2026-02-01'::timestamptz + $3 * interval '1 hour')",
            )
            .bind(&host)
            .bind(path)
            .bind(offset)
            .execute(&pool)
            .await
            .expect("observed state");
        }
        hosts.push(host);
    }
    for table in [
        "commits",
        "derivations",
        "cache_push_jobs",
        "systems",
        "system_states",
    ] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(&pool)
            .await
            .expect("fixture statistics");
    }

    let selected = get_latest_deployable_targets_for_flake_hosts(&pool, flake, &configs)
        .await
        .expect("production batched selector");
    assert_eq!(selected.len(), configs.len());
    for config in &configs {
        let expected: i32 = sqlx::query_scalar(
            "SELECT d.id FROM derivations d JOIN commits c ON c.id = d.commit_id \
             JOIN cache_push_jobs cpj ON cpj.derivation_id = d.id AND cpj.status = 'completed' \
             WHERE c.flake_id = $1 AND d.derivation_name = $2 \
             ORDER BY c.commit_timestamp DESC, d.completed_at DESC NULLS LAST, d.id DESC LIMIT 1",
        )
        .bind(flake)
        .bind(config)
        .fetch_one(&pool)
        .await
        .expect("expected newest cached build");
        assert_eq!(
            selected
                .iter()
                .filter(|row| &row.hostname == config)
                .count(),
            1
        );
        assert_eq!(
            selected
                .iter()
                .find(|row| &row.hostname == config)
                .unwrap()
                .derivation_id,
            expected
        );
    }
    for view in [
        "view_system_deployment_status",
        "view_system_list",
        "view_system_detail",
    ] {
        let total: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {view} WHERE hostname = ANY($1::text[])"
        ))
        .bind(&hosts)
        .fetch_one(&pool)
        .await
        .expect("view total");
        assert_eq!(total, 12, "{view} fanout");
        for host in &hosts {
            let count: i64 =
                sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {view} WHERE hostname = $1"))
                    .bind(host)
                    .fetch_one(&pool)
                    .await
                    .expect("per-host view count");
            assert_eq!(count, 1, "{view} for {host}");
        }
    }
    let statuses: Vec<(String, String)> = sqlx::query_as(
        "SELECT hostname, deployment_status FROM view_system_deployment_status \
         WHERE hostname = ANY($1::text[]) ORDER BY hostname",
    )
    .bind(&hosts)
    .fetch_all(&pool)
    .await
    .expect("deployment statuses");
    assert_eq!(
        statuses
            .iter()
            .filter(|(_, status)| status == "up_to_date")
            .count(),
        6
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|(_, status)| status == "behind")
            .count(),
        6
    );

    let view_queries = [
        (
            "status single",
            "SELECT * FROM view_system_deployment_status WHERE hostname = $1",
        ),
        ("status all", "SELECT * FROM view_system_deployment_status"),
        (
            "status single count",
            "SELECT COUNT(*) FROM view_system_deployment_status WHERE hostname = $1",
        ),
        (
            "status full count",
            "SELECT COUNT(*) FROM view_system_deployment_status",
        ),
        (
            "list single",
            "SELECT * FROM view_system_list WHERE hostname = $1",
        ),
        ("list all", "SELECT * FROM view_system_list"),
        (
            "list single count",
            "SELECT COUNT(*) FROM view_system_list WHERE hostname = $1",
        ),
        ("list full count", "SELECT COUNT(*) FROM view_system_list"),
        (
            "detail single",
            "SELECT * FROM view_system_detail WHERE hostname = $1",
        ),
        ("detail all", "SELECT * FROM view_system_detail"),
        (
            "detail single count",
            "SELECT COUNT(*) FROM view_system_detail WHERE hostname = $1",
        ),
        (
            "detail full count",
            "SELECT COUNT(*) FROM view_system_detail",
        ),
    ];
    // The migration index is present initially. Remove it only from this
    // disposable SQLx database to compare the pre-index query plan.
    sqlx::query("DROP INDEX derivations_deployment_effective_store_path_idx")
        .execute(&pool)
        .await
        .expect("remove migration index in isolated test database");
    sqlx::query("ANALYZE derivations")
        .execute(&pool)
        .await
        .expect("unindexed fixture statistics");
    for indexed in [false, true] {
        if indexed {
            // Restore the production migration's index only in this test DB.
            sqlx::query(
                "CREATE INDEX derivations_deployment_effective_store_path_idx \
                  ON derivations ((COALESCE(store_path, expected_store_path)))",
            )
            .execute(&pool)
            .await
            .expect("candidate expression index");
            sqlx::query("ANALYZE derivations")
                .execute(&pool)
                .await
                .expect("indexed fixture statistics");
        }
        let phase = if indexed { "indexed" } else { "baseline" };
        plan(
            &pool,
            &format!("{phase} batched selector"),
            BATCHED_SELECTOR_SQL,
            Some(flake),
            &configs,
            &hosts[0],
        )
        .await;
        for (label, sql) in view_queries {
            let lines = plan(
                &pool,
                &format!("{phase} {label}"),
                sql,
                None,
                &configs,
                &hosts[0],
            )
            .await;
            if indexed && matches!(label, "status single" | "list single" | "detail single") {
                assert!(
                    lines.iter().any(|line| {
                        line.contains(
                            "Index Scan using derivations_deployment_effective_store_path_idx",
                        ) || line.contains(
                            "Bitmap Index Scan on derivations_deployment_effective_store_path_idx",
                        )
                    }),
                    "{label} must use the expression index at normal planner settings"
                );
            }
        }
    }
}
