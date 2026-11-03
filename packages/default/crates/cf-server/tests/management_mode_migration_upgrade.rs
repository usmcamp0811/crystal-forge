//! Proves migrations 0311 onward upgrade a database that already contains
//! enrolled systems, not only a fresh empty database.
//!
//! # Why
//!
//! Management modes add a column, constraints, views and triggers around the
//! `systems` table. A constraint that legacy rows violate, or a backfill that
//! changes what an existing row means, fails or silently corrupts only on a
//! populated database. An empty database has no such rows, so every ordinary
//! migration-backed test passes without exercising the upgrade.
//!
//! # Contract
//!
//! Every system that exists before 0311 is a managed system with the key,
//! deployment policy and observation history it had. No migration infers an
//! external or reference resource, rewrites a key, enables a policy, or starts a
//! new enrollment boundary for it. After the upgrade, the new modes work.

use sqlx::{PgPool, migrate::Migrate};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The last schema before management modes.
const LEGACY_VERSION: i64 = 310;

/// Applies every embedded migration up to and including `version`.
async fn apply_migrations_through(pool: &PgPool, version: i64) {
    let mut connection = pool.acquire().await.expect("acquire migration connection");
    connection
        .ensure_migrations_table()
        .await
        .expect("create migrations table");
    for migration in MIGRATOR.iter().filter(|m| m.version <= version) {
        connection
            .apply(migration)
            .await
            .unwrap_or_else(|error| panic!("apply migration {}: {error}", migration.version));
    }
}

/// Applies every embedded migration after `version`, one at a time, so a failure
/// names the migration that a populated database rejects.
async fn apply_migrations_after(pool: &PgPool, version: i64) -> Vec<i64> {
    let mut connection = pool.acquire().await.expect("acquire migration connection");
    let mut applied = Vec::new();
    for migration in MIGRATOR.iter().filter(|m| m.version > version) {
        connection
            .apply(migration)
            .await
            .unwrap_or_else(|error| panic!("apply migration {}: {error}", migration.version));
        applied.push(migration.version);
    }
    applied
}

/// What an operator would call "this system", read through columns that exist
/// both before and after the upgrade.
#[derive(Debug, PartialEq, sqlx::FromRow)]
struct LegacySystem {
    hostname: String,
    public_key: Option<String>,
    deployment_policy: String,
    is_active: bool,
    environment_id: Option<Uuid>,
    flake_id: Option<i32>,
}

async fn read_legacy_systems(pool: &PgPool) -> Vec<LegacySystem> {
    sqlx::query_as(
        "SELECT hostname, public_key, deployment_policy, is_active, environment_id, flake_id
         FROM systems ORDER BY hostname",
    )
    .fetch_all(pool)
    .await
    .expect("systems should load")
}

#[sqlx::test(migrations = false)]
async fn existing_enrolled_systems_survive_the_management_mode_upgrade(pool: PgPool) {
    apply_migrations_through(&pool, LEGACY_VERSION).await;

    // Legacy data: three enrolled systems with different policies, one inactive,
    // and observation history for each.
    let environment: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES('legacy-env') RETURNING id")
            .fetch_one(&pool)
            .await
            .unwrap();
    let flake: i32 = sqlx::query_scalar(
        "INSERT INTO flakes(name,repo_url,branch) VALUES('legacy-flake','https://example.test/legacy','main') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for (hostname, policy, active) in [
        ("legacy-manual", "manual", true),
        ("legacy-auto", "auto_latest", true),
        ("legacy-retired", "manual", false),
    ] {
        sqlx::query(
            "INSERT INTO systems(hostname,public_key,derivation,deployment_policy,environment_id,flake_id,is_active)
             VALUES($1,$2,'',$3,$4,$5,$6)",
        )
        .bind(hostname)
        .bind(format!("legacy-key-for-{hostname}"))
        .bind(policy)
        .bind(environment)
        .bind(flake)
        .bind(active)
        .execute(&pool)
        .await
        .unwrap();
        for generation in [1, 2] {
            sqlx::query(
                "INSERT INTO system_states(hostname,change_reason,store_path,generation,
                    generation_matches_current_store_path,timestamp)
                 VALUES($1,'startup',$2,$3,true,now()-($3||' hours')::interval)",
            )
            .bind(hostname)
            .bind(format!("/nix/store/{}-{hostname}", "a".repeat(32)))
            .bind(generation)
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    let before = read_legacy_systems(&pool).await;
    assert_eq!(before.len(), 3);

    let applied = apply_migrations_after(&pool, LEGACY_VERSION).await;
    assert!(
        applied.contains(&311) && applied.contains(&328),
        "the upgrade covers management modes through static publication reuse"
    );

    // The same systems, with the same keys, policies and activity.
    assert_eq!(read_legacy_systems(&pool).await, before);
    let modes: Vec<(
        String,
        String,
        Option<Uuid>,
        Option<chrono::DateTime<chrono::Utc>>,
    )> = sqlx::query_as(
        "SELECT hostname, management_mode, parent_host_id, agent_enrolled_at
             FROM systems ORDER BY hostname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for (hostname, mode, parent, enrolled) in &modes {
        assert_eq!(
            mode, "managed",
            "{hostname} was enrolled before modes existed"
        );
        assert_eq!(*parent, None, "{hostname}");
        assert_eq!(
            *enrolled, None,
            "{hostname} keeps legacy semantics: all of its history stays current"
        );
    }
    // No pending deployment, desired target or reservation appeared.
    let targets: i64 =
        sqlx::query_scalar("SELECT count(*) FROM systems WHERE desired_target IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(targets, 0);
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM pending_system_deployments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pending, 0);

    // Observation history is intact and, with a NULL boundary, entirely current
    // for the active managed systems.
    let history: i64 = sqlx::query_scalar("SELECT count(*) FROM system_states")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(history, 6);
    let current: i64 = sqlx::query_scalar("SELECT count(*) FROM view_agent_current_system_states")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        current, 4,
        "two active systems keep both observations; the retired one has none"
    );

    // The rewritten list and detail views still return every existing system. The
    // views do not expose the mode; the API reads it from `systems`, asserted above.
    let listed: Vec<String> =
        sqlx::query_scalar("SELECT hostname FROM view_system_list ORDER BY hostname")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        listed,
        ["legacy-auto", "legacy-manual"],
        "the list shows the active systems"
    );
    let detail: Vec<String> =
        sqlx::query_scalar("SELECT hostname FROM view_system_detail ORDER BY hostname")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        detail,
        ["legacy-auto", "legacy-manual", "legacy-retired"],
        "detail also keeps the retired system"
    );

    // After the upgrade the new modes work and invalid ones are rejected.
    sqlx::query(
        "INSERT INTO systems(hostname,management_mode,derivation,environment_id,flake_id)
         VALUES('golden-config','reference','',$1,$2)",
    )
    .bind(environment)
    .bind(flake)
    .execute(&pool)
    .await
    .expect("a keyless reference registration is valid after the upgrade");
    for (statement, why) in [
        (
            "INSERT INTO systems(hostname,management_mode,derivation) VALUES('bad-mode','invented','')",
            "an invented mode",
        ),
        (
            "INSERT INTO systems(hostname,management_mode,derivation) VALUES('keyless-managed','managed','')",
            "a managed system without a key",
        ),
        (
            "INSERT INTO systems(hostname,management_mode,public_key,derivation) VALUES('keyed-reference','reference','k','')",
            "a reference resource holding an agent key",
        ),
    ] {
        assert!(
            sqlx::query(statement).execute(&pool).await.is_err(),
            "{why} must be rejected"
        );
    }
    // Admission of reused static publications is available.
    let eligible: bool = sqlx::query_scalar(
        "SELECT derivation_publication_eligible(1, '/nix/store/x', NULL, 'ssh-ng://cache@host')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!eligible, "no publication exists, so nothing is eligible");
}
