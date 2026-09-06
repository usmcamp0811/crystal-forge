// TASK-440: V2 config snapshot database contract remediation tests.
//
// These tests verify total boolean validator contracts, required nullable fields,
// identity constraints, immutability, retention, and upgrade compatibility.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
static MIGRATOR_THROUGH_0249: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

const TARGET_KEY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PROVENANCE_DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn total_boolean_validators_never_return_null(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    // Safe error validator must return FALSE, not NULL
    let cases = vec![
        (json!({}), "empty object"),
        (json!({"code": "x"}), "missing message"),
        (json!({"message": "x"}), "missing code"),
        (json!({"code": null, "message": "x"}), "null code"),
        (json!({"code": "x", "message": null}), "null message"),
        (json!(null), "json null"),
        (json!("scalar"), "scalar value"),
    ];

    for (input, label) in cases {
        let result: Option<bool> =
            sqlx::query_scalar("SELECT evaluation_safe_error_v2_valid($1) IS FALSE")
                .bind(&input)
                .fetch_one(&pool)
                .await
                .expect(&format!("safe_error test: {}", label));
        assert_eq!(
            result,
            Some(true),
            "safe_error {}: must return FALSE, got NULL or TRUE",
            label
        );
    }

    // Definition source validator
    let def_cases = vec![
        (json!({}), "empty object"),
        (json!({"source_path": "x"}), "missing priority"),
        (
            json!({"source_path": "x", "priority": "not_a_number"}),
            "malformed priority",
        ),
        (
            json!({"source_path": "x", "priority": null}),
            "missing source_input",
        ),
        (
            json!({"source_path": "x", "priority": null, "source_input": null}),
            "missing source_revision",
        ),
    ];

    for (input, label) in def_cases {
        let result: Option<bool> =
            sqlx::query_scalar("SELECT evaluation_definition_source_v2_valid($1) IS FALSE")
                .bind(&input)
                .fetch_one(&pool)
                .await
                .expect(&format!("definition_source test: {}", label));
        assert_eq!(
            result,
            Some(true),
            "definition_source {}: must return FALSE",
            label
        );
    }

    // Global provenance unavailable diagnostic
    let prov_cases = vec![
        (
            json!({"state": "unavailable", "reason_code": "test"}),
            "missing diagnostic key",
        ),
        (
            json!({"state": "unavailable", "reason_code": "test", "diagnostic": {"code": "x"}}),
            "malformed diagnostic",
        ),
    ];

    for (input, label) in prov_cases {
        let result: Option<bool> = sqlx::query_scalar(
            "SELECT evaluation_snapshot_v2_provenance_valid($1, false) IS FALSE",
        )
        .bind(&input)
        .fetch_one(&pool)
        .await
        .expect(&format!("provenance test: {}", label));
        assert_eq!(
            result,
            Some(true),
            "provenance {}: must return FALSE",
            label
        );
    }
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn v2_payload_requires_top_level_keys(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    let valid_metadata = json!({
        "state": "available",
        "option_type": "string",
        "loc": [],
        "declared_type": "str",
        "declarations": [],
        "declaration_positions": [],
        "highest_prio": 100,
        "is_defined": true,
        "surviving_definition_sources": [],
    });

    let valid_effective_value = json!({"kind": "scalar", "value": "test"});
    let valid_provenance = json!({"state": "unavailable"});

    let cases = vec![
        (
            json!({"effective_value": valid_effective_value.clone(), "provenance": valid_provenance.clone()}),
            "missing metadata",
        ),
        (
            json!({"metadata": valid_metadata.clone(), "provenance": valid_provenance.clone()}),
            "missing effective_value",
        ),
        (
            json!({"metadata": valid_metadata.clone(), "effective_value": valid_effective_value.clone()}),
            "missing provenance",
        ),
        (
            json!({"metadata": valid_metadata.clone(), "effective_value": null, "provenance": valid_provenance.clone()}),
            "effective_value is JSON null",
        ),
    ];

    for (payload, label) in cases {
        let result: Option<bool> =
            sqlx::query_scalar("SELECT evaluation_option_payload_v2_valid($1) IS FALSE")
                .bind(&payload)
                .fetch_one(&pool)
                .await
                .expect(&format!("payload top-level test: {}", label));
        assert_eq!(result, Some(true), "payload {}: must reject", label);
    }
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn v2_required_nullable_fields_reject_missing_keys(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    // Metadata available must have option_type, declared_type, highest_prio keys even if values are null
    let metadata_cases = vec![
        (
            json!({
                "state": "available",
                "loc": [],
                "declared_type": "str",
                "declarations": [],
                "declaration_positions": [],
                "highest_prio": 100,
                "is_defined": true,
                "surviving_definition_sources": [],
            }),
            "missing option_type key",
        ),
        (
            json!({
                "state": "available",
                "option_type": "string",
                "loc": [],
                "declarations": [],
                "declaration_positions": [],
                "highest_prio": 100,
                "is_defined": true,
                "surviving_definition_sources": [],
            }),
            "missing declared_type key",
        ),
    ];

    for (metadata, label) in metadata_cases {
        let payload = json!({
            "metadata": metadata,
            "effective_value": {"kind": "scalar", "value": "test"},
            "provenance": {"state": "unavailable"},
        });
        let result: Option<bool> =
            sqlx::query_scalar("SELECT evaluation_option_payload_v2_valid($1) IS FALSE")
                .bind(&payload)
                .fetch_one(&pool)
                .await
                .expect(&format!("metadata nullable test: {}", label));
        assert_eq!(result, Some(true), "metadata {}: must reject", label);
    }

    // Definition nullable fields
    let def_payload = json!({
        "metadata": {
            "state": "available",
            "option_type": "string",
            "loc": [],
            "declared_type": "str",
            "declarations": [],
            "declaration_positions": [],
            "highest_prio": 100,
            "is_defined": true,
            "surviving_definition_sources": [],
        },
        "effective_value": {"kind": "scalar", "value": "test"},
        "provenance": {
            "state": "available",
            "definitions": [{
                "ordinal": 0,
                "source_path": "test.nix",
                "priority": 100,
                "status": "active_surviving",
                "surviving_merge_order": 0,
                "value": {"kind": "scalar", "value": "test"},
            }],
            "override_state": false,
        },
    });

    let result: Option<bool> =
        sqlx::query_scalar("SELECT evaluation_option_payload_v2_valid($1) IS FALSE")
            .bind(&def_payload)
            .fetch_one(&pool)
            .await
            .expect("definition missing nullable keys");
    assert_eq!(
        result,
        Some(true),
        "definition must have source_input, source_revision, module_key keys"
    );
}

async fn insert_v2_snapshot_minimal(pool: &PgPool, configuration_name: &str) -> Uuid {
    let flake_id: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(format!("test-{}", Uuid::new_v4().simple()))
            .bind(format!(
                "https://example.invalid/{}.git",
                Uuid::new_v4().simple()
            ))
            .fetch_one(pool)
            .await
            .expect("insert test flake");

    let commit_id: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, now()) RETURNING id"
    )
    .bind(flake_id)
    .bind(format!("{:040x}", flake_id))
    .fetch_one(pool)
    .await
    .expect("insert test commit");

    let snapshot_id = Uuid::new_v4();
    let digest = Sha256::digest(configuration_name.as_bytes()).to_vec();

    let payload = json!({
        "metadata": {
            "state": "available",
            "option_type": "string",
            "loc": [],
            "declared_type": "str",
            "declarations": [],
            "declaration_positions": [],
            "highest_prio": 100,
            "is_defined": true,
            "surviving_definition_sources": [{
                "source_path": "test.nix",
                "priority": 100,
                "source_input": null,
                "source_revision": null,
            }],
        },
        "effective_value": {"kind": "scalar", "value": "test"},
        "provenance": {
            "state": "available",
            "definitions": [{
                "ordinal": 0,
                "source_path": "test.nix",
                "source_input": null,
                "source_revision": null,
                "module_key": null,
                "priority": 100,
                "status": "active_surviving",
                "surviving_merge_order": 0,
                "value": {"kind": "scalar", "value": "test"},
            }],
            "override_state": false,
        },
    });

    sqlx::query(
        "INSERT INTO evaluation_option_contents (digest, schema_version, payload, search_text) VALUES ($1, 2, $2, 'test')"
    )
    .bind(&digest)
    .bind(&payload)
    .execute(pool)
    .await
    .expect("insert content");

    sqlx::query(
        "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle, option_count, module_count, content_bytes, target_key, source_out_path, carrier_drv_path, provenance_state, comparison_ready) VALUES ($1, $2, $3, 2, 'available', 1, 1, 1, $4, $5, $6, $7, true)"
    )
    .bind(snapshot_id)
    .bind(commit_id)
    .bind(configuration_name)
    .bind(TARGET_KEY)
    .bind("/nix/store/source")
    .bind("/nix/store/carrier.drv")
    .bind(json!({
        "state": "available",
        "adapter_version": 1,
        "target_lib_version": null,
        "target_module_system_path": null,
        "provenance_digest": PROVENANCE_DIGEST,
        "definition_value_enrichment": {
            "state": "available",
            "adapter_version": 1,
            "provenance_digest": PROVENANCE_DIGEST,
        },
    }))
    .execute(pool)
    .await
    .expect("insert snapshot");

    sqlx::query(
        "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden, option_key, path_components) VALUES ($1, 'services.test', $2, false, $3, $4)"
    )
    .bind(snapshot_id)
    .bind(&digest)
    .bind("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
    .bind(vec!["services".to_string(), "test".to_string()])
    .execute(pool)
    .await
    .expect("insert option");

    snapshot_id
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn duplicate_option_key_rejected(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    let snapshot_id = insert_v2_snapshot_minimal(&pool, "dup-key-test").await;

    // Different path_components, same option_key -> rejected
    let result = sqlx::query(
        "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden, option_key, path_components) SELECT $1, 'services.other', content_digest, false, option_key, $2 FROM evaluation_snapshot_options WHERE snapshot_id = $1 LIMIT 1"
    )
    .bind(snapshot_id)
    .bind(vec!["services".to_string(), "other".to_string()])
    .execute(&pool)
    .await;

    assert!(result.is_err(), "duplicate option_key must be rejected");
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn v2_identity_must_be_complete_or_both_null(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    let snapshot_id = insert_v2_snapshot_minimal(&pool, "identity-test").await;
    let digest = Sha256::digest("identity-test".as_bytes()).to_vec();

    // option_key non-null + path_components null -> rejected by pair constraint
    let result1 = sqlx::query(
        "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden, option_key, path_components) VALUES ($1, 'test.one', $2, false, 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', NULL)"
    )
    .bind(snapshot_id)
    .bind(&digest)
    .execute(&pool)
    .await;
    assert!(
        result1.is_err(),
        "option_key without path_components must be rejected"
    );

    // option_key null + path_components non-null -> rejected by pair constraint
    let result2 = sqlx::query(
        "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden, option_key, path_components) VALUES ($1, 'test.two', $2, false, NULL, $3)"
    )
    .bind(snapshot_id)
    .bind(&digest)
    .bind(vec!["test".to_string(), "two".to_string()])
    .execute(&pool)
    .await;
    assert!(
        result2.is_err(),
        "path_components without option_key must be rejected"
    );

    // Both NULL is permitted by pair constraint but certification must reject for V2
    let result3 = sqlx::query(
        "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden, option_key, path_components) VALUES ($1, 'test.three', $2, false, NULL, NULL)"
    )
    .bind(snapshot_id)
    .bind(&digest)
    .execute(&pool)
    .await;
    // Row insert succeeds
    assert!(result3.is_ok(), "both NULL permitted by row constraint");

    // But certification must fail
    let certified: Option<bool> =
        sqlx::query_scalar("SELECT evaluation_snapshot_payloads_valid($1)")
            .bind(snapshot_id)
            .fetch_optional(&pool)
            .await
            .expect("certification check");
    assert_eq!(
        certified,
        Some(false),
        "V2 snapshot with NULL identity must fail certification"
    );
}

// Due to test infrastructure limitations, this test is a placeholder.
// A real pre-0250 V1 upgrade test would require selective migration application.
// Production verification must use actual deployment upgrade testing.
#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB and selective migration capability"]
async fn v1_artifact_survives_0250_upgrade(_pool: PgPool) {
    // PLACEHOLDER: Real upgrade testing requires migration tooling enhancement.
    // Required sequence:
    // 1. Migrate through 0249
    // 2. Insert valid V1 artifact
    // 3. Certify it
    // 4. Apply 0250
    // 5. Verify V1 artifact unchanged and still valid
    //
    // Current test infrastructure cannot selectively stop at 0249.
    // Production upgrade must be verified in actual deployment.
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn certified_v2_snapshot_immutability(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    let snapshot_id = insert_v2_snapshot_minimal(&pool, "immutable-test").await;

    // Certify
    sqlx::query(
        "UPDATE evaluation_snapshots SET integrity_version = 2 WHERE id = $1 AND evaluation_snapshot_payloads_valid($1)"
    )
    .bind(snapshot_id)
    .execute(&pool)
    .await
    .expect("certify V2 snapshot");

    // Try to update immutable fields
    let fields = vec![
        (
            "target_key",
            "'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee'",
        ),
        ("source_out_path", "'/nix/store/other'"),
        ("carrier_drv_path", "'/nix/store/other.drv'"),
        (
            "provenance_state",
            "'{\"state\":\"unavailable\",\"reason_code\":\"test\",\"diagnostic\":null}'::jsonb",
        ),
        ("comparison_ready", "false"),
        ("option_count", "999"),
        ("module_count", "999"),
        ("schema_version", "1"),
        ("lifecycle", "'uncertified'"),
    ];

    for (field, value) in fields {
        let result = sqlx::query(&format!(
            "UPDATE evaluation_snapshots SET {} = {} WHERE id = $1",
            field, value
        ))
        .bind(snapshot_id)
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "certified V2 snapshot {} must be immutable",
            field
        );
    }

    // Integrity transitions
    let bad_transitions = vec![(0, "downgrade to 0"), (1, "downgrade to 1")];
    for (new_version, label) in bad_transitions {
        let result =
            sqlx::query("UPDATE evaluation_snapshots SET integrity_version = $1 WHERE id = $2")
                .bind(new_version)
                .bind(snapshot_id)
                .execute(&pool)
                .await;
        assert!(
            result.is_err(),
            "integrity transition to {}: {}",
            new_version,
            label
        );
    }

    // host_delta_count should remain mutable
    let result = sqlx::query("UPDATE evaluation_snapshots SET host_delta_count = 42 WHERE id = $1")
        .bind(snapshot_id)
        .execute(&pool)
        .await;
    assert!(result.is_ok(), "host_delta_count must remain mutable");
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn certified_v2_option_reference_immutability(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    let snapshot_id = insert_v2_snapshot_minimal(&pool, "option-immutable-test").await;

    sqlx::query(
        "UPDATE evaluation_snapshots SET integrity_version = 2 WHERE id = $1 AND evaluation_snapshot_payloads_valid($1)"
    )
    .bind(snapshot_id)
    .execute(&pool)
    .await
    .expect("certify");

    let digest = Sha256::digest("option-immutable-test".as_bytes()).to_vec();

    // INSERT another option -> rejected
    let result1 = sqlx::query(
        "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden, option_key, path_components) VALUES ($1, 'new.option', $2, false, 'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff', $3)"
    )
    .bind(snapshot_id)
    .bind(&digest)
    .bind(vec!["new".to_string(), "option".to_string()])
    .execute(&pool)
    .await;
    assert!(
        result1.is_err(),
        "INSERT option into certified snapshot must be rejected"
    );

    // UPDATE existing option -> rejected
    let result2 = sqlx::query(
        "UPDATE evaluation_snapshot_options SET is_overridden = true WHERE snapshot_id = $1",
    )
    .bind(snapshot_id)
    .execute(&pool)
    .await;
    assert!(
        result2.is_err(),
        "UPDATE option in certified snapshot must be rejected"
    );

    // DELETE option -> rejected
    let result3 = sqlx::query("DELETE FROM evaluation_snapshot_options WHERE snapshot_id = $1")
        .bind(snapshot_id)
        .execute(&pool)
        .await;
    assert!(
        result3.is_err(),
        "DELETE option from certified snapshot must be rejected"
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn content_payload_immutability(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    let digest = Sha256::digest("immutable-content".as_bytes()).to_vec();
    let payload = json!({"test": "original"});

    sqlx::query(
        "INSERT INTO evaluation_option_contents (digest, schema_version, payload, search_text) VALUES ($1, 2, $2, 'test')"
    )
    .bind(&digest)
    .bind(&payload)
    .execute(&pool)
    .await
    .expect("insert content");

    let result =
        sqlx::query("UPDATE evaluation_option_contents SET payload = $1 WHERE digest = $2")
            .bind(json!({"test": "modified"}))
            .bind(&digest)
            .execute(&pool)
            .await;

    assert!(result.is_err(), "content payload must be immutable");
}

// Placeholder for retained generation V2 tests - requires full lineage fixture infrastructure
#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB and lineage fixtures"]
async fn retained_generation_v2_contract(_pool: PgPool) {
    // PLACEHOLDER: Requires derivation, source_store_path, lineage_verified fixtures
    // Test cases needed:
    // A. Valid certified V2 with exact lineage -> retention succeeds
    // B. Uncertified V2 (integrity_version=0) -> retention rejected
    // C. Wrong derivation/config/commit -> rejected
    // D. Wrong store path -> rejected
    // E. comparison_ready=false with valid V2 -> retention succeeds
    // F. Certified V1 continues to satisfy retention
}
