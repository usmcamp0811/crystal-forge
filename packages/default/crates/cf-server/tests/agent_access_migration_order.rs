//! Upgrades databases that applied the CF-AGENT-ACCESS-001 migrations in a
//! non-numbered order, through the same embedded runner as server startup.
//!
//! TASK-534.9 (0346) reached a persistent database from a branch that did not
//! yet contain 0343, and the TASK-534.7/534.8 branch carries 0344, 0345 and
//! 0347. SQLx applies every missing version at startup in ascending order,
//! including versions lower than an applied version. Each test seeds one
//! deployed predecessor state, then runs the full migrator. Every test uses a
//! disposable SQLx-created database.

use sqlx::{PgPool, migrate::Migrate};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Every scope that a release of CF-AGENT-ACCESS-001 defines.
const ALL_SCOPES: [&str; 14] = [
    "cves:read",
    "poams:read",
    "provenance:read",
    "scans:request",
    "poams:write",
    "poams:verify",
    "poams:close",
    "poams:create",
    "policies:read",
    "policies:draft",
    "policies:import",
    "bundles:read",
    "bundles:draft",
    "compliance:preview",
];

/// Applies the given versions in the given order, as a deployed branch did.
async fn apply_in_order(pool: &PgPool, versions: impl IntoIterator<Item = i64>) {
    let mut connection = pool.acquire().await.unwrap();
    connection.ensure_migrations_table().await.unwrap();
    for version in versions {
        let migration = MIGRATOR
            .iter()
            .find(|migration| migration.version == version)
            .unwrap_or_else(|| panic!("migration {version} exists"));
        connection.apply(migration).await.unwrap();
    }
}

fn versions_through(last: i64) -> Vec<i64> {
    MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .filter(|version| *version <= last)
        .collect()
}

async fn scope_check(pool: &PgPool) -> String {
    sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conname = 'api_access_token_scopes_scope_check'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Runs the startup migrator and checks the final state that every order
/// must reach.
async fn upgrade_and_assert_final_state(pool: &PgPool) {
    MIGRATOR.run(pool).await.unwrap();

    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
            .fetch_all(pool)
            .await
            .unwrap();
    let expected: Vec<i64> = MIGRATOR.iter().map(|migration| migration.version).collect();
    assert_eq!(applied, expected);

    let check = scope_check(pool).await;
    for scope in ALL_SCOPES {
        assert!(check.contains(&format!("'{scope}'")), "{scope} in {check}");
    }
    // The planning table (0346) and the statement-level bundle revision
    // triggers (0347) both exist after the upgrade.
    let objects: (bool, bool) = sqlx::query_as(
        "SELECT to_regclass('poam_configuration_cve_findings') IS NOT NULL, \
                EXISTS (SELECT 1 FROM pg_trigger \
                        WHERE tgname = 'trigger_bump_bundle_membership_revision_insert')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(objects, (true, true));
}

/// A database that ran TASK-534.9 before its branch contained 0343.
#[sqlx::test(migrations = false)]
async fn planning_migration_applied_before_compliance_scopes(pool: PgPool) {
    let mut deployed = versions_through(342);
    deployed.push(346);
    apply_in_order(&pool, deployed).await;
    assert!(scope_check(&pool).await.contains("'poams:create'"));

    upgrade_and_assert_final_state(&pool).await;
}

/// The same predecessor with a `poams:create` grant, as on the development
/// server. 0343 cannot add its 13-scope CHECK while that row exists, so the
/// startup migrator stops before 0348. The documented operator step removes
/// only the grant row; the token, its other grants and its history remain.
/// The upgrade then completes and the new allowlist accepts the scope again.
#[sqlx::test(migrations = false)]
async fn planning_grant_requires_operator_step_before_compliance_scopes(pool: PgPool) {
    let mut deployed = versions_through(342);
    deployed.push(346);
    apply_in_order(&pool, deployed).await;
    let token: uuid::Uuid = sqlx::query_scalar(
        "WITH owner AS (
             INSERT INTO users (username, email, first_name, last_name)
             VALUES ('order-owner', 'order-owner@example.invalid', 'Order', 'Owner')
             RETURNING id)
         INSERT INTO api_access_tokens
             (user_id, name, audience, secret_hash, display_hint, expires_at)
         SELECT id, 'planning', 'mcp', sha256('order-token'::bytea), 'cfat_v1_order',
                now() + interval '30 days'
         FROM owner RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO api_access_token_scopes (token_id, scope)
         VALUES ($1, 'cves:read'), ($1, 'poams:create')",
    )
    .bind(token)
    .execute(&pool)
    .await
    .unwrap();

    let blocked = MIGRATOR.run(&pool).await;
    assert!(
        blocked.is_err(),
        "0343 rejects the existing poams:create row"
    );
    let applied_0343: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version = 343)")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!applied_0343, "the failed 0343 transaction records nothing");

    // Operator step from the TASK-534.9 record.
    sqlx::query("DELETE FROM api_access_token_scopes WHERE scope = 'poams:create'")
        .execute(&pool)
        .await
        .unwrap();
    upgrade_and_assert_final_state(&pool).await;

    let scopes: Vec<String> = sqlx::query_scalar(
        "SELECT scope FROM api_access_token_scopes WHERE token_id = $1 ORDER BY scope",
    )
    .bind(token)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(scopes, ["cves:read"]);
    sqlx::query(
        "INSERT INTO api_access_token_scopes (token_id, scope) VALUES ($1, 'poams:create')",
    )
    .bind(token)
    .execute(&pool)
    .await
    .unwrap();
}

/// A database that ran TASK-534.9 on the TASK-534.6 base (0343, then 0346).
#[sqlx::test(migrations = false)]
async fn planning_migration_applied_before_authoring_migrations(pool: PgPool) {
    let mut deployed = versions_through(343);
    deployed.push(346);
    apply_in_order(&pool, deployed).await;

    upgrade_and_assert_final_state(&pool).await;
}

/// A database that ran TASK-534.8 (through 0347) before 0346 existed.
#[sqlx::test(migrations = false)]
async fn authoring_migrations_applied_before_planning_migration(pool: PgPool) {
    let mut deployed = versions_through(345);
    deployed.push(347);
    apply_in_order(&pool, deployed).await;
    assert!(!scope_check(&pool).await.contains("'poams:create'"));

    upgrade_and_assert_final_state(&pool).await;
}

/// A database that ran no CF-AGENT-ACCESS-001 migration after 0335.
#[sqlx::test(migrations = false)]
async fn dev_database_upgrades_in_numbered_order(pool: PgPool) {
    apply_in_order(&pool, versions_through(335)).await;

    upgrade_and_assert_final_state(&pool).await;
}
