// TASK-440: V2 config snapshot database contract remediation tests.
//
// These tests verify total boolean validator contracts, required nullable fields,
// identity constraints, immutability, retention, and upgrade compatibility.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, migrate::Migrate};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

const TARGET_KEY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PROVENANCE_DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

async fn apply_migrations_through(pool: &PgPool, version: i64) {
    let mut connection = pool.acquire().await.expect("acquire migration connection");
    connection
        .ensure_migrations_table()
        .await
        .expect("create migrations table");
    for migration in MIGRATOR
        .iter()
        .filter(|migration| migration.version <= version)
    {
        connection
            .apply(migration)
            .await
            .unwrap_or_else(|error| panic!("apply migration {}: {error}", migration.version));
    }
}

async fn apply_migration_version(pool: &PgPool, version: i64) {
    let migration = MIGRATOR
        .iter()
        .find(|migration| migration.version == version)
        .unwrap_or_else(|| panic!("migration {version} is not embedded"));
    let mut connection = pool.acquire().await.expect("acquire migration connection");
    connection
        .apply(migration)
        .await
        .unwrap_or_else(|error| panic!("apply migration {version}: {error}"));
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn total_boolean_validators_never_return_null(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    let sql_null_cases = [
        "evaluation_safe_error_v2_valid(NULL::jsonb)",
        "evaluation_definition_source_v2_valid(NULL::jsonb)",
        "evaluation_option_payload_v2_valid(NULL::jsonb)",
        "evaluation_snapshot_v2_provenance_valid(NULL::jsonb, false)",
    ];
    for expression in sql_null_cases {
        let query = format!("SELECT {expression} IS FALSE");
        let result: bool = sqlx::query_scalar(&query)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|error| panic!("SQL NULL validator assertion {expression}: {error}"));
        assert!(result, "{expression} must return FALSE, not SQL NULL");
    }

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

async fn insert_v2_snapshot_minimal(
    pool: &PgPool,
    configuration_name: &str,
    comparison_ready: bool,
) -> Uuid {
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

    let definition_value = if comparison_ready {
        json!({"kind": "scalar", "value": "test"})
    } else {
        Value::Null
    };
    let enrichment = if comparison_ready {
        json!({
            "state": "available",
            "adapter_version": 1,
            "provenance_digest": PROVENANCE_DIGEST,
        })
    } else {
        json!({
            "state": "unavailable",
            "reason_code": "not_evaluated",
            "diagnostic": null,
        })
    };
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
                "value": definition_value,
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

    let has_inventory_columns: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'evaluation_snapshots' AND column_name = 'option_inventory_complete')",
    )
    .fetch_one(pool)
    .await
    .expect("inspect inventory migration state");
    let statement = if has_inventory_columns {
        "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle, option_count, module_count, content_bytes, target_key, source_out_path, carrier_drv_path, provenance_state, comparison_ready, option_inventory_complete, option_inventory_diagnostics, option_inventory_diagnostics_truncated) VALUES ($1, $2, $3, 2, 'available', 1, 1, 1, $4, $5, $6, $7, $8, true, '[]'::jsonb, false)"
    } else {
        "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle, option_count, module_count, content_bytes, target_key, source_out_path, carrier_drv_path, provenance_state, comparison_ready) VALUES ($1, $2, $3, 2, 'available', 1, 1, 1, $4, $5, $6, $7, $8)"
    };
    sqlx::query(statement)
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
            "definition_value_enrichment": enrichment,
        }))
        .bind(comparison_ready)
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

#[sqlx::test(migrations = false)]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn migration_0254_backfills_only_preexisting_v2_as_complete(pool: PgPool) {
    apply_migrations_through(&pool, 253).await;
    let v2_id = insert_v2_snapshot_minimal(&pool, "pre-0254-v2", true).await;
    sqlx::query("UPDATE evaluation_snapshots SET integrity_version = 2 WHERE id = $1")
        .bind(v2_id)
        .execute(&pool)
        .await
        .expect("certify pre-0254 V2 artifact");
    let commit_id: i32 =
        sqlx::query_scalar("SELECT commit_id FROM evaluation_snapshots WHERE id = $1")
            .bind(v2_id)
            .fetch_one(&pool)
            .await
            .expect("load V2 commit");
    let v1_id = Uuid::new_v4();
    sqlx::query("INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle) VALUES ($1, $2, 'pre-0254-v1', 1, 'failed')")
        .bind(v1_id)
        .bind(commit_id)
        .execute(&pool)
        .await
        .expect("insert pre-0254 V1 row");

    apply_migration_version(&pool, 254).await;

    let v2: (Option<bool>, Option<Value>, Option<bool>, i16) = sqlx::query_as(
        "SELECT option_inventory_complete, option_inventory_diagnostics, option_inventory_diagnostics_truncated, integrity_version FROM evaluation_snapshots WHERE id = $1",
    )
    .bind(v2_id)
    .fetch_one(&pool)
    .await
    .expect("load migrated V2 row");
    assert_eq!(v2, (Some(true), Some(json!([])), Some(false), 2));
    let v1: (Option<bool>, Option<Value>, Option<bool>) = sqlx::query_as(
        "SELECT option_inventory_complete, option_inventory_diagnostics, option_inventory_diagnostics_truncated FROM evaluation_snapshots WHERE id = $1",
    )
    .bind(v1_id)
    .fetch_one(&pool)
    .await
    .expect("load migrated V1 row");
    assert_eq!(v1, (None, None, None));
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn migration_0254_validates_inventory_diagnostics_and_immutability(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");
    let valid_partial = json!([{
        "path": ["services", "poison"],
        "code": "unreadable_option_subtree",
        "message": "Option subtree could not be inspected",
    }]);
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT evaluation_option_inventory_diagnostics_v2_valid($1, false, false)",
        )
        .bind(&valid_partial)
        .fetch_one(&pool)
        .await
        .expect("validate partial diagnostics")
    );
    for invalid in [
        json!([]),
        json!([{"path": [], "code": "unreadable_option_subtree", "message": "Option subtree could not be inspected"}]),
        json!([{"path": ["services", ""], "code": "unreadable_option_subtree", "message": "Option subtree could not be inspected"}]),
        json!([{"path": ["services"], "code": "unknown", "message": "trace: secret"}]),
    ] {
        assert!(
            !sqlx::query_scalar::<_, bool>(
                "SELECT evaluation_option_inventory_diagnostics_v2_valid($1, false, false)",
            )
            .bind(invalid)
            .fetch_one(&pool)
            .await
            .expect("reject invalid diagnostics")
        );
    }
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT evaluation_option_inventory_diagnostics_v2_valid('[]'::jsonb, true, false)",
        )
        .fetch_one(&pool)
        .await
        .expect("validate complete inventory")
    );
    let bounded = Value::Array(
        (0..128)
            .map(|index| {
                json!({
                    "path": [format!("poison{index:03}")],
                    "code": "unreadable_option_subtree",
                    "message": "Option subtree could not be inspected",
                })
            })
            .collect(),
    );
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT evaluation_option_inventory_diagnostics_v2_valid($1, false, true)",
        )
        .bind(&bounded)
        .fetch_one(&pool)
        .await
        .expect("validate bounded truncated diagnostics")
    );
    let redaction_deduplicated = json!([{
        "path": ["[REDACTED]"],
        "code": "unreadable_option_subtree",
        "message": "Option subtree could not be inspected",
    }]);
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT evaluation_option_inventory_diagnostics_v2_valid($1, false, true)",
        )
        .bind(redaction_deduplicated)
        .fetch_one(&pool)
        .await
        .expect("validate truncated diagnostics after redaction deduplication")
    );
    let mut over_limit = bounded
        .as_array()
        .expect("bounded diagnostics array")
        .clone();
    over_limit.push(json!({
        "path": ["poison128"],
        "code": "unreadable_option_subtree",
        "message": "Option subtree could not be inspected",
    }));
    assert!(
        !sqlx::query_scalar::<_, bool>(
            "SELECT evaluation_option_inventory_diagnostics_v2_valid($1, false, true)",
        )
        .bind(json!(over_limit))
        .fetch_one(&pool)
        .await
        .expect("reject diagnostics above the retained detail budget")
    );
    let adversarial = json!([
        {"path": ["Zoo"], "code": "unreadable_option_subtree", "message": "Option subtree could not be inspected"},
        {"path": ["alpha"], "code": "unreadable_option_subtree", "message": "Option subtree could not be inspected"},
        {"path": ["Ångström"], "code": "unreadable_option_subtree", "message": "Option subtree could not be inspected"},
    ]);
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT evaluation_option_inventory_diagnostics_v2_valid($1, false, false)"
        )
        .bind(&adversarial)
        .fetch_one(&pool)
        .await
        .expect("validate bytewise mixed-case Unicode diagnostic ordering")
    );
    let mut reversed = adversarial.as_array().expect("array").clone();
    reversed.reverse();
    assert!(
        !sqlx::query_scalar::<_, bool>(
            "SELECT evaluation_option_inventory_diagnostics_v2_valid($1, false, false)"
        )
        .bind(json!(reversed))
        .fetch_one(&pool)
        .await
        .expect("reject non-bytewise diagnostic ordering")
    );

    let snapshot_id = insert_v2_snapshot_minimal(&pool, "immutable-inventory", true).await;
    sqlx::query("UPDATE evaluation_snapshots SET integrity_version = 2 WHERE id = $1")
        .bind(snapshot_id)
        .execute(&pool)
        .await
        .expect("certify inventory fixture");
    assert!(sqlx::query("UPDATE evaluation_snapshots SET option_inventory_complete = false, option_inventory_diagnostics = $2, option_inventory_diagnostics_truncated = false WHERE id = $1")
        .bind(snapshot_id)
        .bind(valid_partial)
        .execute(&pool)
        .await
        .is_err());
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn duplicate_option_key_rejected(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    let snapshot_id = insert_v2_snapshot_minimal(&pool, "dup-key-test", true).await;

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

    let snapshot_id = insert_v2_snapshot_minimal(&pool, "identity-test", true).await;
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

#[sqlx::test(migrations = false)]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn v1_artifact_survives_0250_upgrade(pool: PgPool) {
    apply_migrations_through(&pool, 249).await;

    let flake_id: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(format!("v1-upgrade-{}", Uuid::new_v4().simple()))
            .bind(format!(
                "https://example.invalid/{}.git",
                Uuid::new_v4().simple()
            ))
            .fetch_one(&pool)
            .await
            .expect("insert V1 flake");
    let commit_id: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, now()) RETURNING id",
    )
    .bind(flake_id)
    .bind(format!("{flake_id:040x}"))
    .fetch_one(&pool)
    .await
    .expect("insert V1 commit");

    let snapshot_id = Uuid::new_v4();
    let digest = Sha256::digest(b"v1-upgrade-content").to_vec();
    let payload = json!({
        "declared_type": "string",
        "value": {"kind": "scalar", "value": "safe"},
        "definitions": [],
        "overridden": false,
    });
    sqlx::query(
        "INSERT INTO evaluation_option_contents (digest, schema_version, payload, search_text) VALUES ($1, 1, $2, $3)",
    )
    .bind(&digest)
    .bind(&payload)
    .bind("v1-upgrade-search")
    .execute(&pool)
    .await
    .expect("insert V1 content");
    sqlx::query(
        "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle, option_count, module_count, content_bytes) VALUES ($1, $2, 'host', 1, 'available', 1, 0, 17)",
    )
    .bind(snapshot_id)
    .bind(commit_id)
    .execute(&pool)
    .await
    .expect("insert V1 snapshot");
    sqlx::query(
        "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden) VALUES ($1, 'services.test', $2, false)",
    )
    .bind(snapshot_id)
    .bind(&digest)
    .execute(&pool)
    .await
    .expect("insert V1 option reference");
    sqlx::query(
        "UPDATE evaluation_snapshots SET integrity_version = 1 WHERE id = $1 AND evaluation_snapshot_payloads_valid($1)",
    )
    .bind(snapshot_id)
    .execute(&pool)
    .await
    .expect("certify V1 snapshot");

    let before = sqlx::query_as::<_, (i32, String, i32, i16, String, i32, i32, i64, String, Vec<u8>, bool, i32, Value, String)>(
        "SELECT s.commit_id, s.configuration_name, s.schema_version, s.integrity_version, s.lifecycle, s.option_count, s.module_count, s.content_bytes, o.option_path, o.content_digest, o.is_overridden, c.schema_version, c.payload, c.search_text FROM evaluation_snapshots s JOIN evaluation_snapshot_options o ON o.snapshot_id = s.id JOIN evaluation_option_contents c ON c.digest = o.content_digest WHERE s.id = $1",
    )
    .bind(snapshot_id)
    .fetch_one(&pool)
    .await
    .expect("capture V1 state before upgrade");

    apply_migration_version(&pool, 250).await;

    let after = sqlx::query_as::<_, (i32, String, i32, i16, String, i32, i32, i64, String, Vec<u8>, bool, i32, Value, String)>(
        "SELECT s.commit_id, s.configuration_name, s.schema_version, s.integrity_version, s.lifecycle, s.option_count, s.module_count, s.content_bytes, o.option_path, o.content_digest, o.is_overridden, c.schema_version, c.payload, c.search_text FROM evaluation_snapshots s JOIN evaluation_snapshot_options o ON o.snapshot_id = s.id JOIN evaluation_option_contents c ON c.digest = o.content_digest WHERE s.id = $1",
    )
    .bind(snapshot_id)
    .fetch_one(&pool)
    .await
    .expect("capture V1 state after upgrade");
    let after_extra = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>, Option<Value>, Option<bool>, Option<String>, Option<Vec<String>>)>(
        "SELECT s.target_key, s.source_out_path, s.carrier_drv_path, s.provenance_state, s.comparison_ready, o.option_key, o.path_components FROM evaluation_snapshots s JOIN evaluation_snapshot_options o ON o.snapshot_id = s.id WHERE s.id = $1",
    )
    .bind(snapshot_id)
    .fetch_one(&pool)
    .await
    .expect("capture upgraded V1 nullable state");

    assert_eq!(after.0, before.0);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2);
    assert_eq!(after.3, before.3);
    assert_eq!(after.4, before.4);
    assert_eq!(after.5, before.5);
    assert_eq!(after.6, before.6);
    assert_eq!(after.7, before.7);
    assert_eq!(after.8, before.8);
    assert_eq!(after.9, before.9);
    assert_eq!(after.10, before.10);
    assert_eq!(after.11, before.11);
    assert_eq!(after.12, before.12);
    assert_eq!(after.13, before.13);
    assert!(after_extra.0.is_none());
    assert!(after_extra.1.is_none());
    assert!(after_extra.2.is_none());
    assert!(after_extra.3.is_none());
    assert!(after_extra.4.is_none());
    assert!(after_extra.5.is_none());
    assert!(after_extra.6.is_none());
    let valid: bool = sqlx::query_scalar("SELECT evaluation_snapshot_payloads_valid($1)")
        .bind(snapshot_id)
        .fetch_one(&pool)
        .await
        .expect("validate upgraded V1 snapshot");
    assert!(valid);

    let immutable = sqlx::query("UPDATE evaluation_snapshots SET content_bytes = 18 WHERE id = $1")
        .bind(snapshot_id)
        .execute(&pool)
        .await;
    assert!(
        immutable.is_err(),
        "upgraded certified V1 must remain immutable"
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn certified_v2_snapshot_immutability(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    let snapshot_id = insert_v2_snapshot_minimal(&pool, "immutable-test", true).await;

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

    let snapshot_id = insert_v2_snapshot_minimal(&pool, "option-immutable-test", true).await;

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

async fn prepare_lineage(pool: &PgPool, snapshot_id: Uuid) -> (Uuid, i32, i32, String, String) {
    let (commit_id, configuration_name, flake_id): (i32, String, i32) = sqlx::query_as(
        "SELECT snapshot.commit_id, snapshot.configuration_name, commit.flake_id FROM evaluation_snapshots snapshot JOIN commits commit ON commit.id = snapshot.commit_id WHERE snapshot.id = $1",
    )
    .bind(snapshot_id)
    .fetch_one(pool)
    .await
    .expect("load snapshot lineage identity");
    let store_path = format!("/nix/store/{}-system", Uuid::new_v4().simple());
    let system_id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems (hostname, public_key, flake_id, derivation, system_configuration_name) VALUES ($1, 'test-public-key', $2, $3, $4) RETURNING id",
    )
    .bind(format!("host-{}", Uuid::new_v4().simple()))
    .bind(flake_id)
    .bind(&configuration_name)
    .bind(&configuration_name)
    .fetch_one(pool)
    .await
    .expect("insert lineage system");
    let derivation_id: i32 = sqlx::query_scalar(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, attempt_count, expected_store_path) VALUES ($1, 'nixos', $2, 1, 0, $3) RETURNING id",
    )
    .bind(commit_id)
    .bind(&configuration_name)
    .bind(&store_path)
    .fetch_one(pool)
    .await
    .expect("insert lineage derivation");
    sqlx::query("UPDATE derivations SET store_path = $2 WHERE id = $1")
        .bind(derivation_id)
        .bind(&store_path)
        .execute(pool)
        .await
        .expect("set derivation store path");
    (
        system_id,
        derivation_id,
        commit_id,
        configuration_name,
        store_path,
    )
}

async fn insert_retained_generation(
    pool: &PgPool,
    system_id: Uuid,
    generation: i32,
    snapshot_id: Uuid,
    derivation_id: i32,
    commit_id: i32,
    source_store_path: &str,
    configuration_name: &str,
    lineage_verified: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO evaluation_generation_snapshots (system_id, generation, snapshot_id, derivation_id, commit_id, source_store_path, configuration_name, lineage_verified) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(system_id)
    .bind(generation)
    .bind(snapshot_id)
    .bind(derivation_id)
    .bind(commit_id)
    .bind(source_store_path)
    .bind(configuration_name)
    .bind(lineage_verified)
    .execute(pool)
    .await
    .map(|_| ())
}

async fn insert_v1_snapshot_post_0250(pool: &PgPool, name: &str) -> Uuid {
    let flake_id: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(format!("v1-retain-{}", Uuid::new_v4().simple()))
            .bind(format!(
                "https://example.invalid/{}.git",
                Uuid::new_v4().simple()
            ))
            .fetch_one(pool)
            .await
            .expect("insert V1 retention flake");
    let commit_id: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, now()) RETURNING id",
    )
    .bind(flake_id)
    .bind(format!("{flake_id:040x}"))
    .fetch_one(pool)
    .await
    .expect("insert V1 retention commit");
    let snapshot_id = Uuid::new_v4();
    let digest = Sha256::digest(name.as_bytes()).to_vec();
    let payload = json!({
        "declared_type": "string",
        "value": {"kind": "scalar", "value": "safe"},
        "definitions": [],
        "overridden": false,
    });
    sqlx::query("INSERT INTO evaluation_option_contents (digest, schema_version, payload, search_text) VALUES ($1, 1, $2, 'v1')")
        .bind(&digest)
        .bind(&payload)
        .execute(pool)
        .await
        .expect("insert V1 retention content");
    sqlx::query("INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle, option_count, module_count, content_bytes) VALUES ($1, $2, $3, 1, 'available', 1, 0, 1)")
        .bind(snapshot_id)
        .bind(commit_id)
        .bind(name)
        .execute(pool)
        .await
        .expect("insert V1 retention snapshot");
    sqlx::query("INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden) VALUES ($1, 'services.v1', $2, false)")
        .bind(snapshot_id)
        .bind(&digest)
        .execute(pool)
        .await
        .expect("insert V1 retention option");
    sqlx::query("UPDATE evaluation_snapshots SET integrity_version = 1 WHERE id = $1 AND evaluation_snapshot_payloads_valid($1)")
        .bind(snapshot_id)
        .execute(pool)
        .await
        .expect("certify V1 retention snapshot");
    snapshot_id
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB and lineage fixtures"]
async fn retained_generation_v2_contract(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    let valid_v2 = insert_v2_snapshot_minimal(&pool, "retained-v2", true).await;
    sqlx::query("UPDATE evaluation_snapshots SET integrity_version = 2 WHERE id = $1 AND evaluation_snapshot_payloads_valid($1)")
        .bind(valid_v2)
        .execute(&pool)
        .await
        .expect("certify retained V2");
    let (system_id, derivation_id, commit_id, configuration_name, store_path) =
        prepare_lineage(&pool, valid_v2).await;
    insert_retained_generation(
        &pool,
        system_id,
        1,
        valid_v2,
        derivation_id,
        commit_id,
        &store_path,
        &configuration_name,
        true,
    )
    .await
    .expect("exact certified V2 retention must succeed");

    let uncertified = insert_v2_snapshot_minimal(&pool, "uncertified-v2", true).await;
    let (
        uncertified_system,
        uncertified_derivation,
        uncertified_commit,
        uncertified_name,
        uncertified_store,
    ) = prepare_lineage(&pool, uncertified).await;
    assert!(
        insert_retained_generation(
            &pool,
            uncertified_system,
            1,
            uncertified,
            uncertified_derivation,
            uncertified_commit,
            &uncertified_store,
            &uncertified_name,
            true
        )
        .await
        .is_err()
    );

    assert!(
        insert_retained_generation(
            &pool,
            system_id,
            2,
            valid_v2,
            derivation_id,
            commit_id,
            "/nix/store/wrong",
            &configuration_name,
            true
        )
        .await
        .is_err()
    );
    assert!(
        insert_retained_generation(
            &pool,
            system_id,
            3,
            valid_v2,
            derivation_id,
            commit_id,
            &store_path,
            "wrong-config",
            true
        )
        .await
        .is_err()
    );
    let wrong_derivation: i32 = sqlx::query_scalar(
        "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, attempt_count, expected_store_path) VALUES ($1, 'nixos', 'wrong-derivation', 1, 0, $2) RETURNING id",
    )
    .bind(commit_id)
    .bind(&store_path)
    .fetch_one(&pool)
    .await
    .expect("insert wrong derivation");
    assert!(
        insert_retained_generation(
            &pool,
            system_id,
            4,
            valid_v2,
            wrong_derivation,
            commit_id,
            &store_path,
            &configuration_name,
            true
        )
        .await
        .is_err()
    );
    assert!(
        insert_retained_generation(
            &pool,
            system_id,
            5,
            valid_v2,
            derivation_id,
            commit_id,
            &store_path,
            &configuration_name,
            false
        )
        .await
        .is_err()
    );

    let unready = insert_v2_snapshot_minimal(&pool, "unready-v2", false).await;
    sqlx::query("UPDATE evaluation_snapshots SET integrity_version = 2 WHERE id = $1 AND evaluation_snapshot_payloads_valid($1)")
        .bind(unready)
        .execute(&pool)
        .await
        .expect("certify comparison-unready V2");
    let (unready_system, unready_derivation, unready_commit, unready_name, unready_store) =
        prepare_lineage(&pool, unready).await;
    insert_retained_generation(
        &pool,
        unready_system,
        1,
        unready,
        unready_derivation,
        unready_commit,
        &unready_store,
        &unready_name,
        true,
    )
    .await
    .expect("comparison_ready=false V2 retention must succeed");

    let v1 = insert_v1_snapshot_post_0250(&pool, "retained-v1").await;
    let (v1_system, v1_derivation, v1_commit, v1_name, v1_store) = prepare_lineage(&pool, v1).await;
    insert_retained_generation(
        &pool,
        v1_system,
        1,
        v1,
        v1_derivation,
        v1_commit,
        &v1_store,
        &v1_name,
        true,
    )
    .await
    .expect("certified V1 retention must succeed");
}
