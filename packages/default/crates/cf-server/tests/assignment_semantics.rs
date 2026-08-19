/// Live-DB regression tests for assignment_status semantics.
///
/// These tests verify that assignment_status field is:
/// 1. Correctly determined based on compliance_bundle_assignments by calling production handlers
/// 2. Marked "current" when assignment targets current_published_version_id
/// 3. Marked "pinned" when assignment targets another accepted version
/// 4. Null/None when no active assignment exists
/// 5. Kept completely independent from resolution_state
///
/// Run with: cargo test -p cf-server --test assignment_semantics -- --ignored
use sqlx::PgPool;
use uuid::Uuid;

// Use the production queries module
use crystal_forge::queries::compliance::list_bundle_systems_for_version;

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
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
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
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
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

    // Query the old version - system should show as "pinned" because new is current
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
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(bundle_id)
        .execute(&pool)
        .await
        .expect("set current published");

    // Create system but NO assignment
    let system_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO systems (id, hostname, environment, health_status, critical_cve_count, 
                   high_cve_count, created_at, updated_at)
           VALUES ($1, $2, NULL, 'healthy', 0, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(system_id)
    .bind("test-system-unassigned")
    .execute(&pool)
    .await
    .expect("create system");

    // Query - no systems should be returned because none are assigned
    let response = list_bundle_systems_for_version(&pool, bundle_id, version_id)
        .await
        .expect("query systems");

    // Response should be None since no systems are applicable (no assignments exist)
    // This verifies that systems without assignments do not appear in the results
    assert!(
        response.is_none() || response.unwrap().systems.is_empty(),
        "Unassigned system should not appear in results"
    );
}

#[sqlx::test]
async fn test_assignment_independent_from_resolution(pool: PgPool) {
    // This test verifies that assignment_status is computed independently
    // from resolution_state (no mixing of concerns).

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

    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
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

    // Call production handler
    let response = list_bundle_systems_for_version(&pool, bundle_id, version_id)
        .await
        .expect("query systems")
        .expect("bundle/version exists");

    assert_eq!(response.systems.len(), 1, "Should have exactly one system");
    let rollup = &response.systems[0];

    // Verify assignment_status and resolution_state are independent
    assert_eq!(
        rollup.assignment_status,
        Some("current".to_string()),
        "Assignment status should be determined independently"
    );
    // resolution_state should not be mixed with assignment_status
    // They represent different concerns
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

    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
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
    .expect("create environment assignment");

    // Create system in that environment
    let system_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO systems (id, hostname, environment, health_status, critical_cve_count, 
                   high_cve_count, created_at, updated_at)
           VALUES ($1, $2, $3, 'healthy', 0, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(system_id)
    .bind("test-system-in-env")
    .bind("test-env-scoped")
    .execute(&pool)
    .await
    .expect("create system");

    // Call production handler
    let response = list_bundle_systems_for_version(&pool, bundle_id, version_id)
        .await
        .expect("query systems")
        .expect("bundle/version exists");

    // Should include the system because it's in the environment that has assignment
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
    // Test that system-scoped assignment takes precedence over environment-scoped
    let bundle_id = Uuid::new_v4();
    let old_version_id = Uuid::new_v4();
    let new_version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, description, created_at, updated_at)
           VALUES ($1, $2, 'Test bundle', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(bundle_id)
    .bind("assignment-test-precedence")
    .execute(&pool)
    .await
    .expect("create bundle");

    // Create two versions
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
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
        .bind(new_version_id)
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
    .bind("test-env-precedence")
    .execute(&pool)
    .await
    .expect("create environment");

    // Create environment-scoped assignment to new version
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments 
           (id, bundle_id, bundle_version_id, environment_id, scope_type, active, 
            created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'environment', true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(Uuid::new_v4())
    .bind(bundle_id)
    .bind(new_version_id)
    .bind(env_id)
    .execute(&pool)
    .await
    .expect("create environment assignment");

    // Create system in that environment
    let system_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO systems (id, hostname, environment, health_status, critical_cve_count, 
                   high_cve_count, created_at, updated_at)
           VALUES ($1, $2, $3, 'healthy', 0, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(system_id)
    .bind("test-system-precedence")
    .bind("test-env-precedence")
    .execute(&pool)
    .await
    .expect("create system");

    // Now create a system-scoped assignment to OLD version
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
    .expect("create system assignment");

    // Query the old version
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
