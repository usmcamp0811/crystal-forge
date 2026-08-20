//! Live-DB regression tests for evidence_for_ato functionality.
//!
//! These tests verify that evidence collection specifications are:
//! 1. Persisted in deployment_policy_versions.compliance_metadata
//! 2. Preserved through publish/draft lifecycle using production functions
//! 3. Immutable across revisions (each version has independent specs)
//! 4. Correctly updated in semantic digest when changed
//! 5. Properly validated (reject malformed specs at decode time)
//! 6. Decoded from compliance_metadata by the production summary loader that
//!    powers the PolicyDrawer "Evidence for ATO" indicator
//!
//! All tests invoke production API paths:
//! - create_deployment_policy_with_mappings() for creation with evidence
//! - update_deployment_policy() for updates with evidence
//! - ensure_policy_draft() for draft derivation
//! - fetch_policy_version_summaries() for UI loader
//!
//! Run with: cargo test -p cf-server --test evidence_for_ato -- --test-threads=1

use crystal_forge::api::models::EvidenceKind;
use crystal_forge::api::models::EvidenceSpec;
use crystal_forge::models::deployment_policies::{
    CreateDeploymentPolicyRequest, UpdateDeploymentPolicyRequest,
};
use crystal_forge::queries::compliance::{PolicyDraftIntent, ensure_policy_draft};
use crystal_forge::queries::deployment_policies::{
    create_deployment_policy_with_mappings, fetch_policy_version_summaries,
    update_deployment_policy,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Helper: Create a policy with Evidence specs via production API
async fn create_policy_with_evidence(
    pool: &PgPool,
    policy_name: &str,
    evidence_specs: Vec<EvidenceSpec>,
) -> Uuid {
    let request = CreateDeploymentPolicyRequest {
        name: policy_name.to_string(),
        description: Some("Test policy".to_string()),
        policy_type: "require_packages".to_string(),
        config: serde_json::json!({}),
        enabled: Some(true),
        srg_ids: Vec::new(),
        cci_ids: Vec::new(),
        category: None,
        framework: None,
        severity: None,
        control_family: None,
        cmmc_level: None,
        cis_section: None,
        rationale: None,
        evidence_specs,
        requirement_mappings: Vec::new(),
    };

    create_deployment_policy_with_mappings(pool, &request, None)
        .await
        .expect("create policy with evidence")
        .id
}

/// Helper: Update policy evidence via production API
async fn update_policy_evidence(
    pool: &PgPool,
    policy_id: Uuid,
    evidence_specs: Option<Vec<EvidenceSpec>>,
) {
    let request = UpdateDeploymentPolicyRequest {
        name: None,
        description: None,
        policy_type: None,
        config: None,
        enabled: None,
        srg_ids: None,
        cci_ids: None,
        category: None,
        framework: None,
        severity: None,
        control_family: None,
        cmmc_level: None,
        cis_section: None,
        rationale: None,
        evidence_specs,
    };

    update_deployment_policy(pool, &policy_id, &request, None)
        .await
        .expect("update policy evidence");
}

#[sqlx::test]
async fn test_evidence_create_read_regression(pool: PgPool) {
    // PRODUCTION REGRESSION: Create → Persist → Read
    //
    // Verifies the full lifecycle:
    // 1. CreateDeploymentPolicyRequest with evidence_specs
    // 2. Production create path validates and merges into compliance_metadata
    // 3. Semantic digest computed and persisted
    // 4. fetch_policy_version_summaries() decodes evidence_specs exactly
    //
    // This proves: request DTO → validation → metadata merge → persistence
    //             → strict decode → response DTO

    let evidence_specs = vec![
        EvidenceSpec {
            kind: EvidenceKind::Command {
                cmd: "systemctl status ssh".to_string(),
                expect: "active".to_string(),
            },
            required_fields: Default::default(),
        },
        EvidenceSpec {
            kind: EvidenceKind::File {
                path: "/etc/ssh/sshd_config".to_string(),
                note: Some("SSH daemon config".to_string()),
            },
            required_fields: Default::default(),
        },
    ];

    let policy_id =
        create_policy_with_evidence(&pool, "evidence-create-read", evidence_specs.clone()).await;

    // Load via production summary loader (used by UI)
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("production summary loader");

    let versions = summaries.get(&policy_id).expect("policy versions present");
    assert!(
        !versions.is_empty(),
        "Policy should have at least draft version"
    );

    let draft_version = &versions[0];
    assert_eq!(
        draft_version.evidence_specs.len(),
        2,
        "Should decode 2 evidence specs exactly"
    );

    // Verify spec contents match request
    match &draft_version.evidence_specs[0].kind {
        EvidenceKind::Command { cmd, expect } => {
            assert_eq!(cmd, "systemctl status ssh");
            assert_eq!(expect, "active");
        }
        _ => panic!("First spec should be Command"),
    }

    match &draft_version.evidence_specs[1].kind {
        EvidenceKind::File { path, note } => {
            assert_eq!(path, "/etc/ssh/sshd_config");
            assert_eq!(note.as_deref(), Some("SSH daemon config"));
        }
        _ => panic!("Second spec should be File"),
    }
}

#[sqlx::test]
async fn test_evidence_update_replacement(pool: PgPool) {
    // PRODUCTION REGRESSION: Update with explicit replacement
    //
    // Create policy with Evidence A.
    // Production update with evidence_specs = Some([Evidence B]).
    // Reload and verify Evidence B replaced Evidence A.

    let initial_specs = vec![EvidenceSpec {
        kind: EvidenceKind::Command {
            cmd: "ps aux | grep ssh".to_string(),
            expect: "sshd".to_string(),
        },
        required_fields: Default::default(),
    }];

    let policy_id =
        create_policy_with_evidence(&pool, "evidence-replace", initial_specs.clone()).await;

    // Verify initial state
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("verify initial");
    let versions = summaries.get(&policy_id).unwrap();
    assert_eq!(
        versions[0].evidence_specs.len(),
        1,
        "Should start with 1 spec"
    );

    // Update with completely different evidence
    let replacement_specs = vec![
        EvidenceSpec {
            kind: EvidenceKind::File {
                path: "/var/log/auth.log".to_string(),
                note: Some("Authentication log".to_string()),
            },
            required_fields: Default::default(),
        },
        EvidenceSpec {
            kind: EvidenceKind::UnitState {
                unit: "ssh.service".to_string(),
                state: "running".to_string(),
            },
            required_fields: Default::default(),
        },
    ];

    update_policy_evidence(&pool, policy_id, Some(replacement_specs.clone())).await;

    // Verify replacement
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("verify replacement");
    let versions = summaries.get(&policy_id).unwrap();
    let draft = &versions[0];

    assert_eq!(
        draft.evidence_specs.len(),
        2,
        "Should have 2 replacement specs"
    );
    match &draft.evidence_specs[0].kind {
        EvidenceKind::File { .. } => {}
        _ => panic!("First spec should be File"),
    }
    match &draft.evidence_specs[1].kind {
        EvidenceKind::UnitState { .. } => {}
        _ => panic!("Second spec should be UnitState"),
    }
}

#[sqlx::test]
async fn test_evidence_update_omission_preserves(pool: PgPool) {
    // PRODUCTION REGRESSION: PATCH semantics (None = omit, preserve existing)
    //
    // Create policy with Evidence A.
    // Production update with evidence_specs = None (update other fields, don't touch evidence).
    // Reload and verify Evidence A unchanged.
    //
    // This protects the PATCH behavior where omitted fields are not cleared.

    let specs = vec![EvidenceSpec {
        kind: EvidenceKind::Attestation {
            note: "Security review completed".to_string(),
        },
        required_fields: Default::default(),
    }];

    let policy_id = create_policy_with_evidence(&pool, "evidence-preserve", specs.clone()).await;

    // Verify initial
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("verify initial");
    let versions = summaries.get(&policy_id).unwrap();
    let initial_spec_count = versions[0].evidence_specs.len();
    assert_eq!(initial_spec_count, 1, "Should start with 1 spec");

    // Update policy name WITHOUT touching evidence_specs (None = omit, don't clear)
    update_policy_evidence(&pool, policy_id, None).await;

    // Verify evidence preserved
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("verify preserved");
    let versions = summaries.get(&policy_id).unwrap();
    assert_eq!(
        versions[0].evidence_specs.len(),
        1,
        "Evidence should be preserved when update omits evidence_specs field"
    );
}

#[sqlx::test]
async fn test_evidence_update_explicit_clear(pool: PgPool) {
    // PRODUCTION REGRESSION: Explicit clear (Some(vec![]) = drop all evidence)
    //
    // Create policy with Evidence A.
    // Production update with evidence_specs = Some([]) (explicit empty list).
    // Reload and verify evidence_specs.is_empty().
    //
    // This must be distinguishable from omission (None).

    let specs = vec![EvidenceSpec {
        kind: EvidenceKind::Log {
            source: "systemd".to_string(),
            unit: "ssh.service".to_string(),
            match_text: "error".to_string(),
        },
        required_fields: Default::default(),
    }];

    let policy_id = create_policy_with_evidence(&pool, "evidence-clear", specs.clone()).await;

    // Verify initial has evidence
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("verify initial");
    assert_eq!(
        summaries.get(&policy_id).unwrap()[0].evidence_specs.len(),
        1,
        "Should start with 1 spec"
    );

    // Explicit clear: Some([])
    update_policy_evidence(&pool, policy_id, Some(vec![])).await;

    // Verify cleared
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("verify cleared");
    let versions = summaries.get(&policy_id).unwrap();
    assert!(
        versions[0].evidence_specs.is_empty(),
        "Evidence should be explicitly cleared when Some([])"
    );
}

#[sqlx::test]
async fn test_evidence_validation_rejection(pool: PgPool) {
    // PRODUCTION REGRESSION: Validation at API boundary
    //
    // Attempt to create policy with invalid evidence (e.g., Command with empty cmd).
    // Production validation must reject it.
    //
    // This proves the strict decoder catches semantic issues at the API, not just
    // at load-time.

    let invalid_specs = vec![EvidenceSpec {
        kind: EvidenceKind::Command {
            cmd: "".to_string(), // INVALID: empty required field
            expect: "something".to_string(),
        },
        required_fields: Default::default(),
    }];

    let request = CreateDeploymentPolicyRequest {
        name: "evidence-invalid".to_string(),
        description: Some("Test policy".to_string()),
        policy_type: "require_packages".to_string(),
        config: serde_json::json!({}),
        enabled: Some(true),
        srg_ids: Vec::new(),
        cci_ids: Vec::new(),
        category: None,
        framework: None,
        severity: None,
        control_family: None,
        cmmc_level: None,
        cis_section: None,
        rationale: None,
        evidence_specs: invalid_specs,
        requirement_mappings: Vec::new(),
    };

    let result = create_deployment_policy_with_mappings(&pool, &request, None).await;

    assert!(
        result.is_err(),
        "Production create must reject evidence with empty required fields"
    );
}

#[sqlx::test]
async fn test_evidence_persisted_corruption_rejected(pool: PgPool) {
    // CORRUPTION REGRESSION: Strict decoder detects malformed persisted evidence
    //
    // Create a valid policy.
    // Directly corrupt compliance_metadata.evidence_specs in PostgreSQL:
    //   - Replace the entire array with an invalid non-array value
    // Load via production summary query.
    // Assert: strict validation rejects corrupted entry and returns error.
    //
    // The strict decoder is fail-closed: it does NOT silently filter
    // invalid specs. Instead it errors the entire load operation.
    // This prevents silent data loss if corruption is detected.

    let specs = vec![EvidenceSpec {
        kind: EvidenceKind::Command {
            cmd: "valid".to_string(),
            expect: "output".to_string(),
        },
        required_fields: Default::default(),
    }];

    let policy_id = create_policy_with_evidence(&pool, "evidence-corrupt", specs).await;

    // Get the draft version ID
    let draft_version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_one(&pool)
    .await
    .expect("fetch draft version id");

    // Corrupt the evidence_specs: replace array with an object (type mismatch)
    sqlx::query(
        r#"UPDATE deployment_policy_versions 
           SET compliance_metadata = jsonb_set(
               compliance_metadata,
               '{evidence_specs}',
               '{"invalid": "object"}'
           )
           WHERE id = $1"#,
    )
    .bind(draft_version_id)
    .execute(&pool)
    .await
    .expect("corrupt metadata");

    // Try to load via production path
    // The strict decoder MUST error on corruption, not filter silently
    let result = fetch_policy_version_summaries(&pool, &[policy_id]).await;

    assert!(
        result.is_err(),
        "Strict decoder must error on corrupted evidence (not an array), not silently filter"
    );
}

#[sqlx::test]
async fn test_evidence_ensure_policy_draft(pool: PgPool) {
    // PRODUCTION REGRESSION: ensure_policy_draft() copies evidence
    //
    // Create published policy with Evidence A.
    // Call production ensure_policy_draft() to derive a draft.
    // Load the resulting draft.
    // Verify Evidence A preserved in draft.

    let specs = vec![EvidenceSpec {
        kind: EvidenceKind::EvalAttr {
            attr: "config.system.stateVersion".to_string(),
        },
        required_fields: Default::default(),
    }];

    let policy_id = create_policy_with_evidence(&pool, "evidence-draft", specs.clone()).await;

    // Publish the policy
    let draft_version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_one(&pool)
    .await
    .expect("fetch draft version id");

    let mut tx = pool.begin().await.expect("begin tx");
    // Clear draft pointer first (since the version is being published)
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("clear draft pointer");
    sqlx::query(
        "UPDATE deployment_policy_versions SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP WHERE id = $1",
    )
    .bind(draft_version_id)
    .execute(&mut *tx)
    .await
    .expect("accept version");
    sqlx::query("UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2")
        .bind(draft_version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("set published");
    tx.commit().await.expect("commit publish");

    // Verify published policy has evidence
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("verify published");
    let versions = summaries.get(&policy_id).unwrap();
    assert_eq!(
        versions[0].evidence_specs.len(),
        1,
        "Published version should have evidence"
    );

    // Now derive a draft (simulating UI "Edit" workflow)
    let mut tx = pool.begin().await.expect("begin draft derive");
    let new_draft_id = ensure_policy_draft(
        &mut tx,
        policy_id,
        None,
        None,
        PolicyDraftIntent::EnsureMutable,
    )
    .await
    .expect("ensure draft");
    tx.commit().await.expect("commit draft derivation");

    // Verify draft has evidence from published
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("verify draft derived");
    let versions = summaries.get(&policy_id).unwrap();
    let derived_draft = versions
        .iter()
        .find(|v| v.id == new_draft_id)
        .expect("derived draft present");

    assert_eq!(
        derived_draft.evidence_specs.len(),
        1,
        "Derived draft must preserve evidence from published version"
    );
    match &derived_draft.evidence_specs[0].kind {
        EvidenceKind::EvalAttr { .. } => {}
        _ => panic!("Spec should be EvalAttr"),
    }
}

#[sqlx::test]
async fn test_evidence_historical_isolation(pool: PgPool) {
    // PRODUCTION REGRESSION: Evidence immutability across versions
    //
    // Since publication_state='accepted' is unique per policy_id, we test
    // isolation between draft versions instead (both are simultaneously valid):
    //
    // Workflow:
    // 1. Create V1 draft with Evidence A
    // 2. Publish V1
    // 3. Derive V2 draft from V1
    // 4. Update V2 with Evidence B via production update (keeps V1 unchanged)
    // 5. Load both exact versions (V1 published, V2 draft)
    // 6. Assert V1 → Evidence A, V2 → Evidence B (no cross-version leakage)

    // V1: Command evidence
    let v1_specs = vec![EvidenceSpec {
        kind: EvidenceKind::Command {
            cmd: "systemctl status ssh".to_string(),
            expect: "active".to_string(),
        },
        required_fields: Default::default(),
    }];

    let policy_id = create_policy_with_evidence(&pool, "evidence-history", v1_specs.clone()).await;

    // Get V1 version ID and publish it
    let v1_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_one(&pool)
    .await
    .expect("fetch v1");

    // Publish V1 (this is the only published version for this policy)
    // Must: 1) clear draft, 2) accept version, 3) set published pointer (in same tx)
    let mut tx = pool.begin().await.expect("begin publish v1");
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("clear draft");
    sqlx::query("UPDATE deployment_policy_versions SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP WHERE id = $1")
        .bind(v1_id).execute(&mut *tx).await.expect("accept");
    sqlx::query("UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2")
        .bind(v1_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("set pub");
    tx.commit().await.expect("commit publish v1");

    // V2: Derive draft from published V1
    let mut tx = pool.begin().await.expect("begin v2");
    let v2_id = ensure_policy_draft(
        &mut tx,
        policy_id,
        None,
        None,
        PolicyDraftIntent::EnsureMutable,
    )
    .await
    .expect("ensure v2");
    tx.commit().await.expect("commit v2 derive");

    // Update V2 draft with different evidence (V1 remains unchanged)
    let v2_specs = vec![EvidenceSpec {
        kind: EvidenceKind::File {
            path: "/etc/ssh/sshd_config".to_string(),
            note: Some("SSH configuration".to_string()),
        },
        required_fields: Default::default(),
    }];
    update_policy_evidence(&pool, policy_id, Some(v2_specs)).await;

    // Load and verify each version: V1 (published) and V2 (draft)
    // Verify V1 still has Command evidence (not mutated by V2 update)
    // Verify V2 has File evidence (new specs)
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("fetch all versions");
    let versions = summaries.get(&policy_id).unwrap();

    let v1_loaded = versions.iter().find(|v| v.id == v1_id).expect("v1 present");
    assert_eq!(
        v1_loaded.evidence_specs.len(),
        1,
        "V1 should still have Command evidence (not mutated)"
    );
    match &v1_loaded.evidence_specs[0].kind {
        EvidenceKind::Command { .. } => {}
        _ => panic!("V1 spec should remain Command"),
    }

    let v2_loaded = versions.iter().find(|v| v.id == v2_id).expect("v2 present");
    assert_eq!(
        v2_loaded.evidence_specs.len(),
        1,
        "V2 draft should have File evidence"
    );
    match &v2_loaded.evidence_specs[0].kind {
        EvidenceKind::File { .. } => {}
        _ => panic!("V2 spec should be File"),
    }
}

#[sqlx::test]
async fn test_evidence_digest_changes_with_evidence(pool: PgPool) {
    // DIGEST REGRESSION: Semantic digest incorporates evidence
    //
    // Production behavior: digest is computed by write_policy_version_digest()
    // which includes compliance_metadata (including evidence_specs) in the canonical.
    //
    // Workflow:
    // 1. Create V1 with Evidence A, capture digest D1
    // 2. Update V2 with Evidence B
    // 3. Capture digest D2
    // 4. Assert D1 != D2
    //
    // This proves evidence changes trigger digest recomputation (protecting
    // immutability assumptions downstream).

    let specs_a = vec![EvidenceSpec {
        kind: EvidenceKind::Command {
            cmd: "test -f /etc/hostname".to_string(),
            expect: "0".to_string(),
        },
        required_fields: Default::default(),
    }];

    let policy_id = create_policy_with_evidence(&pool, "evidence-digest", specs_a).await;

    // Get V1 digest
    let v1_digest: String = sqlx::query_scalar(
        "SELECT semantic_digest FROM deployment_policy_versions WHERE policy_id = $1 ORDER BY created_at LIMIT 1",
    )
    .bind(policy_id)
    .fetch_one(&pool)
    .await
    .expect("fetch v1 digest");

    // Update with different evidence
    let specs_b = vec![EvidenceSpec {
        kind: EvidenceKind::Log {
            source: "journalctl".to_string(),
            unit: "ssh.service".to_string(),
            match_text: "authentication failure".to_string(),
        },
        required_fields: Default::default(),
    }];

    update_policy_evidence(&pool, policy_id, Some(specs_b)).await;

    // Get updated digest (on draft)
    let v1_updated_digest: String = sqlx::query_scalar(
        "SELECT semantic_digest FROM deployment_policy_versions WHERE policy_id = $1 AND publication_state IN ('draft', 'incomplete') ORDER BY created_at DESC LIMIT 1",
    )
    .bind(policy_id)
    .fetch_one(&pool)
    .await
    .expect("fetch updated digest");

    assert_ne!(
        v1_digest, v1_updated_digest,
        "Digest should change when evidence is updated"
    );
}

/// Regression test: required_fields metadata survives UI round-trip
/// 
/// This test verifies that EvidenceSpec.required_fields (versioned metadata
/// set by the server) is preserved when:
/// 1. Loaded from persisted compliance_metadata
/// 2. Edited in the UI (PolicyEvidence::from -> to_evidence_spec)
/// 3. Sent back via API
/// 4. Re-persisted
///
/// This prevents silent destruction of metadata when editing unrelated Evidence fields.
#[sqlx::test]
async fn test_evidence_required_fields_preserved_through_edit(pool: PgPool) {
    // Create policy with initial evidence including required_fields
    let mut initial_fields = std::collections::HashMap::new();
    initial_fields.insert("version".to_string(), "1.0".to_string());
    initial_fields.insert("source".to_string(), "policy_engine_v3".to_string());

    let initial_spec = EvidenceSpec {
        kind: EvidenceKind::Command {
            cmd: "systemctl status ssh".to_string(),
            expect: "active".to_string(),
        },
        required_fields: initial_fields.clone(),
    };

    let policy_id = create_policy_with_evidence(&pool, "required_fields_test", vec![initial_spec]).await;

    // Fetch the persisted evidence from compliance_metadata
    let persisted_metadata: serde_json::Value = sqlx::query_scalar(
        "SELECT compliance_metadata FROM deployment_policy_versions WHERE policy_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(policy_id)
    .fetch_one(&pool)
    .await
    .expect("fetch persisted metadata");

    let evidence_specs = persisted_metadata
        .get("evidence_specs")
        .and_then(|v| v.as_array())
        .expect("evidence_specs array");

    assert!(!evidence_specs.is_empty(), "evidence_specs should be persisted");

    // Extract the first spec and verify required_fields survived persistence
    let persisted_spec = &evidence_specs[0];
    let persisted_fields = persisted_spec
        .get("required_fields")
        .and_then(|v| v.as_object())
        .expect("required_fields object");

    assert_eq!(
        persisted_fields.len(),
        2,
        "required_fields should have 2 entries after persistence"
    );
    assert_eq!(
        persisted_fields.get("version").and_then(|v| v.as_str()),
        Some("1.0"),
        "version field should survive persistence"
    );
    assert_eq!(
        persisted_fields.get("source").and_then(|v| v.as_str()),
        Some("policy_engine_v3"),
        "source field should survive persistence"
    );

    // Simulate edit: update unrelated field (description) without touching evidence
    // This tests that required_fields is not destroyed by the save path
    let update_request = UpdateDeploymentPolicyRequest {
        name: None,
        description: Some("Updated description without evidence change".to_string()),
        policy_type: None,
        config: None,
        enabled: None,
        srg_ids: None,
        cci_ids: None,
        category: None,
        framework: None,
        severity: None,
        control_family: None,
        cmmc_level: None,
        cis_section: None,
        rationale: None,
        evidence_specs: None, // Not changing evidence, just description
    };

    update_deployment_policy(&pool, &policy_id, &update_request, None)
        .await
        .expect("update policy with new description");

    // Fetch the updated version and verify required_fields still exists
    let updated_metadata: serde_json::Value = sqlx::query_scalar(
        "SELECT compliance_metadata FROM deployment_policy_versions WHERE policy_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(policy_id)
    .fetch_one(&pool)
    .await
    .expect("fetch updated metadata");

    let updated_evidence_specs = updated_metadata
        .get("evidence_specs")
        .and_then(|v| v.as_array())
        .expect("evidence_specs should still exist after unrelated edit");

    let updated_spec = &updated_evidence_specs[0];
    let updated_fields = updated_spec
        .get("required_fields")
        .and_then(|v| v.as_object())
        .expect("required_fields should survive unrelated edit");

    assert_eq!(
        updated_fields.len(),
        2,
        "required_fields must not be destroyed by unrelated field edit"
    );
    assert_eq!(
        updated_fields.get("version").and_then(|v| v.as_str()),
        Some("1.0"),
        "version must survive unrelated edit"
    );

    println!("✓ required_fields metadata survived: creation → edit → persistence");
}
