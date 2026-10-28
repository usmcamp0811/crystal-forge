//! Live-DB regressions for the TASK-433 Phase 2 unified policy editor.
//!
//! Two contracts are protected here, both of which the UI presents as
//! authoritative:
//!
//! 1. Imported policy-origin provenance. Provenance is recorded at import time
//!    in `compliance_source_artifacts` / `compliance_source_object_mappings`
//!    and is surfaced read-only by `fetch_policy_version_summaries()`. A draft
//!    derived from an imported version must keep the imported origin of its
//!    ancestor, and resolving provenance must never rewrite immutable source
//!    history.
//!
//! 2. Requirement-mapping mutation authorization. Only `provenance = 'manual'`
//!    mappings may be updated or deleted. Every other provenance value the
//!    schema allows (`imported`, `inherited`, `inferred`, `suggested`) must be
//!    rejected without changing the row or the version's mapping digest.
//!
//! Run with:
//! `cargo test -p cf-server --test policy_editor_phase2 -- --test-threads=1`

use crystal_forge::compliance::framework_model::FrameworkVersionCanonical;
use crystal_forge::compliance::requirement_model::RequirementVersionCanonical;
use crystal_forge::queries::compliance::{PolicyDraftIntent, ensure_policy_draft};
use crystal_forge::queries::deployment_policies::fetch_policy_version_summaries;
use crystal_forge::queries::framework_requirements::{
    create_policy_mapping, delete_policy_mapping, insert_framework_version,
    insert_requirement_version, update_policy_mapping, upsert_framework_lineage,
    upsert_requirement_lineage,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Every provenance value the schema permits other than `manual`.
const NON_MANUAL_PROVENANCE: [&str; 4] = ["imported", "inherited", "inferred", "suggested"];

async fn insert_policy(pool: &PgPool, name: &str) -> (Uuid, Uuid) {
    let policy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
         VALUES ($1, 'custom_check', '{\"mode\":\"all\",\"rules\":[]}', false) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("insert policy lineage");

    let version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_one(pool)
    .await
    .expect("load draft version id");

    (policy_id, version_id)
}

async fn insert_source_artifact(pool: &PgPool, body: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO compliance_source_artifacts \
             (content, filename, media_type, sha256, parser_version, detected_xccdf_version) \
         VALUES ($1, 'U_RHEL_9_STIG.xml', 'application/xml', \
                 encode(digest($1, 'sha256'), 'hex'), 'xccdf-1.2', '1.2') \
         RETURNING id",
    )
    .bind(body.as_bytes())
    .fetch_one(pool)
    .await
    .expect("insert source artifact")
}

async fn map_source_rule(
    pool: &PgPool,
    artifact_id: Uuid,
    rule_id: &str,
    version_id: Uuid,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO compliance_source_object_mappings \
             (source_artifact_id, object_kind, source_identity, policy_version_id, fidelity) \
         VALUES ($1, 'rule', $2, $3, 'preserved_opaque') RETURNING id",
    )
    .bind(artifact_id)
    .bind(rule_id)
    .bind(version_id)
    .fetch_one(pool)
    .await
    .expect("insert source object mapping")
}

/// Accept the current draft the way the production publish path leaves it, so
/// `ensure_policy_draft(CreateExplicit)` derives a fresh mutable draft from an
/// immutable published version.
async fn publish_current_draft(pool: &PgPool, policy_id: Uuid, version_id: Uuid) {
    let mut tx = pool.begin().await.expect("begin publish");
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("clear draft pointer");
    sqlx::query(
        "UPDATE deployment_policy_versions \
         SET publication_state = 'accepted', trust_state = 'trusted', \
             trusted_at = NOW(), published_at = NOW() \
         WHERE id = $1",
    )
    .bind(version_id)
    .execute(&mut *tx)
    .await
    .expect("accept version");
    sqlx::query("UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("set published pointer");
    tx.commit().await.expect("commit publish");
}

async fn create_requirement_version(pool: &PgPool, label: &str) -> Uuid {
    let fw_key = format!("phase2-fw-{label}-{}", Uuid::new_v4());
    let mut tx = pool.begin().await.expect("begin requirement fixture");
    let fw_id = upsert_framework_lineage(&mut tx, "Phase 2 FW", None, &fw_key, None)
        .await
        .expect("framework lineage");
    let fv = FrameworkVersionCanonical {
        canonical_source_key: fw_key.clone(),
        canonical_release_key: "V1R1".to_string(),
        version: "V1R1".to_string(),
        publisher: None,
        title: None,
    };
    let fv_id = insert_framework_version(&mut tx, fw_id, &fv, None, None)
        .await
        .expect("framework version");
    let req_key = format!("V-{}", Uuid::new_v4().simple());
    let req_id = upsert_requirement_lineage(&mut tx, fw_id, &req_key)
        .await
        .expect("requirement lineage");
    let rv = RequirementVersionCanonical {
        canonical_requirement_key: req_key.clone(),
        external_id: req_key,
        title: Some("Phase 2 requirement".to_string()),
        description: None,
        kind: "rule".to_string(),
        severity: None,
        check_text: None,
        fix_text: None,
        metadata: serde_json::json!({}),
    };
    let rv_id = insert_requirement_version(&mut tx, req_id, fv_id, &rv, None)
        .await
        .expect("requirement version");
    tx.commit().await.expect("commit requirement fixture");
    rv_id
}

async fn actor_id(pool: &PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (username, first_name, last_name, email, user_type) \
         VALUES ($1, 'Phase', 'Two', $2, 'human') RETURNING id",
    )
    .bind(format!("phase2-actor-{}", Uuid::new_v4().simple()))
    .bind(format!("phase2-{}@example.test", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .expect("insert actor")
}

async fn mapping_digest(pool: &PgPool, version_id: Uuid) -> String {
    sqlx::query_scalar("SELECT mapping_digest FROM deployment_policy_versions WHERE id = $1")
        .bind(version_id)
        .fetch_one(pool)
        .await
        .expect("load mapping digest")
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct MappingRow {
    relationship: String,
    coverage: String,
    rationale: Option<String>,
    provenance: String,
    trust_state: String,
}

async fn mapping_row(pool: &PgPool, mapping_id: Uuid) -> Option<MappingRow> {
    sqlx::query_as::<_, MappingRow>(
        "SELECT relationship, coverage, rationale, provenance, trust_state \
           FROM policy_requirement_mappings WHERE id = $1",
    )
    .bind(mapping_id)
    .fetch_optional(pool)
    .await
    .expect("load mapping row")
}

// ── Provenance ────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn custom_policy_reports_no_imported_provenance(pool: PgPool) {
    let (policy_id, version_id) = insert_policy(&pool, "phase2-custom-policy").await;

    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("hydrate summaries");
    let version = summaries
        .get(&policy_id)
        .and_then(|versions| versions.iter().find(|version| version.id == version_id))
        .expect("draft summary");

    assert!(
        version.provenance.is_empty(),
        "a policy authored in Crystal Forge must not claim imported provenance"
    );
}

#[sqlx::test]
async fn imported_policy_version_reports_authoritative_provenance(pool: PgPool) {
    let (policy_id, version_id) = insert_policy(&pool, "phase2-imported-policy").await;
    let artifact_id = insert_source_artifact(&pool, "<Benchmark id=\"RHEL_9_STIG\"/>").await;
    sqlx::query("UPDATE deployment_policy_versions SET source_artifact_id = $1 WHERE id = $2")
        .bind(artifact_id)
        .bind(version_id)
        .execute(&pool)
        .await
        .expect("attach artifact to version");
    map_source_rule(&pool, artifact_id, "SV-257777r925318_rule", version_id).await;

    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("hydrate summaries");
    let version = summaries
        .get(&policy_id)
        .and_then(|versions| versions.iter().find(|version| version.id == version_id))
        .expect("imported summary");

    assert_eq!(
        version.provenance.len(),
        1,
        "the direct artifact link must not duplicate the source-object mapping"
    );
    let origin = &version.provenance[0];
    assert_eq!(origin.source_artifact_id, artifact_id);
    assert_eq!(origin.filename, "U_RHEL_9_STIG.xml");
    assert_eq!(origin.media_type, "application/xml");
    assert_eq!(origin.parser_version, "xccdf-1.2");
    assert_eq!(origin.detected_xccdf_version.as_deref(), Some("1.2"));
    assert_eq!(origin.object_kind.as_deref(), Some("rule"));
    assert_eq!(
        origin.source_identity.as_deref(),
        Some("SV-257777r925318_rule")
    );
    assert_eq!(origin.fidelity.as_deref(), Some("preserved_opaque"));
    assert_eq!(origin.origin_policy_version_id, version_id);
    assert_eq!(origin.lineage_depth, 0);
    assert!(!origin.inherited, "a direct import is not inherited");
    assert!(!origin.sha256.is_empty(), "artifact digest must be exposed");
}

#[sqlx::test]
async fn derived_draft_inherits_imported_provenance_and_source_stays_immutable(pool: PgPool) {
    let (policy_id, imported_version_id) = insert_policy(&pool, "phase2-derived-policy").await;
    let artifact_id = insert_source_artifact(&pool, "<Benchmark id=\"DERIVED\"/>").await;
    sqlx::query("UPDATE deployment_policy_versions SET source_artifact_id = $1 WHERE id = $2")
        .bind(artifact_id)
        .bind(imported_version_id)
        .execute(&pool)
        .await
        .expect("attach artifact to version");
    let source_mapping_id =
        map_source_rule(&pool, artifact_id, "SV-900001r1_rule", imported_version_id).await;

    publish_current_draft(&pool, policy_id, imported_version_id).await;

    let mut tx = pool.begin().await.expect("begin derivation");
    let draft_version_id = ensure_policy_draft(
        &mut tx,
        policy_id,
        None,
        None,
        PolicyDraftIntent::CreateExplicit,
    )
    .await
    .expect("derive explicit draft");
    tx.commit().await.expect("commit derivation");
    assert_ne!(draft_version_id, imported_version_id);

    // Reload through the production hydration path, exactly as the editor does.
    let summaries = fetch_policy_version_summaries(&pool, &[policy_id])
        .await
        .expect("hydrate summaries");
    let versions = summaries.get(&policy_id).expect("policy versions");
    let draft = versions
        .iter()
        .find(|version| version.id == draft_version_id)
        .expect("draft summary");

    assert_eq!(
        draft.derived_from_version_id,
        Some(imported_version_id),
        "explicit derivation must record its ancestor"
    );
    assert_eq!(
        draft.provenance.len(),
        1,
        "the derived draft must keep exactly the imported origin"
    );
    let origin = &draft.provenance[0];
    assert_eq!(origin.source_artifact_id, artifact_id);
    assert_eq!(origin.source_identity.as_deref(), Some("SV-900001r1_rule"));
    assert_eq!(origin.origin_policy_version_id, imported_version_id);
    assert_eq!(origin.lineage_depth, 1);
    assert!(
        origin.inherited,
        "origin resolved through ancestry must be reported as inherited"
    );

    // The imported version keeps its own direct provenance.
    let imported = versions
        .iter()
        .find(|version| version.id == imported_version_id)
        .expect("imported summary");
    assert_eq!(imported.provenance.len(), 1);
    assert!(!imported.provenance[0].inherited);

    // Resolving provenance must not rewrite immutable source history.
    let (still_mapped_version, still_mapped_artifact): (Option<Uuid>, Uuid) = sqlx::query_as(
        "SELECT policy_version_id, source_artifact_id \
           FROM compliance_source_object_mappings WHERE id = $1",
    )
    .bind(source_mapping_id)
    .fetch_one(&pool)
    .await
    .expect("reload source mapping");
    assert_eq!(still_mapped_version, Some(imported_version_id));
    assert_eq!(still_mapped_artifact, artifact_id);

    let source_publication_state: String = sqlx::query_scalar(
        "SELECT publication_state FROM deployment_policy_versions WHERE id = $1",
    )
    .bind(imported_version_id)
    .fetch_one(&pool)
    .await
    .expect("reload imported version state");
    assert_eq!(source_publication_state, "accepted");
}

// ── Mapping mutation authorization ────────────────────────────────────────────

#[sqlx::test]
async fn manual_mapping_supports_update_and_delete(pool: PgPool) {
    let (_policy_id, version_id) = insert_policy(&pool, "phase2-manual-mapping").await;
    let requirement_version_id = create_requirement_version(&pool, "manual").await;
    let actor = actor_id(&pool).await;

    let mapping_id = create_policy_mapping(
        &pool,
        version_id,
        requirement_version_id,
        "implements",
        "full",
        Some("initial rationale"),
        "manual",
        actor,
    )
    .await
    .expect("create manual mapping");

    update_policy_mapping(
        &pool,
        version_id,
        mapping_id,
        "supports",
        "partial",
        Some("revised rationale"),
    )
    .await
    .expect("manual mapping update must be permitted");

    let row = mapping_row(&pool, mapping_id)
        .await
        .expect("manual mapping row");
    assert_eq!(row.relationship, "supports");
    assert_eq!(row.coverage, "partial");
    assert_eq!(row.rationale.as_deref(), Some("revised rationale"));
    assert_eq!(row.provenance, "manual");

    delete_policy_mapping(&pool, version_id, mapping_id)
        .await
        .expect("manual mapping delete must be permitted");
    assert!(
        mapping_row(&pool, mapping_id).await.is_none(),
        "deleted manual mapping must be gone"
    );
}

#[sqlx::test]
async fn non_manual_mappings_reject_update_and_delete_without_side_effects(pool: PgPool) {
    let actor = actor_id(&pool).await;

    for provenance in NON_MANUAL_PROVENANCE {
        let (_policy_id, version_id) =
            insert_policy(&pool, &format!("phase2-{provenance}-mapping")).await;
        let requirement_version_id = create_requirement_version(&pool, provenance).await;

        let mapping_id = create_policy_mapping(
            &pool,
            version_id,
            requirement_version_id,
            "implements",
            "full",
            Some("authoritative rationale"),
            provenance,
            actor,
        )
        .await
        .unwrap_or_else(|error| panic!("create {provenance} mapping: {error}"));

        let before = mapping_row(&pool, mapping_id)
            .await
            .unwrap_or_else(|| panic!("{provenance} mapping row"));
        let digest_before = mapping_digest(&pool, version_id).await;

        let update = update_policy_mapping(
            &pool,
            version_id,
            mapping_id,
            "supports",
            "partial",
            Some("tampered"),
        )
        .await;
        let update_error = update
            .expect_err(&format!("{provenance} mapping update must be rejected"))
            .to_string();
        assert!(
            update_error.contains("POLICY_MAPPING_IMPORTED"),
            "expected a read-only rejection for {provenance}, got: {update_error}"
        );

        let delete = delete_policy_mapping(&pool, version_id, mapping_id).await;
        let delete_error = delete
            .expect_err(&format!("{provenance} mapping delete must be rejected"))
            .to_string();
        assert!(
            delete_error.contains("POLICY_MAPPING_IMPORTED"),
            "expected a read-only rejection for {provenance}, got: {delete_error}"
        );

        let after = mapping_row(&pool, mapping_id)
            .await
            .unwrap_or_else(|| panic!("{provenance} mapping must still exist"));
        assert_eq!(
            before, after,
            "rejected mutations must leave the {provenance} mapping unchanged"
        );
        assert_eq!(
            digest_before,
            mapping_digest(&pool, version_id).await,
            "rejected mutations must not change the {provenance} mapping digest"
        );
    }
}
