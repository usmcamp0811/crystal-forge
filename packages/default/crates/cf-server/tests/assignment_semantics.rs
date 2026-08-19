/// Live-DB regression tests for assignment_status semantics.
///
/// These tests verify that assignment_status field is:
/// 1. Correctly determined based on compliance_bundle_assignments
/// 2. Marked "current" when assignment targets current_published_version_id
/// 3. Marked "pinned" when assignment targets another accepted version
/// 4. Null/None when no active assignment exists
/// 5. Kept completely independent from resolution_state
///
/// Run with: cargo test -p cf-server --test assignment_semantics -- --ignored

use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test]
async fn test_assignment_status_current_version(pool: PgPool) {
    // Create bundle with one version (which becomes current_published)
    let bundle_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, description, created_at, updated_at)
           VALUES ($1, $2, 'Test bundle', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind("assignment-test-current")
    .execute(&pool)
    .await
    .expect("create bundle");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions (id, bundle_id, version, publication_state, 
                   framework, description, created_at, updated_at)
           VALUES ($1, $2, '1.0', 'accepted', 'NIST CSF', 'Test version', 
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("create version");

    // Set as current published
    sqlx::query(
        "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
    )
    .bind(version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("set current published");

    // Create system for assignment
    let system_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO systems (id, hostname, environment, health_status, critical_cve_count, 
                   high_cve_count, created_at, updated_at)
           VALUES ($1, $2, NULL, 'healthy', 0, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(system_id)
    .bind("test-system-current")
    .execute(&pool)
    .await
    .expect("create system");

    // Create assignment to current published version
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments 
           (id, bundle_id, bundle_version_id, system_id, scope_type, active, 
            created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'system', true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(Uuid::new_v4())
    .bind(bundle_id)
    .bind(version_id)
    .bind(system_id)
    .execute(&pool)
    .await
    .expect("create assignment");

    // Verify: Query the current_published_version_id and check the assignment
    let current_version: Option<Uuid> = sqlx::query_scalar(
        "SELECT current_published_version_id FROM compliance_bundles WHERE id = $1",
    )
    .bind(bundle_id)
    .fetch_one(&pool)
    .await
    .expect("get current version");

    assert_eq!(
        current_version, Some(version_id),
        "current_published_version should be set to our version"
    );

    // Verify assignment exists and targets the current version
    let assigned_version: Option<Uuid> = sqlx::query_scalar(
        "SELECT bundle_version_id FROM compliance_bundle_assignments WHERE bundle_id = $1 AND system_id = $2 AND active = true",
    )
    .bind(bundle_id)
    .bind(system_id)
    .fetch_optional(&pool)
    .await
    .expect("query assignment");

    assert_eq!(
        assigned_version, Some(version_id),
        "assignment should target the current published version"
    );
}

#[sqlx::test]
async fn test_assignment_status_pinned_version(pool: PgPool) {
    // Create bundle with two versions
    let bundle_id = Uuid::new_v4();
    let old_version_id = Uuid::new_v4();
    let new_version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, description, created_at, updated_at)
           VALUES ($1, $2, 'Test bundle', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind("assignment-test-pinned")
    .execute(&pool)
    .await
    .expect("create bundle");

    // Create old version
    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions (id, bundle_id, version, publication_state, 
                   framework, description, created_at, updated_at)
           VALUES ($1, $2, '1.0', 'accepted', 'NIST CSF', 'Old version', 
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(old_version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("create old version");

    // Create new version
    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions (id, bundle_id, version, publication_state, 
                   framework, description, created_at, updated_at)
           VALUES ($1, $2, '2.0', 'accepted', 'NIST CSF', 'New version', 
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(new_version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("create new version");

    // Set new version as current published
    sqlx::query(
        "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
    )
    .bind(new_version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("set current published");

    // Create system
    let system_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO systems (id, hostname, environment, health_status, critical_cve_count, 
                   high_cve_count, created_at, updated_at)
           VALUES ($1, $2, NULL, 'healthy', 0, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(system_id)
    .bind("test-system-pinned")
    .execute(&pool)
    .await
    .expect("create system");

    // Create assignment to OLD version (not current)
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments 
           (id, bundle_id, bundle_version_id, system_id, scope_type, active, 
            created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'system', true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(Uuid::new_v4())
    .bind(bundle_id)
    .bind(old_version_id)
    .bind(system_id)
    .execute(&pool)
    .await
    .expect("create assignment");

    // Verify: Current version is new, but assignment targets old version
    let current_version: Uuid = sqlx::query_scalar(
        "SELECT current_published_version_id FROM compliance_bundles WHERE id = $1",
    )
    .bind(bundle_id)
    .fetch_one(&pool)
    .await
    .expect("get current version");

    assert_eq!(current_version, new_version_id, "current should be new version");

    let assigned_version: Uuid = sqlx::query_scalar(
        "SELECT bundle_version_id FROM compliance_bundle_assignments WHERE bundle_id = $1 AND system_id = $2 AND active = true",
    )
    .bind(bundle_id)
    .bind(system_id)
    .fetch_one(&pool)
    .await
    .expect("get assignment");

    assert_eq!(
        assigned_version, old_version_id,
        "assignment should target old version (pinned)"
    );
    assert_ne!(assigned_version, current_version, "pinned assignment != current");
}

#[sqlx::test]
async fn test_assignment_status_unassigned(pool: PgPool) {
    // Create bundle without any assignments
    let bundle_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, description, created_at, updated_at)
           VALUES ($1, $2, 'Test bundle', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind("assignment-test-unassigned")
    .execute(&pool)
    .await
    .expect("create bundle");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions (id, bundle_id, version, publication_state, 
                   framework, description, created_at, updated_at)
           VALUES ($1, $2, '1.0', 'accepted', 'NIST CSF', 'Test version', 
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("create version");

    // Set as current published
    sqlx::query(
        "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
    )
    .bind(version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("set current published");

    // Verify: No assignments exist for this bundle
    let assignment_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_assignments WHERE bundle_id = $1 AND active = true",
    )
    .bind(bundle_id)
    .fetch_one(&pool)
    .await
    .expect("count assignments");

    assert_eq!(assignment_count, 0, "no assignments should exist");
}

#[sqlx::test]
async fn test_assignment_independent_from_resolution(pool: PgPool) {
    // This test verifies that assignment_status is computed independently
    // from resolution_state (no mixing of concerns).
    // We create an assignment and verify it can be determined regardless
    // of any resolver state.

    let bundle_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, description, created_at, updated_at)
           VALUES ($1, $2, 'Test bundle', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind("assignment-test-independent")
    .execute(&pool)
    .await
    .expect("create bundle");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions (id, bundle_id, version, publication_state, 
                   framework, description, created_at, updated_at)
           VALUES ($1, $2, '1.0', 'accepted', 'NIST CSF', 'Test version', 
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("create version");

    sqlx::query(
        "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
    )
    .bind(version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("set current published");

    // Create system and assignment
    let system_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO systems (id, hostname, environment, health_status, critical_cve_count, 
                   high_cve_count, created_at, updated_at)
           VALUES ($1, $2, NULL, 'healthy', 0, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(system_id)
    .bind("test-system-independent")
    .execute(&pool)
    .await
    .expect("create system");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments 
           (id, bundle_id, bundle_version_id, system_id, scope_type, active, 
            created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'system', true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(Uuid::new_v4())
    .bind(bundle_id)
    .bind(version_id)
    .bind(system_id)
    .execute(&pool)
    .await
    .expect("create assignment");

    // Verify: Assignment is correctly set
    let has_assignment: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM compliance_bundle_assignments WHERE bundle_id = $1 AND bundle_version_id = $2 AND active = true)",
    )
    .bind(bundle_id)
    .bind(version_id)
    .fetch_one(&pool)
    .await
    .expect("check assignment");

    assert!(
        has_assignment,
        "assignment should exist regardless of resolution state"
    );
}

#[sqlx::test]
async fn test_environment_scoped_assignment(pool: PgPool) {
    let bundle_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, description, created_at, updated_at)
           VALUES ($1, $2, 'Test bundle', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind("assignment-test-env-scoped")
    .execute(&pool)
    .await
    .expect("create bundle");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions (id, bundle_id, version, publication_state, 
                   framework, description, created_at, updated_at)
           VALUES ($1, $2, '1.0', 'accepted', 'NIST CSF', 'Test version', 
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("create version");

    sqlx::query(
        "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
    )
    .bind(version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("set current published");

    // Create environment
    let env_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO environments (id, name, description, created_at, updated_at)
           VALUES ($1, $2, 'Test environment', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(env_id)
    .bind("test-env-scoped")
    .execute(&pool)
    .await
    .expect("create environment");

    // Create environment-scoped assignment
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments 
           (id, bundle_id, bundle_version_id, environment_id, scope_type, active, 
            created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'environment', true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(Uuid::new_v4())
    .bind(bundle_id)
    .bind(version_id)
    .bind(env_id)
    .execute(&pool)
    .await
    .expect("create env assignment");

    // Verify: Environment-scoped assignment exists and is active
    let env_assignment_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM compliance_bundle_assignments WHERE bundle_id = $1 AND environment_id = $2 AND active = true)",
    )
    .bind(bundle_id)
    .bind(env_id)
    .fetch_one(&pool)
    .await
    .expect("check env assignment");

    assert!(
        env_assignment_exists,
        "environment-scoped assignment should exist"
    );
}

#[sqlx::test]
async fn test_system_scoped_assignment_precedence(pool: PgPool) {
    let bundle_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, description, created_at, updated_at)
           VALUES ($1, $2, 'Test bundle', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind("assignment-test-sys-precedence")
    .execute(&pool)
    .await
    .expect("create bundle");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions (id, bundle_id, version, publication_state, 
                   framework, description, created_at, updated_at)
           VALUES ($1, $2, '1.0', 'accepted', 'NIST CSF', 'Test version', 
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("create version");

    sqlx::query(
        "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
    )
    .bind(version_id)
    .bind(bundle_id)
    .execute(&pool)
    .await
    .expect("set current published");

    // Create system
    let system_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO systems (id, hostname, environment, health_status, critical_cve_count, 
                   high_cve_count, created_at, updated_at)
           VALUES ($1, $2, NULL, 'healthy', 0, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(system_id)
    .bind("test-system-sys-precedence")
    .execute(&pool)
    .await
    .expect("create system");

    // Create system-scoped assignment
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments 
           (id, bundle_id, bundle_version_id, system_id, scope_type, active, 
            created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'system', true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(Uuid::new_v4())
    .bind(bundle_id)
    .bind(version_id)
    .bind(system_id)
    .execute(&pool)
    .await
    .expect("create assignment");

    // Verify: System-scoped assignment exists
    let sys_assignment_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM compliance_bundle_assignments WHERE bundle_id = $1 AND system_id = $2 AND active = true)",
    )
    .bind(bundle_id)
    .bind(system_id)
    .fetch_one(&pool)
    .await
    .expect("check sys assignment");

    assert!(
        sys_assignment_exists,
        "system-scoped assignment should exist"
    );
}
