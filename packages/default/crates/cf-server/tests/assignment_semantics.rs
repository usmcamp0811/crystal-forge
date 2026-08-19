/// Live-DB regression tests for assignment_status semantics.
///
/// These tests verify that assignment_status field is:
/// 1. Correctly determined based on compliance_bundle_assignments by calling production handlers
/// 2. Marked "current" when assignment targets current_published_version_id
/// 3. Marked "pinned" when assignment targets another accepted version
/// 4. Null/None when no active assignment exists
/// 5. Kept completely independent from resolution_state
///
/// Run with: cargo test -p cf-server --test assignment_semantics
///
/// These tests call the production queries module:
///   crystal_forge::queries::compliance::list_bundle_systems_for_version
use sqlx::PgPool;
use uuid::Uuid;

use crystal_forge::queries::compliance::list_bundle_systems_for_version;

// ---------------------------------------------------------------------------
// Schema-accurate fixtures. Every INSERT below matches the current migrations:
//   compliance_bundles   : name, framework, version, layer, owner are NOT NULL
//   compliance_bundle_versions : name, framework, layer, owner, semantic_digest NOT NULL
//   systems              : hostname, public_key, derivation NOT NULL
//   environments         : name NOT NULL (varchar 50)
//   compliance_bundle_assignments : bundle_version_id, scope_type,
//                          assignment_overlay_digest, bundle_id NOT NULL
// ---------------------------------------------------------------------------

async fn create_bundle(pool: &PgPool, name: &str, framework: &str) -> Uuid {
    let bundle_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, framework, version, description, layer, owner, created_at, updated_at)
           VALUES ($1, $2, $3, '1.0', 'Test bundle', 'fleet', 'Platform Security',
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind(name)
    .bind(framework)
    .execute(pool)
    .await
    .expect("create bundle");
    bundle_id
}

async fn create_bundle_version(
    pool: &PgPool,
    bundle_id: Uuid,
    version: &str,
    publication_state: &str,
    description: &str,
) -> Uuid {
    let version_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions
           (id, bundle_id, version, publication_state, name, framework, framework_version,
            description, layer, owner, semantic_digest, created_at)
           VALUES ($1, $2, $3, 'draft', $4, $5, '1.0', $6, 'fleet', 'Platform Security',
                   'sha256-test', CURRENT_TIMESTAMP)"#,
    )
    .bind(version_id)
    .bind(bundle_id)
    .bind(version)
    .bind(format!("Bundle {version}"))
    .bind("NIST CSF")
    .bind(description)
    .execute(pool)
    .await
    .expect("create bundle version");

    // Versions must begin mutable (guard_version_insert_state). Promotion to an
    // immutable state must clear the draft pointer, set the published pointer,
    // and flip publication_state in ONE transaction: the deferred lineage
    // constraint validates the published pointer at commit. This mirrors the
    // production publish ordering (see publish_bundle_version_row).
    if publication_state == "accepted" {
        let mut tx = pool.begin().await.expect("begin tx");
        sqlx::query(
            "UPDATE compliance_bundles SET current_draft_version_id = NULL \
             WHERE current_draft_version_id = $1",
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .expect("clear draft pointer");
        sqlx::query(
            "UPDATE compliance_bundle_versions SET publication_state = 'accepted', \
             published_at = CURRENT_TIMESTAMP WHERE id = $1",
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .expect("accept bundle version");
        sqlx::query(
            "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("set published pointer");
        tx.commit().await.expect("commit publish");
    }
    version_id
}

async fn set_current_published(pool: &PgPool, bundle_id: Uuid, version_id: Uuid) {
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(bundle_id)
        .execute(pool)
        .await
        .expect("set current published");
}

async fn create_environment(pool: &PgPool, name: &str) -> Uuid {
    let env_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO environments (id, name, description, created_at, updated_at)
           VALUES ($1, $2, 'Test environment', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(env_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("create environment");
    env_id
}

async fn create_system(pool: &PgPool, hostname: &str, environment_id: Option<Uuid>) -> Uuid {
    let system_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO systems (id, hostname, environment_id, is_active, public_key, derivation,
                    reachability, created_at, updated_at)
           VALUES ($1, $2, $3, true, $4, $4, 'direct', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(system_id)
    .bind(hostname)
    .bind(environment_id)
    .bind(format!("ssh-key-{hostname}"))
    .execute(pool)
    .await
    .expect("create system");
    system_id
}

async fn create_system_assignment(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    system_id: Uuid,
) {
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments
           (bundle_id, bundle_version_id, system_id, scope_type, active,
            assignment_overlay_digest, created_at, updated_at)
           VALUES ($1, $2, $3, 'system', true, 'test-digest',
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(system_id)
    .execute(pool)
    .await
    .expect("create system assignment");
}

async fn create_environment_assignment(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    environment_id: Uuid,
) {
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments
           (bundle_id, bundle_version_id, environment_id, scope_type, active,
            assignment_overlay_digest, created_at, updated_at)
           VALUES ($1, $2, $3, 'environment', true, 'test-digest',
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(environment_id)
    .execute(pool)
    .await
    .expect("create environment assignment");
}

#[sqlx::test]
async fn test_assignment_status_current_version(pool: PgPool) {
    let bundle_id = create_bundle(&pool, "assignment-test-current", "NIST CSF").await;
    let version_id =
        create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Test version").await;
    set_current_published(&pool, bundle_id, version_id).await;

    let system_id = create_system(&pool, "test-system-current", None).await;
    create_system_assignment(&pool, bundle_id, version_id, system_id).await;

    // Call production handler and verify assignment_status
    let response = list_bundle_systems_for_version(&pool, bundle_id, version_id)
        .await
        .expect("query systems")
        .expect("bundle/version exists");

    assert_eq!(response.systems.len(), 1, "Should have exactly one system");
    let rollup = &response.systems[0];
    assert_eq!(rollup.system_id, system_id, "Should be the assigned system");
    assert_eq!(
        rollup.assignment_status,
        Some("current".to_string()),
        "System assigned to current published version should have status 'current'"
    );
}

#[sqlx::test]
async fn test_assignment_status_pinned_version(pool: PgPool) {
    let bundle_id = create_bundle(&pool, "assignment-test-pinned", "NIST CSF").await;
    let old_version_id =
        create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Old version").await;
    let new_version_id =
        create_bundle_version(&pool, bundle_id, "2.0", "accepted", "New version").await;
    set_current_published(&pool, bundle_id, new_version_id).await;

    let system_id = create_system(&pool, "test-system-pinned", None).await;
    // Assign to the OLD version (not current)
    create_system_assignment(&pool, bundle_id, old_version_id, system_id).await;

    let response = list_bundle_systems_for_version(&pool, bundle_id, old_version_id)
        .await
        .expect("query systems")
        .expect("bundle/version exists");

    assert_eq!(response.systems.len(), 1, "Should have exactly one system");
    let rollup = &response.systems[0];
    assert_eq!(
        rollup.assignment_status,
        Some("pinned".to_string()),
        "System assigned to older version should have status 'pinned' when newer version is current"
    );
}

#[sqlx::test]
async fn test_assignment_status_unassigned(pool: PgPool) {
    let bundle_id = create_bundle(&pool, "assignment-test-unassigned", "NIST CSF").await;
    let version_id =
        create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Test version").await;
    set_current_published(&pool, bundle_id, version_id).await;

    // System exists but has NO assignment
    let _system_id = create_system(&pool, "test-system-unassigned", None).await;

    let response = list_bundle_systems_for_version(&pool, bundle_id, version_id)
        .await
        .expect("query systems");

    // Response should be None since no systems are applicable (no assignments exist)
    assert!(
        response.is_none() || response.unwrap().systems.is_empty(),
        "Unassigned system should not appear in results"
    );
}

#[sqlx::test]
async fn test_assignment_independent_from_resolution(pool: PgPool) {
    let bundle_id = create_bundle(&pool, "assignment-test-independent", "NIST CSF").await;
    let version_id =
        create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Test version").await;
    set_current_published(&pool, bundle_id, version_id).await;

    let system_id = create_system(&pool, "test-system-independent", None).await;
    create_system_assignment(&pool, bundle_id, version_id, system_id).await;

    let response = list_bundle_systems_for_version(&pool, bundle_id, version_id)
        .await
        .expect("query systems")
        .expect("bundle/version exists");

    assert_eq!(response.systems.len(), 1, "Should have exactly one system");
    let rollup = &response.systems[0];

    // assignment_status derives purely from compliance_bundle_assignments.
    // The resolution path (policies in the bundle version) may report
    // not_applicable when no policies exist, but assignment must still be
    // "current" - the two fields are independent concerns.
    assert_eq!(
        rollup.assignment_status,
        Some("current".to_string()),
        "Assignment status should be determined independently of resolution"
    );
}

#[sqlx::test]
async fn test_environment_scoped_assignment(pool: PgPool) {
    let bundle_id = create_bundle(&pool, "assignment-test-env-scoped", "NIST CSF").await;
    let version_id =
        create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Test version").await;
    set_current_published(&pool, bundle_id, version_id).await;

    let env_id = create_environment(&pool, "test-env-scoped").await;
    create_environment_assignment(&pool, bundle_id, version_id, env_id).await;

    let system_id = create_system(&pool, "test-system-in-env", Some(env_id)).await;

    let response = list_bundle_systems_for_version(&pool, bundle_id, version_id)
        .await
        .expect("query systems")
        .expect("bundle/version exists");

    assert!(
        response.systems.iter().any(|r| r.system_id == system_id),
        "System in assigned environment should be included"
    );

    let rollup = response
        .systems
        .iter()
        .find(|r| r.system_id == system_id)
        .expect("system found");
    assert_eq!(
        rollup.assignment_status,
        Some("current".to_string()),
        "Environment-scoped assignment should result in 'current' status"
    );
}

#[sqlx::test]
async fn test_system_assignment_precedence(pool: PgPool) {
    let bundle_id = create_bundle(&pool, "assignment-test-precedence", "NIST CSF").await;
    let old_version_id =
        create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Old version").await;
    let new_version_id =
        create_bundle_version(&pool, bundle_id, "2.0", "accepted", "New version").await;
    set_current_published(&pool, bundle_id, new_version_id).await;

    let env_id = create_environment(&pool, "test-env-precedence").await;
    create_environment_assignment(&pool, bundle_id, new_version_id, env_id).await;

    let system_id = create_system(&pool, "test-system-precedence", Some(env_id)).await;
    // System-scoped assignment to OLD version overrides env assignment to new
    create_system_assignment(&pool, bundle_id, old_version_id, system_id).await;

    let response = list_bundle_systems_for_version(&pool, bundle_id, old_version_id)
        .await
        .expect("query systems")
        .expect("bundle/version exists");

    let rollup = response
        .systems
        .iter()
        .find(|r| r.system_id == system_id)
        .expect("system found");

    assert_eq!(
        rollup.assignment_status,
        Some("pinned".to_string()),
        "System-scoped assignment should take precedence and show 'pinned' because new is current"
    );
}
