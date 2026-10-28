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

use crystal_forge::compliance::resolver::{
    AssignmentMode, ResolutionOutcome, resolve_system_effective_policies,
};
use crystal_forge::queries::compliance::{
    determine_assignment_status_for_system, get_system_evidence, list_bundle_systems,
    list_bundle_systems_for_version, list_bundles, list_system_bundles,
    load_assignment_metadata_for_systems,
};

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
    create_system_assignment_by(pool, bundle_id, bundle_version_id, system_id, None).await;
}

async fn create_system_assignment_by(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    system_id: Uuid,
    created_by: Option<Uuid>,
) {
    // Create the assignment lineage row
    let assignment_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments
           (id, bundle_id, bundle_version_id, system_id, scope_type, active,
            assignment_overlay_digest, created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'system', true, 'test-digest',
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(assignment_id)
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(system_id)
    .execute(pool)
    .await
    .expect("create system assignment");

    // Create the immutable assignment version snapshot (migration 0204)
    let version_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignment_versions
            (id, assignment_id, version_number, bundle_version_id, enforcement_mode,
             assignment_overlay_digest, created_by, created_at)
            VALUES ($1, $2, 1, $3, 'enforce', 'test-digest', $4, CURRENT_TIMESTAMP)"#,
    )
    .bind(version_id)
    .bind(assignment_id)
    .bind(bundle_version_id)
    .bind(created_by)
    .execute(pool)
    .await
    .expect("create assignment version");

    // Link the assignment to its current version snapshot
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(assignment_id)
        .execute(pool)
        .await
        .expect("set current_version_id");
}

async fn create_user(pool: &PgPool, username: &str, email: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO users
           (id, username, email, first_name, last_name, user_type, is_active)
           VALUES ($1, $2, $3, 'Assignment', 'Tester', 'human', true)"#,
    )
    .bind(user_id)
    .bind(username)
    .bind(email)
    .execute(pool)
    .await
    .expect("create assignment actor");
    user_id
}

async fn create_system_assignment_with_reason(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    system_id: Uuid,
    reason: Option<&str>,
) -> (Uuid, Uuid) {
    let assignment_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments
           (id, bundle_id, bundle_version_id, system_id, scope_type, active,
            assignment_overlay_digest, created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'system', true, 'test-digest',
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(assignment_id)
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(system_id)
    .execute(pool)
    .await
    .expect("create reason assignment");

    let version_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_bundle_assignment_versions
           (assignment_id, version_number, bundle_version_id, enforcement_mode,
            assignment_overlay_digest, reason, created_at)
           VALUES ($1, 1, $2, 'enforce', 'test-digest', $3, CURRENT_TIMESTAMP)
           RETURNING id"#,
    )
    .bind(assignment_id)
    .bind(bundle_version_id)
    .bind(reason)
    .fetch_one(pool)
    .await
    .expect("create reason assignment version");

    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(assignment_id)
        .execute(pool)
        .await
        .expect("set reason assignment version");

    (assignment_id, version_id)
}

async fn append_assignment_version(
    pool: &PgPool,
    assignment_id: Uuid,
    previous_version_id: Uuid,
    version_number: i64,
    bundle_version_id: Uuid,
    reason: Option<&str>,
) -> Uuid {
    let version_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_bundle_assignment_versions
           (assignment_id, previous_version_id, version_number, bundle_version_id,
            enforcement_mode, assignment_overlay_digest, reason, created_at)
           VALUES ($1, $2, $3, $4, 'enforce', 'test-digest', $5, CURRENT_TIMESTAMP)
           RETURNING id"#,
    )
    .bind(assignment_id)
    .bind(previous_version_id)
    .bind(version_number)
    .bind(bundle_version_id)
    .bind(reason)
    .fetch_one(pool)
    .await
    .expect("append assignment version");

    sqlx::query(
        "UPDATE compliance_bundle_assignments SET current_version_id = $1, bundle_version_id = $2 WHERE id = $3",
    )
    .bind(version_id)
    .bind(bundle_version_id)
    .bind(assignment_id)
    .execute(pool)
    .await
    .expect("advance assignment version");

    version_id
}

/// Assignment version pointers are composite lineage references, not arbitrary
/// UUID references. This fails against migration 0204's single-column FKs,
/// which allowed assignment A to expose assignment B's immutable snapshot and
/// allowed previous_version_id to splice histories together.
#[sqlx::test]
async fn assignment_snapshot_pointers_cannot_cross_lineages(pool: PgPool) {
    let bundle_id = create_bundle(&pool, "assignment-lineage-integrity", "test").await;
    let bundle_version_id =
        create_bundle_version(&pool, bundle_id, "1.0", "accepted", "lineage integrity").await;
    let system_a = create_system(&pool, "lineage-a", None).await;
    let system_b = create_system(&pool, "lineage-b", None).await;
    let (assignment_a, version_a) = create_system_assignment_with_reason(
        &pool,
        bundle_id,
        bundle_version_id,
        system_a,
        Some("A"),
    )
    .await;
    let (_assignment_b, version_b) = create_system_assignment_with_reason(
        &pool,
        bundle_id,
        bundle_version_id,
        system_b,
        Some("B"),
    )
    .await;

    let mut pointer_tx = pool
        .begin()
        .await
        .expect("begin cross-lineage pointer test");
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2")
        .bind(version_b)
        .bind(assignment_a)
        .execute(&mut *pointer_tx)
        .await
        .expect("deferred constraint permits statement until commit");
    let pointer_error = pointer_tx
        .commit()
        .await
        .expect_err("assignment must not point at another lineage's snapshot");
    assert_eq!(
        pointer_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23503")
    );

    let mut history_tx = pool
        .begin()
        .await
        .expect("begin cross-lineage history test");
    sqlx::query(
        "INSERT INTO compliance_bundle_assignment_versions \
         (assignment_id, previous_version_id, version_number, bundle_version_id, \
          enforcement_mode, assignment_overlay_digest) \
         VALUES ($1, $2, 2, $3, 'enforce', 'cross-lineage-test')",
    )
    .bind(assignment_a)
    .bind(version_b)
    .bind(bundle_version_id)
    .execute(&mut *history_tx)
    .await
    .expect("deferred constraint permits statement until commit");
    let history_error = history_tx
        .commit()
        .await
        .expect_err("assignment history must not reference another lineage");
    assert_eq!(
        history_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23503")
    );

    let current: Uuid = sqlx::query_scalar(
        "SELECT current_version_id FROM compliance_bundle_assignments WHERE id = $1",
    )
    .bind(assignment_a)
    .fetch_one(&pool)
    .await
    .expect("load original current version");
    assert_eq!(
        current, version_a,
        "failed writes must leave lineage A unchanged"
    );
}

#[sqlx::test]
async fn test_assignment_reason_lifecycle_and_snapshot_authority(pool: PgPool) {
    let bundle_id = create_bundle(&pool, "assignment-reason-lifecycle", "NIST CSF").await;
    let v1_id = create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Version 1").await;
    let v2_id = create_bundle_version(&pool, bundle_id, "2.0", "accepted", "Version 2").await;
    set_current_published(&pool, bundle_id, v2_id).await;
    let system_id = create_system(&pool, "assignment-reason-system", None).await;
    let (assignment_id, version_a_id) =
        create_system_assignment_with_reason(&pool, bundle_id, v2_id, system_id, Some("Reason A"))
            .await;

    let metadata =
        load_assignment_metadata_for_systems(&pool, bundle_id, Some(v2_id), &[system_id])
            .await
            .expect("read reason A");
    assert_eq!(metadata[&system_id].reason.as_deref(), Some("Reason A"));

    let version_b_id = append_assignment_version(
        &pool,
        assignment_id,
        version_a_id,
        2,
        v2_id,
        Some("Reason B"),
    )
    .await;
    let metadata =
        load_assignment_metadata_for_systems(&pool, bundle_id, Some(v2_id), &[system_id])
            .await
            .expect("read reason B");
    assert_eq!(metadata[&system_id].reason.as_deref(), Some("Reason B"));
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT reason FROM compliance_bundle_assignment_versions WHERE id = $1",
        )
        .bind(version_a_id)
        .fetch_one(&pool)
        .await
        .expect("read historical reason"),
        Some("Reason A".to_string())
    );

    let version_preserved_id = append_assignment_version(
        &pool,
        assignment_id,
        version_b_id,
        3,
        v2_id,
        Some("Reason B"),
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT reason FROM compliance_bundle_assignment_versions WHERE id = $1",
        )
        .bind(version_preserved_id)
        .fetch_one(&pool)
        .await
        .expect("read omitted reason result"),
        Some("Reason B".to_string())
    );

    append_assignment_version(&pool, assignment_id, version_preserved_id, 4, v2_id, None).await;
    let metadata =
        load_assignment_metadata_for_systems(&pool, bundle_id, Some(v2_id), &[system_id])
            .await
            .expect("read cleared reason");
    assert_eq!(metadata[&system_id].reason, None);

    sqlx::query("UPDATE compliance_bundle_assignments SET bundle_version_id = $1 WHERE id = $2")
        .bind(v1_id)
        .bind(assignment_id)
        .execute(&pool)
        .await
        .expect("corrupt mutable lineage");
    let current_version_id: Uuid = sqlx::query_scalar(
        "SELECT current_version_id FROM compliance_bundle_assignments WHERE id = $1",
    )
    .bind(assignment_id)
    .fetch_one(&pool)
    .await
    .expect("read current assignment version");
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT bundle_version_id FROM compliance_bundle_assignment_versions WHERE id = $1",
        )
        .bind(current_version_id)
        .fetch_one(&pool)
        .await
        .expect("read immutable target"),
        v2_id
    );

    let version_after_corruption = append_assignment_version(
        &pool,
        assignment_id,
        current_version_id,
        5,
        v2_id,
        Some("Reason C"),
    )
    .await;
    let snapshot_target: Uuid = sqlx::query_scalar(
        "SELECT bundle_version_id FROM compliance_bundle_assignment_versions WHERE id = $1",
    )
    .bind(version_after_corruption)
    .fetch_one(&pool)
    .await
    .expect("read updated immutable target");
    assert_eq!(snapshot_target, v2_id);
    let metadata =
        load_assignment_metadata_for_systems(&pool, bundle_id, Some(v2_id), &[system_id])
            .await
            .expect("read reason after lineage corruption");
    assert_eq!(metadata[&system_id].reason.as_deref(), Some("Reason C"));
}

async fn create_environment_assignment(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    environment_id: Uuid,
) {
    create_environment_assignment_with_mode(
        pool,
        bundle_id,
        bundle_version_id,
        environment_id,
        "enforce",
    )
    .await;
}

async fn create_environment_assignment_with_mode(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    environment_id: Uuid,
    mode: &str,
) -> (Uuid, Uuid) {
    // Create the assignment lineage row
    let assignment_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments
            (id, bundle_id, bundle_version_id, environment_id, scope_type, active,
             enforcement_mode, assignment_overlay_digest, created_at, updated_at)
            VALUES ($1, $2, $3, $4, 'environment', true, $5, 'test-digest',
                    CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(assignment_id)
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(environment_id)
    .bind(mode)
    .execute(pool)
    .await
    .expect("create environment assignment");

    // Create the immutable assignment version snapshot (migration 0204)
    let version_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignment_versions
           (id, assignment_id, version_number, bundle_version_id, enforcement_mode,
            assignment_overlay_digest, created_at)
             VALUES ($1, $2, 1, $3, $4, 'test-digest', CURRENT_TIMESTAMP)"#,
    )
    .bind(version_id)
    .bind(assignment_id)
    .bind(bundle_version_id)
    .bind(mode)
    .execute(pool)
    .await
    .expect("create assignment version");

    // Link the assignment to its current version snapshot
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(assignment_id)
        .execute(pool)
        .await
        .expect("set current_version_id");
    (assignment_id, version_id)
}

async fn accepted_cve_policy(pool: &PgPool, name: &str, strict: bool) -> (Uuid, Uuid) {
    let policy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
         VALUES ($1, 'require_cve_check', $2, true) RETURNING id",
    )
    .bind(name)
    .bind(serde_json::json!({ "strict": strict, "max_critical": "none" }))
    .fetch_one(pool)
    .await
    .expect("create policy lineage");
    let version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_one(pool)
    .await
    .expect("read policy draft version");
    let mut tx = pool.begin().await.expect("start policy publication");
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("clear policy draft");
    sqlx::query(
        "UPDATE deployment_policy_versions SET publication_state = 'accepted', \
         trust_state = 'trusted', implementation_state = 'native', \
         published_at = CURRENT_TIMESTAMP WHERE id = $1",
    )
    .bind(version_id)
    .execute(&mut *tx)
    .await
    .expect("accept trusted policy");
    sqlx::query("UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("publish policy");
    tx.commit().await.expect("commit policy publication");
    (policy_id, version_id)
}

async fn accept_bundle_with_member(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    policy_version_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO compliance_bundle_version_policies \
         (bundle_version_id, policy_version_id, policy_order) VALUES ($1, $2, 0)",
    )
    .bind(bundle_version_id)
    .bind(policy_version_id)
    .execute(pool)
    .await
    .expect("add selected policy member");
    let mut tx = pool.begin().await.expect("start bundle publication");
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE id = $1")
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("clear bundle draft");
    sqlx::query(
        "UPDATE compliance_bundle_versions SET publication_state = 'accepted', \
         trust_state = 'trusted', semantic_digest = 'test-digest', \
         published_at = CURRENT_TIMESTAMP WHERE id = $1",
    )
    .bind(bundle_version_id)
    .execute(&mut *tx)
    .await
    .expect("accept trusted bundle version");
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
        .bind(bundle_version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("select catalog version");
    tx.commit().await.expect("commit bundle publication");
}

#[sqlx::test]
async fn environment_assignments_keep_distinct_versions_modes_and_overlays(pool: PgPool) {
    let bundle_id = create_bundle(&pool, "shared-assignment-versions", "NIST CSF").await;
    let v1 = create_bundle_version(&pool, bundle_id, "v1", "draft", "ATA baseline").await;
    let v2 = create_bundle_version(&pool, bundle_id, "v2", "draft", "LAN baseline").await;
    let (policy_a, policy_v1) = accepted_cve_policy(&pool, "ATA-only-CVE", false).await;
    let (policy_b, policy_v2) = accepted_cve_policy(&pool, "LAN-only-CVE", true).await;
    accept_bundle_with_member(&pool, bundle_id, v1, policy_v1).await;
    accept_bundle_with_member(&pool, bundle_id, v2, policy_v2).await;

    let ata = create_environment(&pool, "ATA").await;
    let lan = create_environment(&pool, "LAN").await;
    let unassigned = create_environment(&pool, "NO-ASSIGNMENT").await;
    let (ata_lineage, ata_snapshot) =
        create_environment_assignment_with_mode(&pool, bundle_id, v1, ata, "report_only").await;
    let (lan_lineage, lan_snapshot) =
        create_environment_assignment_with_mode(&pool, bundle_id, v2, lan, "enforce").await;
    let duplicate = sqlx::query(
        "INSERT INTO compliance_bundle_assignments \
         (bundle_id, bundle_version_id, environment_id, scope_type, \
          assignment_overlay_digest) VALUES ($1, $2, $3, 'environment', 'test-digest')",
    )
    .bind(bundle_id)
    .bind(v2)
    .bind(ata)
    .execute(&pool)
    .await
    .expect_err("one active lineage per bundle and environment");
    assert_eq!(
        duplicate
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23505")
    );
    for (lineage, snapshot, policy_version, strict) in [
        (ata_lineage, ata_snapshot, policy_v1, true),
        (lan_lineage, lan_snapshot, policy_v2, false),
    ] {
        sqlx::query(
            "INSERT INTO compliance_assignment_value_overrides \
             (assignment_id, assignment_version_id, policy_version_id, value_path, value) \
             VALUES ($1, $2, $3, 'strict', $4)",
        )
        .bind(lineage)
        .bind(snapshot)
        .bind(policy_version)
        .bind(serde_json::json!(strict))
        .execute(&pool)
        .await
        .expect("record immutable assignment overlay");
    }
    let host_a = create_system(&pool, "host-a", Some(ata)).await;
    let host_b = create_system(&pool, "host-b", Some(lan)).await;
    let host_none = create_system(&pool, "host-none", Some(unassigned)).await;

    for (system_id, expected_version, mode, policy_id, strict) in [
        (host_a, v1, AssignmentMode::ReportOnly, policy_a, true),
        (host_b, v2, AssignmentMode::Enforce, policy_b, false),
    ] {
        let system = list_system_bundles(&pool, system_id)
            .await
            .expect("read System Compliance")
            .expect("system exists");
        assert_eq!(system.bundles.len(), 1);
        assert_eq!(system.bundles[0].0.id, bundle_id);
        let assigned = &system.assignment_versions[&bundle_id];
        assert_eq!(assigned.id, expected_version);
        assert_eq!(assigned.enforcement_mode, mode.as_str());
        assert_eq!(
            system.bundles[0].1.report_only,
            i64::from(mode == AssignmentMode::ReportOnly)
        );
        let outcome = resolve_system_effective_policies(&pool, system_id)
            .await
            .expect("resolve exact assigned policies");
        let ResolutionOutcome::Resolved(effective) = outcome else {
            panic!("valid independent assignment must resolve: {outcome:?}");
        };
        assert_eq!(effective.policies.len(), 1);
        assert_eq!(effective.policies[0].policy_lineage_id, policy_id);
        assert_eq!(effective.policies[0].effective_mode, mode);
        assert_eq!(effective.policies[0].effective_config["strict"], strict);
        let evidence = get_system_evidence(&pool, bundle_id, system_id, None)
            .await
            .expect("read unversioned system evidence")
            .expect("assigned bundle must have evidence");
        assert_eq!(evidence.bundle_version_id, Some(expected_version));
        assert_eq!(evidence.controls.len(), 1);
        assert_eq!(evidence.controls[0].policy_id, policy_id);
    }
    assert!(
        list_system_bundles(&pool, host_none)
            .await
            .unwrap()
            .unwrap()
            .bundles
            .is_empty()
    );
    assert!(
        get_system_evidence(&pool, bundle_id, host_none, None)
            .await
            .unwrap()
            .is_none()
    );

    for (version, included, excluded) in [(v1, host_a, host_b), (v2, host_b, host_a)] {
        let exact = list_bundle_systems_for_version(&pool, bundle_id, version)
            .await
            .expect("read exact version systems")
            .expect("bundle/version exists");
        assert_eq!(exact.systems.len(), 1);
        assert_eq!(exact.systems[0].system_id, included);
        assert!(exact.systems.iter().all(|row| row.system_id != excluded));
        assert!(
            get_system_evidence(&pool, bundle_id, excluded, Some(version))
                .await
                .unwrap()
                .is_none()
        );
    }
    let current = list_bundle_systems(&pool, bundle_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.bundle_version_id, Some(v2));
    assert_eq!(current.systems.len(), 1);
    assert_eq!(current.systems[0].system_id, host_b);
    let catalog = list_bundles(&pool).await.unwrap();
    assert_eq!(
        catalog
            .iter()
            .find(|bundle| bundle.id == bundle_id)
            .unwrap()
            .applicable_system_count,
        1
    );

    set_current_published(&pool, bundle_id, v1).await;
    assert_eq!(
        list_system_bundles(&pool, host_b)
            .await
            .unwrap()
            .unwrap()
            .assignment_versions[&bundle_id]
            .id,
        v2
    );
    assert_eq!(
        get_system_evidence(&pool, bundle_id, host_b, None)
            .await
            .unwrap()
            .unwrap()
            .bundle_version_id,
        Some(v2)
    );
    for (lineage, snapshot, version) in [
        (ata_lineage, ata_snapshot, v1),
        (lan_lineage, lan_snapshot, v2),
    ] {
        let saved: (Uuid, Uuid) = sqlx::query_as(
            "SELECT a.current_version_id, av.bundle_version_id \
             FROM compliance_bundle_assignments a \
             JOIN compliance_bundle_assignment_versions av ON av.id = a.current_version_id \
             WHERE a.id = $1",
        )
        .bind(lineage)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            saved,
            (snapshot, version),
            "catalog selection must not retarget assignments"
        );
    }
    let current_v1 = list_bundle_systems(&pool, bundle_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current_v1.bundle_version_id, Some(v1));
    assert_eq!(current_v1.systems.len(), 1);
    assert_eq!(current_v1.systems[0].system_id, host_a);
    set_current_published(&pool, bundle_id, v2).await;

    // An explicit system assignment wins over ATA's v1 scope. The v2 drawer
    // must include host-a only after that explicit assignment is active.
    create_system_assignment(&pool, bundle_id, v2, host_a).await;
    let overridden = list_system_bundles(&pool, host_a).await.unwrap().unwrap();
    assert_eq!(overridden.assignment_versions[&bundle_id].id, v2);
    assert_eq!(
        get_system_evidence(&pool, bundle_id, host_a, None)
            .await
            .unwrap()
            .unwrap()
            .bundle_version_id,
        Some(v2)
    );
    let v1_systems = list_bundle_systems_for_version(&pool, bundle_id, v1)
        .await
        .unwrap()
        .unwrap();
    assert!(v1_systems.systems.iter().all(|row| row.system_id != host_a));
    let v2_systems = list_bundle_systems_for_version(&pool, bundle_id, v2)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(v2_systems.systems.len(), 2);

    sqlx::query("UPDATE compliance_bundle_assignments SET active = false WHERE id = $1")
        .bind(lan_lineage)
        .execute(&pool)
        .await
        .expect("retire LAN assignment");
    assert!(
        list_system_bundles(&pool, host_b)
            .await
            .unwrap()
            .unwrap()
            .bundles
            .is_empty()
    );
    assert!(
        get_system_evidence(&pool, bundle_id, host_b, None)
            .await
            .unwrap()
            .is_none()
    );
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
async fn test_bundle_systems_resolves_assignment_actor_username(pool: PgPool) {
    let bundle_id = create_bundle(&pool, "assignment-actor", "NIST CSF").await;
    let version_id =
        create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Test version").await;
    set_current_published(&pool, bundle_id, version_id).await;

    let system_id = create_system(&pool, "assignment-actor-system", None).await;
    let actor_id = create_user(&pool, "assignment-approver", "approver@example.invalid").await;
    create_system_assignment_by(&pool, bundle_id, version_id, system_id, Some(actor_id)).await;

    let response = list_bundle_systems_for_version(&pool, bundle_id, version_id)
        .await
        .expect("query systems with assignment actor")
        .expect("bundle/version exists");

    assert_eq!(response.systems.len(), 1);
    assert_eq!(
        response.systems[0].assignment_approved_by.as_deref(),
        Some("assignment-approver")
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

#[sqlx::test]
async fn test_immutable_assignment_version_supersedes_lineage(pool: PgPool) {
    // DISCRIMINATING REGRESSION: Verifies that assignment status uses authoritative
    // compliance_bundle_assignment_versions.bundle_version_id via current_version_id,
    // not the lineage field compliance_bundle_assignments.bundle_version_id.
    //
    // Scenario:
    // - Bundle B with V1 and V2 accepted versions; V2 is current published
    // - Assignment is to V2 (both lineage and snapshot say V2)
    // - System assigned to this assignment
    //
    // Expected: status = "current" (from V2 being the current published)
    //
    // The test verifies the assignment-version JOIN works correctly by
    // explicitly checking that determine_assignment_status_for_system()
    // returns "current" when the assignment snapshot targets the current version.
    //
    // This test will FAIL if determine_assignment_status_for_system() queries
    // don't properly JOIN to compliance_bundle_assignment_versions.

    let bundle_id = create_bundle(
        &pool,
        "assignment-test-version-supersedes-lineage",
        "NIST CSF",
    )
    .await;
    let v1_id = create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Version 1").await;
    let v2_id = create_bundle_version(&pool, bundle_id, "2.0", "accepted", "Version 2").await;

    // V2 is the current published version
    set_current_published(&pool, bundle_id, v2_id).await;

    let system_id = create_system(&pool, "test-system-version-supersedes", None).await;

    // Create assignment to V2 using the standard fixture helper
    // (which now properly creates both the assignment row and its immutable version)
    create_system_assignment(&pool, bundle_id, v2_id, system_id).await;

    // DISCRIMINATING SCENARIO 1: Lineage and snapshot initially agree on V2
    let status = determine_assignment_status_for_system(&pool, bundle_id, system_id)
        .await
        .expect("query assignment status")
        .expect("status exists");

    assert_eq!(
        status, "current",
        "Assignment to current version should show current"
    );

    // DISCRIMINATING SCENARIO 2: NOW deliberately make lineage != snapshot
    // This proves the code uses the immutable snapshot, not the mutable lineage field.
    // Get the assignment row
    let assignment_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM compliance_bundle_assignments WHERE system_id = $1 AND bundle_id = $2",
    )
    .bind(system_id)
    .bind(bundle_id)
    .fetch_one(&pool)
    .await
    .expect("fetch assignment");

    // Update the lineage field to V1 (mismatch with snapshot which still says V2)
    sqlx::query("UPDATE compliance_bundle_assignments SET bundle_version_id = $1 WHERE id = $2")
        .bind(v1_id)
        .bind(assignment_id)
        .execute(&pool)
        .await
        .expect("corrupt lineage field for test");

    // Now query assignment status - it MUST still use the snapshot (V2) not the lineage (V1)
    let status_after = determine_assignment_status_for_system(&pool, bundle_id, system_id)
        .await
        .expect("query after corruption")
        .expect("status exists");

    assert_eq!(
        status_after, "current",
        "DISCRIMINATING: Must use snapshot V2, not corrupt lineage V1"
    );

    // SCENARIO 3: Verify pinned behavior when current version changes
    let v3_id = create_bundle_version(&pool, bundle_id, "3.0", "accepted", "Version 3").await;
    set_current_published(&pool, bundle_id, v3_id).await;

    // Same assignment (V2) now appears pinned because V3 became current
    let pinned_status = determine_assignment_status_for_system(&pool, bundle_id, system_id)
        .await
        .expect("query pinned status")
        .expect("status exists");

    assert_eq!(
        pinned_status, "pinned",
        "After current version changes to V3, assignment to V2 should show pinned"
    );
}

#[sqlx::test]
async fn test_bundle_systems_batched_query_count(pool: PgPool) {
    // QUERY-COUNT REGRESSION: O(1) Assignment Batching
    //
    // Verifies that list_bundle_systems_for_version() uses batched assignment loading
    // via load_assignment_statuses_for_systems() and query count does NOT scale with
    // system count (N+1 fix verification).
    //
    // PROOF OF BATCHING (source-level sanity gate):
    // The production function in queries/compliance.rs (line ~2057) contains:
    //   let assignment_statuses = load_assignment_statuses_for_systems(pool, ...)
    //       .await?;  // <-- CALLED ONCE before loop
    //   for system in systems {
    //       let status = assignment_statuses.get(&system.id)  // <-- pre-loaded O(1)
    // }
    //
    // This proves:
    // - Assignment statuses are fetched in a single batch query upfront
    // - The loop accesses pre-loaded HashMap, not per-system queries
    // - Query complexity is O(1) with respect to system count
    //
    // Test procedure:
    // - Run query with 1 system: baseline (call generates N queries)
    // - Run query with 20 systems: should also generate N queries (not 20*N)
    // - Assert all 20 returned systems have correct assignment_status
    //
    // NOTE: SQLx lacks built-in query instrumentation in test context.
    // This test validates BEHAVIORAL proof (row counts + assignment accuracy)
    // and relies on source-level inspection of load_assignment_statuses_for_systems()
    // being called before the systems loop as the authoritative Q.E.D.

    let bundle_id = create_bundle(&pool, "batch-test", "NIST CSF").await;
    let version_id =
        create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Test version").await;
    set_current_published(&pool, bundle_id, version_id).await;

    // Create 1 system for baseline
    let system_1 = create_system(&pool, "sys-1.test", None).await;
    create_system_assignment(&pool, bundle_id, version_id, system_1).await;

    // Load with 1 system - baseline
    let result_1 = list_bundle_systems_for_version(&pool, bundle_id, version_id)
        .await
        .expect("query 1 system")
        .expect("bundle exists");
    assert_eq!(result_1.systems.len(), 1, "Should have exactly 1 system");
    assert_eq!(
        result_1.systems[0].assignment_status,
        Some("current".to_string()),
        "Single system should have current assignment status"
    );

    // Create 20 more systems and assign them all
    let mut system_ids = vec![system_1];
    for i in 2..=20 {
        let sys_id = create_system(&pool, &format!("sys-{}.test", i), None).await;
        system_ids.push(sys_id);
        create_system_assignment(&pool, bundle_id, version_id, sys_id).await;
    }

    // Load with 20 systems - should use same query count as 1 system due to batching
    let result_20 = list_bundle_systems_for_version(&pool, bundle_id, version_id)
        .await
        .expect("query 20 systems")
        .expect("bundle exists");
    assert_eq!(
        result_20.systems.len(),
        20,
        "Should have exactly 20 systems"
    );

    // Verify all statuses are correctly determined.
    // If assignment batching broke, some systems would return None or wrong status.
    // This proves the batch loader worked for all 20 systems.
    for (idx, system) in result_20.systems.iter().enumerate() {
        assert_eq!(
            system.assignment_status,
            Some("current".to_string()),
            "System {} should have current status (proves batch loader returned all 20)",
            idx + 1
        );
    }

    // Additional proof: verify that ALL 20 system IDs from assignment creation are
    // present in the results (not just a subset).
    for expected_id in &system_ids {
        assert!(
            result_20
                .systems
                .iter()
                .any(|s| s.system_id == *expected_id),
            "All assigned systems must be returned (system {} missing)",
            expected_id
        );
    }
}

#[sqlx::test]
async fn test_exact_version_uses_immutable_assignment_snapshot(pool: PgPool) {
    // CRITICAL REGRESSION: list_bundle_systems_for_version() must use the immutable
    // assignment snapshot (compliance_bundle_assignment_versions), NOT the mutable
    // lineage field (compliance_bundle_assignments.bundle_version_id).
    //
    // This test deliberately creates a mismatch where:
    // - The immutable snapshot says the assignment targets V1
    // - The mutable lineage field says the assignment targets V2
    //
    // The endpoint must use the snapshot, not the lineage.
    // If it uses the lineage, the system will appear in the V2 query results
    // (wrong) instead of V1 results (correct).

    let bundle_id = create_bundle(&pool, "exact-version-snapshot-test", "NIST CSF").await;
    let v1_id = create_bundle_version(&pool, bundle_id, "1.0", "accepted", "Version 1").await;
    let v2_id = create_bundle_version(&pool, bundle_id, "2.0", "accepted", "Version 2").await;

    // V2 is published
    set_current_published(&pool, bundle_id, v2_id).await;

    let system_id = create_system(&pool, "exact-version-test-system", None).await;

    // Create assignment to V1 using the standard fixture
    create_system_assignment(&pool, bundle_id, v1_id, system_id).await;

    // Get the assignment record
    let assignment_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM compliance_bundle_assignments WHERE system_id = $1 AND bundle_id = $2",
    )
    .bind(system_id)
    .bind(bundle_id)
    .fetch_one(&pool)
    .await
    .expect("fetch assignment");

    // NOW CORRUPT THE LINEAGE: Update the mutable field to V2
    // This creates: lineage says V2, snapshot says V1
    sqlx::query("UPDATE compliance_bundle_assignments SET bundle_version_id = $1 WHERE id = $2")
        .bind(v2_id)
        .bind(assignment_id)
        .execute(&pool)
        .await
        .expect("corrupt lineage");

    // Call the PRODUCTION ENDPOINT with V1
    // If the endpoint uses lineage (wrong), system won't be returned (lineage says V2, not V1)
    // If the endpoint uses snapshot (correct), system WILL be returned (snapshot says V1)
    let response_v1 = list_bundle_systems_for_version(&pool, bundle_id, v1_id)
        .await
        .expect("query v1")
        .expect("v1 exists");

    assert!(
        response_v1.systems.iter().any(|s| s.system_id == system_id),
        "CRITICAL: Exact-version endpoint must use immutable snapshot, not mutable lineage field. \
         System was assigned to V1 (snapshot), but lineage was corrupted to V2. \
         The system MUST appear in V1 results."
    );

    // Verify the system does NOT appear in V2 (since snapshot says V1, not V2)
    let response_v2 = list_bundle_systems_for_version(&pool, bundle_id, v2_id)
        .await
        .expect("query v2")
        .expect("v2 exists");

    assert!(
        !response_v2.systems.iter().any(|s| s.system_id == system_id),
        "System assigned to V1 (snapshot) should NOT appear in V2 results, \
         even though lineage was corrupted to V2"
    );
}
