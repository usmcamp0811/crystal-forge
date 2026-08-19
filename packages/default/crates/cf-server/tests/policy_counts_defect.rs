//! Live-DB regression tests for PolicyCard counts (Defect 2), Owner display
//! (Defect 3), and Evidence for ATO (Defect 4).
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
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crystal_forge::queries::deployment_policies::{
    fetch_policy_version_summaries, get_deployment_policies_by_versions,
    load_policy_version_usage_counts,
};

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

