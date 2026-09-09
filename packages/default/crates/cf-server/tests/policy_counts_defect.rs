//! Live-DB regressions for PolicyCard counts, PolicyDrawer owner and scoped
//! usage hydration, and Evidence for ATO.
//!
//! These tests invoke the production query functions that power the policy
//! management UI:
//! - `load_policy_version_usage_counts` computes mapped_requirement_count and
//!   bundle_usage_count from policy_requirement_mappings /
//!   compliance_bundle_version_policies for exact version IDs (not hardcoded).
//! - `fetch_policy_version_summaries` decodes created_by_display from the users
//!   join and evidence_specs from compliance_metadata.
//!
//! Run with: cargo test -p cf-server --test policy_counts_defect -- --test-threads=1
use axum::{Router, routing::get};
use chrono::Utc;
use crystal_forge::api::models::PolicyVersionUsageResponse;
use crystal_forge::auth::session::{SESSION_COOKIE_NAME, hash_token};
use crystal_forge::compliance::resolver::{
    ResolutionOutcome, resolve_systems_effective_policies_for_bundle_versions_batch,
};
use crystal_forge::handlers::api::compliance::get_policy_version_usage;
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::queries::auth_identity::{create_user_session, sync_user_role};
use crystal_forge::queries::users::insert_user;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crystal_forge::queries::deployment_policies::{
    fetch_policy_version_summaries, get_deployment_policies_by_versions,
    load_policy_version_usage_counts,
};

async fn role_session(pool: &PgPool, role: AuthRole, label: &str) -> (Uuid, String) {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("pu-{}@example.invalid", &suffix[..8]),
        Some(label),
    )
    .await
    .expect("create policy usage user");
    sqlx::query("UPDATE users SET username=$2 WHERE id=$1")
        .bind(user.id)
        .bind(label)
        .execute(pool)
        .await
        .expect("set policy usage display name");
    sync_user_role(pool, user.id, role)
        .await
        .expect("assign policy usage role");
    let token = format!("policy-usage-session-{suffix}");
    create_user_session(
        pool,
        user.id,
        hash_token(&token),
        Utc::now() + chrono::Duration::hours(1),
        Some("policy-usage-regression".into()),
        Some("127.0.0.1".into()),
        "local".into(),
    )
    .await
    .expect("create policy usage session");
    (user.id, token)
}

async fn usage_server(pool: PgPool) -> String {
    let app = Router::new()
        .route(
            "/api/v1/policy-versions/:version_id/usage",
            get(get_policy_version_usage),
        )
        .with_state(pool);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind policy usage server");
    let address = listener.local_addr().expect("read policy usage address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve policy usage API")
    });
    format!("http://{address}")
}

/// Helper to create a policy with a version and return their IDs.
async fn create_test_policy(pool: &PgPool, name: &str) -> (Uuid, Uuid) {
    let policy_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO deployment_policies 
           (id, name, policy_type, config, enabled)
           VALUES ($1, $2, 'require_packages', '{}', true)"#,
    )
    .bind(policy_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("create policy");

    sqlx::query(
        r#"INSERT INTO deployment_policy_versions 
           (id, policy_id, version, publication_state, name, policy_type, config,
            compliance_metadata, semantic_digest, created_at)
           VALUES ($1, $2, '1.0', 'draft', $3, 'require_packages', '{}', '{}',
                   'sha256-test', CURRENT_TIMESTAMP)"#,
    )
    .bind(version_id)
    .bind(policy_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("create version");

    (policy_id, version_id)
}

/// Helper to create a framework, requirement, and requirement version.
/// Returns the requirement_version_id usable in policy_requirement_mappings.
async fn create_test_requirement(pool: &PgPool, framework_name: &str) -> Uuid {
    let framework_id = Uuid::new_v4();
    let framework_version_id = Uuid::new_v4();
    let requirement_id = Uuid::new_v4();
    let req_version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_frameworks (id, name, canonical_source_key)
           VALUES ($1, $2, $3)"#,
    )
    .bind(framework_id)
    .bind(framework_name)
    .bind(format!("canonical-{framework_name}"))
    .execute(pool)
    .await
    .expect("create framework");

    sqlx::query(
        r#"INSERT INTO compliance_framework_versions
           (id, framework_id, version, canonical_release_key, title)
           VALUES ($1, $2, '1.0', $3, $4)"#,
    )
    .bind(framework_version_id)
    .bind(framework_id)
    .bind(format!("canonical-release-{framework_name}"))
    .bind(format!("{framework_name} 1.0"))
    .execute(pool)
    .await
    .expect("create framework version");

    sqlx::query(
        r#"INSERT INTO compliance_requirements (id, framework_id, canonical_requirement_key)
           VALUES ($1, $2, $3)"#,
    )
    .bind(requirement_id)
    .bind(framework_id)
    .bind(format!("REQ-{framework_name}"))
    .execute(pool)
    .await
    .expect("create requirement");

    sqlx::query(
        r#"INSERT INTO compliance_requirement_versions 
           (id, requirement_id, framework_version_id, external_id, title, kind)
           VALUES ($1, $2, $3, $4, $5, 'control')"#,
    )
    .bind(req_version_id)
    .bind(requirement_id)
    .bind(framework_version_id)
    .bind(format!("EXT-{framework_name}"))
    .bind(format!("Requirement {framework_name}"))
    .execute(pool)
    .await
    .expect("create requirement version");

    req_version_id
}

/// Helper to create a policy-requirement mapping (trusted, so it is counted).
async fn create_test_mapping(pool: &PgPool, policy_version_id: Uuid, requirement_version_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO policy_requirement_mappings 
           (policy_version_id, requirement_version_id, relationship, coverage, 
            provenance, trust_state)
           VALUES ($1, $2, 'implements', 'full', 'manual', 'trusted')"#,
    )
    .bind(policy_version_id)
    .bind(requirement_version_id)
    .execute(pool)
    .await
    .expect("create mapping");
}

/// Helper to create a bundle and add a policy selection.
async fn create_test_bundle_with_policy(
    pool: &PgPool,
    bundle_name: &str,
    policy_version_id: Uuid,
) -> Uuid {
    let bundle_id = Uuid::new_v4();
    let bundle_version_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, framework, version, layer, owner)
           VALUES ($1, $2, 'NIST CSF', '1.0', 'fleet', 'Platform Security')"#,
    )
    .bind(bundle_id)
    .bind(bundle_name)
    .execute(pool)
    .await
    .expect("create bundle");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions 
           (id, bundle_id, version, publication_state, name, framework, framework_version,
            description, layer, owner, semantic_digest)
           VALUES ($1, $2, '1.0', 'draft', $3, 'NIST CSF', '1.0',
                   'bundle version', 'fleet', 'Platform Security', 'sha256-test')"#,
    )
    .bind(bundle_version_id)
    .bind(bundle_id)
    .bind(bundle_name)
    .execute(pool)
    .await
    .expect("create bundle version");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_version_policies 
           (bundle_version_id, policy_version_id, selected, policy_order)
           VALUES ($1, $2, true, 1)"#,
    )
    .bind(bundle_version_id)
    .bind(policy_version_id)
    .execute(pool)
    .await
    .expect("create bundle policy selection");

    bundle_version_id
}

async fn create_usage_assignment(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    system_id: Uuid,
) {
    let assignment_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments
           (id, bundle_id, bundle_version_id, system_id, scope_type, active,
            enforcement_mode, assignment_overlay_digest)
           VALUES ($1, $2, $3, $4, 'system', true, 'enforce', 'usage-scope')"#,
    )
    .bind(assignment_id)
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(system_id)
    .execute(pool)
    .await
    .expect("create usage assignment");
    let assignment_version_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_bundle_assignment_versions
           (assignment_id, version_number, bundle_version_id, enforcement_mode,
            assignment_overlay_digest)
           VALUES ($1, 1, $2, 'enforce', 'usage-scope')
           RETURNING id"#,
    )
    .bind(assignment_id)
    .bind(bundle_version_id)
    .fetch_one(pool)
    .await
    .expect("create immutable usage assignment version");
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id=$2 WHERE id=$1")
        .bind(assignment_id)
        .bind(assignment_version_id)
        .execute(pool)
        .await
        .expect("set current usage assignment version");
}

async fn create_usage_scope_fixture(
    pool: &PgPool,
    created_by: Uuid,
) -> (Uuid, Uuid, Uuid, Uuid, Uuid) {
    let (policy_id, policy_version_id) = create_test_policy(pool, "policy-drawer-scope").await;
    // INVARIANT: Acceptance and the lineage pointer must commit atomically.
    let mut tx = pool.begin().await.expect("begin policy publication");
    sqlx::query(
        "UPDATE deployment_policy_versions
         SET created_by=$2, publication_state='accepted', trust_state='trusted'
         WHERE id=$1",
    )
    .bind(policy_version_id)
    .bind(created_by)
    .execute(&mut *tx)
    .await
    .expect("attribute policy version owner");
    sqlx::query("UPDATE deployment_policies SET current_published_version_id=$2 WHERE id=$1")
        .bind(policy_id)
        .bind(policy_version_id)
        .execute(&mut *tx)
        .await
        .expect("publish policy version");
    tx.commit().await.expect("commit policy publication");

    let visible_environment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name,description) VALUES($1,'visible') RETURNING id",
    )
    .bind(format!("visible-{}", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .expect("create visible environment");
    let hidden_environment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name,description) VALUES($1,'hidden') RETURNING id",
    )
    .bind(format!("hidden-{}", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .expect("create hidden environment");

    let visible_system_id = Uuid::new_v4();
    let hidden_system_id = Uuid::new_v4();
    for (system_id, hostname, environment_id) in [
        (
            visible_system_id,
            format!("visible-{}.example.invalid", Uuid::new_v4()),
            visible_environment_id,
        ),
        (
            hidden_system_id,
            format!("hidden-{}.example.invalid", Uuid::new_v4()),
            hidden_environment_id,
        ),
    ] {
        sqlx::query(
            r#"INSERT INTO systems
               (id,hostname,environment_id,is_active,public_key,derivation,reachability)
               VALUES($1,$2,$3,true,$4,$4,'direct')"#,
        )
        .bind(system_id)
        .bind(hostname)
        .bind(environment_id)
        .bind(format!("usage-key-{system_id}"))
        .execute(pool)
        .await
        .expect("create usage system");
    }

    let bundle_id = Uuid::new_v4();
    let bundle_version_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO compliance_bundles(id,name,framework,version,layer,owner)
           VALUES($1,$2,'Scope','1.0','fleet','Security')"#,
    )
    .bind(bundle_id)
    .bind(format!("policy-usage-bundle-{}", Uuid::new_v4()))
    .execute(pool)
    .await
    .expect("create usage bundle");
    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions
           (id,bundle_id,version,publication_state,trust_state,name,framework,
            framework_version,description,layer,owner,semantic_digest)
            VALUES($1,$2,'1.0','draft','trusted','Policy usage','Scope','1.0',
                  'usage','fleet','Security','usage-bundle')"#,
    )
    .bind(bundle_version_id)
    .bind(bundle_id)
    .execute(pool)
    .await
    .expect("create usage bundle version");
    sqlx::query(
        "INSERT INTO compliance_bundle_version_policies
         (bundle_version_id,policy_version_id,selected,policy_order)
         VALUES($1,$2,true,1)",
    )
    .bind(bundle_version_id)
    .bind(policy_version_id)
    .execute(pool)
    .await
    .expect("select policy in usage bundle");
    create_usage_assignment(pool, bundle_id, bundle_version_id, visible_system_id).await;
    create_usage_assignment(pool, bundle_id, bundle_version_id, hidden_system_id).await;
    // INVARIANT: The final digest, acceptance, and lineage pointer must commit
    // atomically after draft membership is complete.
    let mut tx = pool.begin().await.expect("begin bundle publication");
    sqlx::query(
        "UPDATE compliance_bundle_versions
         SET publication_state='accepted', semantic_digest='usage-bundle-final'
         WHERE id=$1",
    )
    .bind(bundle_version_id)
    .execute(&mut *tx)
    .await
    .expect("accept usage bundle version");
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id=$2 WHERE id=$1")
        .bind(bundle_id)
        .bind(bundle_version_id)
        .execute(&mut *tx)
        .await
        .expect("publish usage bundle version");
    tx.commit().await.expect("commit bundle publication");

    let outcomes = resolve_systems_effective_policies_for_bundle_versions_batch(
        pool,
        &[(bundle_version_id, vec![visible_system_id, hidden_system_id])],
    )
    .await
    .expect("resolve usage fixture assignments");
    for system_id in [visible_system_id, hidden_system_id] {
        assert!(
            matches!(
                outcomes.get(&(bundle_version_id, system_id)),
                Some(ResolutionOutcome::Resolved(_))
            ),
            "usage fixture assignment must resolve: {:?}",
            outcomes.get(&(bundle_version_id, system_id))
        );
    }

    (
        policy_id,
        policy_version_id,
        visible_environment_id,
        visible_system_id,
        hidden_system_id,
    )
}

#[sqlx::test]
async fn test_policy_counts_non_zero_for_mapped_requirements(pool: PgPool) {
    // Create a policy with 3 trusted requirements mapped to it
    let (_policy_id, policy_version_id) = create_test_policy(&pool, "mapped-req-test").await;

    let req1 = create_test_requirement(&pool, "fw-a").await;
    let req2 = create_test_requirement(&pool, "fw-b").await;
    let req3 = create_test_requirement(&pool, "fw-c").await;

    create_test_mapping(&pool, policy_version_id, req1).await;
    create_test_mapping(&pool, policy_version_id, req2).await;
    create_test_mapping(&pool, policy_version_id, req3).await;

    // Call the production batched loader used by the handler.
    let counts = load_policy_version_usage_counts(&pool, &[policy_version_id])
        .await
        .expect("production count loader failed");
    let (mapped, bundles) = counts
        .get(&policy_version_id)
        .copied()
        .expect("counts present for version");

    assert_eq!(
        mapped, 3,
        "mapped_requirement_count should be 3 from policy_requirement_mappings (trusted)"
    );
    assert_eq!(
        bundles, 0,
        "bundle_usage_count should be 0 when policy is not in any bundle"
    );

    // The version->record loader surfaces the same real counts.
    let records = get_deployment_policies_by_versions(&pool, &[policy_version_id])
        .await
        .expect("production version loader failed");
    let record = records
        .get(&policy_version_id)
        .expect("record present for version");
    assert_eq!(
        record.mapped_requirement_count, 3,
        "DeploymentPolicyRecord should carry real mapped_requirement_count"
    );
}

#[sqlx::test]
async fn test_policy_counts_zero_no_mappings(pool: PgPool) {
    let (_policy_id, policy_version_id) = create_test_policy(&pool, "no-mappings-test").await;

    let counts = load_policy_version_usage_counts(&pool, &[policy_version_id])
        .await
        .expect("production count loader failed");
    let (mapped, bundles) = counts
        .get(&policy_version_id)
        .copied()
        .expect("counts present for version");

    assert_eq!(
        mapped, 0,
        "mapped_requirement_count should be 0 when no trusted mappings exist"
    );
    assert_eq!(
        bundles, 0,
        "bundle_usage_count should be 0 when policy is in no bundle"
    );
}

#[sqlx::test]
async fn test_bundle_usage_count_distinct_bundles(pool: PgPool) {
    let (_policy_id, policy_version_id) = create_test_policy(&pool, "bundle-usage-test").await;

    // Two distinct bundle versions must each count toward bundle_usage_count.
    create_test_bundle_with_policy(&pool, "bundle-one", policy_version_id).await;
    create_test_bundle_with_policy(&pool, "bundle-two", policy_version_id).await;

    let counts = load_policy_version_usage_counts(&pool, &[policy_version_id])
        .await
        .expect("production count loader failed");
    let (mapped, bundles) = counts
        .get(&policy_version_id)
        .copied()
        .expect("counts present for version");

    assert_eq!(
        bundles, 2,
        "bundle_usage_count should be 2 from compliance_bundle_version_policies (distinct bundles)"
    );
    assert_eq!(mapped, 0, "no requirements mapped in this test");
}

#[sqlx::test]
async fn test_defect_3_created_by_display_shows_username(pool: PgPool) {
    // Defect 3: PolicyDrawer Owner should show display name instead of UUID.
    let user_id = Uuid::new_v4();
    let username = "test-creator";

    sqlx::query(
        "INSERT INTO users (id, username, email, first_name, last_name, user_type, is_active) \
         VALUES ($1, $2, $3, $4, $5, 'human', true)",
    )
    .bind(user_id)
    .bind(username)
    .bind("test@example.com")
    .bind("Test")
    .bind("Creator")
    .execute(&pool)
    .await
    .expect("create user");

    let (policy_id, version_id) = create_test_policy(&pool, "creator-test-policy").await;

    // Attach created_by to the version so the users join has something to resolve.
    sqlx::query("UPDATE deployment_policy_versions SET created_by = $1 WHERE id = $2")
        .bind(user_id)
        .bind(version_id)
        .execute(&pool)
        .await
        .expect("set created_by");

    // Call the production summary loader used by the handler.
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("production summary loader failed");
    let versions = summaries.get(&policy_id).expect("policy summaries present");

    let test_version = versions
        .iter()
        .find(|v| v.id == version_id)
        .expect("test version present in summaries");

    assert_eq!(
        test_version.created_by,
        Some(user_id),
        "created_by UUID should be preserved"
    );
    assert_eq!(
        test_version.created_by_display.as_deref(),
        Some(username),
        "created_by_display should show username instead of UUID"
    );
}

#[sqlx::test]
async fn test_defect_4_evidence_specs_populated_from_compliance_metadata(pool: PgPool) {
    // Defect 4: Evidence for ATO should be populated from compliance_metadata.
    let evidence_json: Value = json!({
        "evidence_specs": [
            {
                "kind": "Command",
                "details": {"cmd": "systemctl is-active ssh", "expect": "active"},
                "required_fields": {}
            },
            {
                "kind": "File",
                "details": {"path": "/etc/ssh/sshd_config", "note": "SSH config present"},
                "required_fields": {}
            }
        ]
    });

    let (policy_id, version_id) = create_test_policy(&pool, "evidence-test-policy").await;

    sqlx::query("UPDATE deployment_policy_versions SET compliance_metadata = $1 WHERE id = $2")
        .bind(evidence_json)
        .bind(version_id)
        .execute(&pool)
        .await
        .expect("attach evidence specs");

    // Call the production summary loader used by the handler (decodes evidence_specs).
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("production summary loader failed");
    let versions = summaries.get(&policy_id).expect("policy summaries present");

    let test_version = versions
        .iter()
        .find(|v| v.id == version_id)
        .expect("test version present in summaries");

    assert_eq!(
        test_version.evidence_specs.len(),
        2,
        "evidence_specs should be decoded from compliance_metadata (not empty)"
    );
    assert_eq!(
        test_version.evidence_specs[0].kind,
        crystal_forge::api::models::EvidenceKind::Command {
            cmd: "systemctl is-active ssh".to_string(),
            expect: "active".to_string(),
        },
        "first evidence spec should decode as a Command"
    );
}

#[sqlx::test]
async fn test_bundle_usage_count_distinct_bundle_lineages_selected_only(pool: PgPool) {
    /// DISCRIMINATING REGRESSION: Verifies that bundle_usage_count counts DISTINCT
    /// bundle lineages (bundle_id), not bundle versions (bundle_version_id), and only
    /// counts selected=true memberships.
    ///
    /// Fixture:
    /// - Policy P
    /// - Bundle A with 2 versions (A1, A2): both selected=true
    /// - Bundle B with 1 version (B1): selected=true
    /// - Bundle C with 1 version (C1): selected=false
    ///
    /// Expected bundle_usage_count = 2 (A and B only; C is excluded because unselected)
    /// Defective behavior (old code): count = 3 (A1, A2, B1 as distinct versions without filter)
    let (_policy_id, policy_version_id) = create_test_policy(&pool, "bundle-lineage-test").await;

    // Bundle A (lineage 1)
    let bundle_a_id = Uuid::new_v4();
    let bundle_a_v1_id = Uuid::new_v4();
    let bundle_a_v2_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, framework, version, layer, owner)
           VALUES ($1, $2, 'NIST CSF', '1.0', 'fleet', 'Platform Security')"#,
    )
    .bind(bundle_a_id)
    .bind("bundle-a-lineage")
    .execute(&pool)
    .await
    .expect("create bundle A");

    // Bundle A version 1
    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions 
           (id, bundle_id, version, publication_state, name, framework, framework_version,
            description, layer, owner, semantic_digest)
           VALUES ($1, $2, '1.0', 'draft', $3, 'NIST CSF', '1.0',
                   'bundle A v1', 'fleet', 'Platform Security', 'sha256-a1')"#,
    )
    .bind(bundle_a_v1_id)
    .bind(bundle_a_id)
    .bind("bundle-a-1.0")
    .execute(&pool)
    .await
    .expect("create bundle A v1");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_version_policies 
           (bundle_version_id, policy_version_id, selected, policy_order)
           VALUES ($1, $2, true, 1)"#,
    )
    .bind(bundle_a_v1_id)
    .bind(policy_version_id)
    .execute(&pool)
    .await
    .expect("select policy in bundle A v1");

    // Bundle A version 2 (same lineage)
    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions 
           (id, bundle_id, version, publication_state, name, framework, framework_version,
            description, layer, owner, semantic_digest)
           VALUES ($1, $2, '2.0', 'draft', $3, 'NIST CSF', '1.0',
                   'bundle A v2', 'fleet', 'Platform Security', 'sha256-a2')"#,
    )
    .bind(bundle_a_v2_id)
    .bind(bundle_a_id)
    .bind("bundle-a-2.0")
    .execute(&pool)
    .await
    .expect("create bundle A v2");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_version_policies 
           (bundle_version_id, policy_version_id, selected, policy_order)
           VALUES ($1, $2, true, 1)"#,
    )
    .bind(bundle_a_v2_id)
    .bind(policy_version_id)
    .execute(&pool)
    .await
    .expect("select policy in bundle A v2");

    // Bundle B (lineage 2)
    let bundle_b_id = Uuid::new_v4();
    let bundle_b_v1_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, framework, version, layer, owner)
           VALUES ($1, $2, 'NIST CSF', '1.0', 'fleet', 'Platform Security')"#,
    )
    .bind(bundle_b_id)
    .bind("bundle-b-lineage")
    .execute(&pool)
    .await
    .expect("create bundle B");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions 
           (id, bundle_id, version, publication_state, name, framework, framework_version,
            description, layer, owner, semantic_digest)
           VALUES ($1, $2, '1.0', 'draft', $3, 'NIST CSF', '1.0',
                   'bundle B v1', 'fleet', 'Platform Security', 'sha256-b1')"#,
    )
    .bind(bundle_b_v1_id)
    .bind(bundle_b_id)
    .bind("bundle-b-1.0")
    .execute(&pool)
    .await
    .expect("create bundle B v1");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_version_policies 
           (bundle_version_id, policy_version_id, selected, policy_order)
           VALUES ($1, $2, true, 1)"#,
    )
    .bind(bundle_b_v1_id)
    .bind(policy_version_id)
    .execute(&pool)
    .await
    .expect("select policy in bundle B v1");

    // Bundle C (lineage 3, unselected)
    let bundle_c_id = Uuid::new_v4();
    let bundle_c_v1_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO compliance_bundles (id, name, framework, version, layer, owner)
           VALUES ($1, $2, 'NIST CSF', '1.0', 'fleet', 'Platform Security')"#,
    )
    .bind(bundle_c_id)
    .bind("bundle-c-lineage")
    .execute(&pool)
    .await
    .expect("create bundle C");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_versions 
           (id, bundle_id, version, publication_state, name, framework, framework_version,
            description, layer, owner, semantic_digest)
           VALUES ($1, $2, '1.0', 'draft', $3, 'NIST CSF', '1.0',
                   'bundle C v1', 'fleet', 'Platform Security', 'sha256-c1')"#,
    )
    .bind(bundle_c_v1_id)
    .bind(bundle_c_id)
    .bind("bundle-c-1.0")
    .execute(&pool)
    .await
    .expect("create bundle C v1");

    sqlx::query(
        r#"INSERT INTO compliance_bundle_version_policies 
           (bundle_version_id, policy_version_id, selected, policy_order)
           VALUES ($1, $2, false, 1)"#, // selected = false
    )
    .bind(bundle_c_v1_id)
    .bind(policy_version_id)
    .execute(&pool)
    .await
    .expect("add policy to bundle C v1 (unselected)");

    // Load the counts
    let counts = load_policy_version_usage_counts(&pool, &[policy_version_id])
        .await
        .expect("production count loader failed");
    let (mapped, bundles) = counts
        .get(&policy_version_id)
        .copied()
        .expect("counts present for version");

    // CRITICAL ASSERTION:
    // If the code counts distinct bundle_version_id (old), it would return:
    //   - A1, A2, B1 (selected only) = 3 (or 4 if C1 were also counted)
    // If the code counts distinct bundle_id with selected=true filter (fixed), it returns:
    //   - A (lineage), B (lineage) = 2 (C is excluded because selected=false)
    assert_eq!(
        bundles, 2,
        "DISCRIMINATING TEST: bundle_usage_count must count DISTINCT bundle lineages with selected=true filter. \
         Expected 2 (Bundle A and B lineages only; C is unselected). \
         Failure indicates the query still counts distinct bundle_version_id or doesn't filter selected=true."
    );

    assert_eq!(mapped, 0, "no requirements mapped in this test");
}

#[sqlx::test]
async fn policy_drawer_owner_and_usage_respect_environment_visibility(pool: PgPool) {
    let (viewer_id, viewer_token) = role_session(&pool, AuthRole::Viewer, "Policy Owner").await;
    let (_unscoped_viewer_id, unscoped_viewer_token) =
        role_session(&pool, AuthRole::Viewer, "Unscoped Viewer").await;
    let (_admin_id, admin_token) = role_session(&pool, AuthRole::Admin, "Fleet Admin").await;
    let (policy_id, policy_version_id, visible_environment_id, visible_system_id, hidden_system_id) =
        create_usage_scope_fixture(&pool, viewer_id).await;
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(viewer_id)
        .bind(visible_environment_id)
        .execute(&pool)
        .await
        .expect("grant viewer environment membership");

    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("load PolicyDrawer owner summary");
    let selected = summaries[&policy_id]
        .iter()
        .find(|summary| summary.id == policy_version_id)
        .expect("find exact PolicyDrawer version");
    assert_eq!(selected.created_by, Some(viewer_id));
    assert_eq!(selected.created_by_display.as_deref(), Some("Policy Owner"));

    let base = usage_server(pool.clone()).await;
    let client = reqwest::Client::new();
    let request_usage = |token: &str| {
        client
            .get(format!(
                "{base}/api/v1/policy-versions/{policy_version_id}/usage"
            ))
            .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
    };

    let viewer_response = request_usage(&viewer_token)
        .send()
        .await
        .expect("request viewer policy usage");
    assert_eq!(viewer_response.status(), reqwest::StatusCode::OK);
    let viewer_usage: PolicyVersionUsageResponse = viewer_response
        .json()
        .await
        .expect("decode viewer policy usage");
    assert_eq!(viewer_usage.policy_version_id, policy_version_id);
    assert_eq!(viewer_usage.bundle_versions.len(), 1);
    assert_eq!(
        viewer_usage
            .systems
            .iter()
            .map(|system| system.system_id)
            .collect::<Vec<_>>(),
        vec![visible_system_id],
        "a viewer must not receive hostnames outside their environment memberships"
    );

    let unscoped_response = request_usage(&unscoped_viewer_token)
        .send()
        .await
        .expect("request unscoped viewer policy usage");
    assert_eq!(unscoped_response.status(), reqwest::StatusCode::OK);
    let unscoped_usage: PolicyVersionUsageResponse = unscoped_response
        .json()
        .await
        .expect("decode unscoped viewer policy usage");
    assert_eq!(unscoped_usage.policy_version_id, policy_version_id);
    assert_eq!(
        unscoped_usage.bundle_versions.len(),
        1,
        "environment scope must not hide non-sensitive bundle membership"
    );
    assert!(
        unscoped_usage.systems.is_empty(),
        "a viewer with no environment memberships must receive no system usage"
    );

    let admin_response = request_usage(&admin_token)
        .send()
        .await
        .expect("request admin policy usage");
    assert_eq!(admin_response.status(), reqwest::StatusCode::OK);
    let admin_usage: PolicyVersionUsageResponse = admin_response
        .json()
        .await
        .expect("decode admin policy usage");
    let admin_system_ids = admin_usage
        .systems
        .iter()
        .map(|system| system.system_id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        admin_system_ids,
        [visible_system_id, hidden_system_id].into_iter().collect(),
        "an administrator must retain fleet-wide PolicyDrawer usage visibility"
    );
}
