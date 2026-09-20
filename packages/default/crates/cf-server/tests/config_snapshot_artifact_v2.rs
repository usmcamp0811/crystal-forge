use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

const TARGET_KEY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PROVENANCE_DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

fn source(path: &str, priority: Option<i64>) -> Value {
    json!({
        "source_path": path,
        "priority": priority,
        "source_input": null,
        "source_revision": null,
    })
}

fn definition(ordinal: i64, merge_order: Option<i64>, status: &str, value: Value) -> Value {
    json!({
        "ordinal": ordinal,
        "source_path": format!("modules/{ordinal}.nix"),
        "source_input": null,
        "source_revision": null,
        "module_key": null,
        "priority": 100,
        "status": status,
        "surviving_merge_order": merge_order,
        "value": value,
    })
}

fn metadata() -> Value {
    json!({
        "state": "available",
        "option_type": "string",
        "loc": [],
        "declared_type": "str",
        "declarations": [],
        "declaration_positions": [],
        "highest_prio": 100,
        "is_defined": true,
        "surviving_definition_sources": [source("modules/0.nix", Some(100))],
    })
}

fn local_payload(definitions: Vec<Value>, override_state: bool) -> Value {
    json!({
        "metadata": metadata(),
        "effective_value": {"kind": "scalar", "value": "safe"},
        "provenance": {
            "state": "available",
            "definitions": definitions,
            "override_state": override_state,
        },
    })
}

fn global_available(enrichment: Value) -> Value {
    json!({
        "state": "available",
        "adapter_version": 1,
        "target_lib_version": null,
        "target_module_system_path": null,
        "provenance_digest": PROVENANCE_DIGEST,
        "definition_value_enrichment": enrichment,
    })
}

fn enrichment_available(adapter_version: i64, provenance_digest: &str) -> Value {
    json!({
        "state": "available",
        "adapter_version": adapter_version,
        "provenance_digest": provenance_digest,
    })
}

fn enrichment_unavailable() -> Value {
    json!({
        "state": "unavailable",
        "reason_code": "not_evaluated",
        "diagnostic": null,
    })
}

fn global_unavailable() -> Value {
    json!({
        "state": "unavailable",
        "reason_code": "provenance_unavailable",
        "diagnostic": null,
    })
}

fn unavailable_payload() -> Value {
    json!({
        "metadata": metadata(),
        "effective_value": {"kind": "scalar", "value": "safe"},
        "provenance": {"state": "unavailable"},
    })
}

async fn insert_snapshot(
    pool: &PgPool,
    configuration_name: &str,
    content_schema: i32,
    payload: Value,
    provenance_state: Option<Value>,
    comparison_ready: Option<bool>,
    target_key: Option<&str>,
    source_out_path: Option<&str>,
    carrier_drv_path: Option<&str>,
    option_key: Option<&str>,
    path_components: Option<&[&str]>,
    is_overridden: Option<bool>,
) -> Uuid {
    let module_count = payload
        .get("provenance")
        .and_then(|provenance| provenance.get("definitions"))
        .and_then(Value::as_array)
        .map(|definitions| {
            definitions
                .iter()
                .filter_map(|definition| definition.get("source_path").and_then(Value::as_str))
                .collect::<std::collections::BTreeSet<_>>()
                .len() as i32
        })
        .unwrap_or(0);
    let flake_id: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(format!(
                "v2-contract-{configuration_name}-{}",
                flake_id_suffix()
            ))
            .bind(format!(
                "https://example.invalid/{configuration_name}-{}.git",
                flake_id_suffix()
            ))
            .fetch_one(pool)
            .await
            .expect("insert test flake");
    let commit_id: i32 = sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, now()) RETURNING id",
    )
    .bind(flake_id)
    .bind(format!("{flake_id:040x}"))
    .fetch_one(pool)
    .await
    .expect("insert test commit");
    let snapshot_id = Uuid::new_v4();
    let digest = Sha256::digest(configuration_name.as_bytes()).to_vec();
    sqlx::query(
        "INSERT INTO evaluation_option_contents (digest, schema_version, payload, search_text) VALUES ($1, $2, $3, 'test')",
    )
    .bind(&digest)
    .bind(content_schema)
    .bind(&payload)
    .execute(pool)
    .await
    .expect("insert test content");
    sqlx::query(
        "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle, option_count, module_count, content_bytes, target_key, source_out_path, carrier_drv_path, provenance_state, comparison_ready, option_inventory_complete, option_inventory_diagnostics, option_inventory_diagnostics_truncated) VALUES ($1, $2, $3, $4, 'available', 1, $5, 1, $6, $7, $8, $9, $10, CASE WHEN $4 = 2 THEN true ELSE NULL END, CASE WHEN $4 = 2 THEN '[]'::jsonb ELSE NULL END, CASE WHEN $4 = 2 THEN false ELSE NULL END)",
    )
    .bind(snapshot_id)
    .bind(commit_id)
    .bind(configuration_name)
    .bind(content_schema)
    .bind(module_count)
    .bind(target_key)
    .bind(source_out_path)
    .bind(carrier_drv_path)
    .bind(provenance_state)
    .bind(comparison_ready)
    .execute(pool)
    .await
    .expect("insert test snapshot");
    sqlx::query(
        "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden, option_key, path_components) VALUES ($1, 'services.test.value', $2, $3, $4, $5)",
    )
    .bind(snapshot_id)
    .bind(digest)
    .bind(is_overridden)
    .bind(option_key)
    .bind(path_components.map(|parts| parts.iter().map(|part| (*part).to_string()).collect::<Vec<_>>()))
    .execute(pool)
    .await
    .expect("insert test option reference");
    snapshot_id
}

fn flake_id_suffix() -> String {
    Uuid::new_v4().simple().to_string()
}

async fn certify(pool: &PgPool, snapshot_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>(
        "UPDATE evaluation_snapshots SET integrity_version = schema_version WHERE id = $1 AND evaluation_snapshot_payloads_valid($1) RETURNING true",
    )
    .bind(snapshot_id)
    .fetch_optional(pool)
    .await
    .expect("certification query")
    .is_some()
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn v1_and_v2_certification_contract(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");
    let v1 = insert_snapshot(
        &pool,
        "v1",
        1,
        json!({
            "declared_type": "string",
            "value": {"kind": "scalar", "value": "safe"},
            "definitions": [],
            "overridden": false,
        }),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(false),
    )
    .await;
    assert!(certify(&pool, v1).await);

    let v2 = insert_snapshot(
        &pool,
        "v2",
        2,
        local_payload(
            vec![definition(
                0,
                Some(0),
                "active_surviving",
                json!({"kind": "scalar", "value": "safe"}),
            )],
            false,
        ),
        Some(global_available(enrichment_available(1, PROVENANCE_DIGEST))),
        Some(true),
        Some(TARGET_KEY),
        Some("/nix/store/source"),
        Some("/nix/store/carrier.drv"),
        Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
        Some(&["services", "test", "value"]),
        Some(false),
    )
    .await;
    assert!(certify(&pool, v2).await);

    let unavailable = insert_snapshot(
        &pool,
        "v2-unavailable",
        2,
        unavailable_payload(),
        Some(global_unavailable()),
        Some(false),
        Some(TARGET_KEY),
        Some("/nix/store/source"),
        Some("/nix/store/carrier.drv"),
        Some("dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"),
        Some(&["services", "test", "unavailable"]),
        None,
    )
    .await;
    assert!(certify(&pool, unavailable).await);

    let enrichment_unavailable_snapshot = insert_snapshot(
        &pool,
        "v2-enrichment-unavailable",
        2,
        local_payload(
            vec![definition(0, Some(0), "active_surviving", Value::Null)],
            false,
        ),
        Some(global_available(enrichment_unavailable())),
        Some(false),
        Some(TARGET_KEY),
        Some("/nix/store/source"),
        Some("/nix/store/carrier.drv"),
        Some("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"),
        Some(&["services", "test", "enrichment"]),
        Some(false),
    )
    .await;
    assert!(certify(&pool, enrichment_unavailable_snapshot).await);
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn v2_rejects_malformed_global_identity_and_definition_state(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");
    for (name, provenance, ready) in [
        (
            "missing-target-lib-version",
            json!({
                "state": "available", "adapter_version": 1,
                "target_module_system_path": null, "provenance_digest": PROVENANCE_DIGEST,
                "definition_value_enrichment": enrichment_available(1, PROVENANCE_DIGEST)
            }),
            true,
        ),
        (
            "enrichment-adapter-mismatch",
            global_available(enrichment_available(2, PROVENANCE_DIGEST)),
            true,
        ),
        (
            "enrichment-digest-mismatch",
            global_available(enrichment_available(
                1,
                "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            )),
            true,
        ),
    ] {
        let snapshot = insert_snapshot(
            &pool,
            name,
            2,
            local_payload(
                vec![definition(
                    0,
                    Some(0),
                    "active_surviving",
                    json!({"kind": "scalar", "value": "safe"}),
                )],
                false,
            ),
            Some(provenance),
            Some(ready),
            Some(TARGET_KEY),
            Some("/nix/store/source"),
            Some("/nix/store/carrier.drv"),
            Some(
                &format!("{}{}", "f", name.len())
                    .chars()
                    .cycle()
                    .take(64)
                    .collect::<String>(),
            ),
            Some(&["services", "test", "malformed"]),
            Some(false),
        )
        .await;
        assert!(
            !certify(&pool, snapshot).await,
            "{name} must fail certification"
        );
    }

    for (name, target, source_out, carrier) in [
        (
            "missing-target",
            None,
            Some("/nix/store/source"),
            Some("/nix/store/carrier.drv"),
        ),
        (
            "missing-source",
            Some(TARGET_KEY),
            None,
            Some("/nix/store/carrier.drv"),
        ),
        (
            "missing-carrier",
            Some(TARGET_KEY),
            Some("/nix/store/source"),
            None,
        ),
    ] {
        let snapshot = insert_snapshot(
            &pool,
            name,
            2,
            local_payload(
                vec![definition(
                    0,
                    Some(0),
                    "active_surviving",
                    json!({"kind": "scalar", "value": "safe"}),
                )],
                false,
            ),
            Some(global_available(enrichment_available(1, PROVENANCE_DIGEST))),
            Some(true),
            target,
            source_out,
            carrier,
            Some(
                &format!("{}{}", "a", name.len())
                    .chars()
                    .cycle()
                    .take(64)
                    .collect::<String>(),
            ),
            Some(&["services", "test", "identity"]),
            Some(false),
        )
        .await;
        assert!(
            !certify(&pool, snapshot).await,
            "{name} must fail certification"
        );
    }
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn v2_validates_merge_order_override_and_exact_identity(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");
    for (name, definitions, override_state, expected) in [
        (
            "ordered-survivors",
            vec![
                definition(
                    0,
                    Some(0),
                    "active_surviving",
                    json!({"kind": "scalar", "value": "a"}),
                ),
                definition(
                    1,
                    Some(1),
                    "active_surviving",
                    json!({"kind": "scalar", "value": "b"}),
                ),
            ],
            false,
            true,
        ),
        (
            "reordered-survivors",
            vec![
                definition(
                    0,
                    Some(1),
                    "active_surviving",
                    json!({"kind": "scalar", "value": "a"}),
                ),
                definition(
                    1,
                    Some(0),
                    "active_surviving",
                    json!({"kind": "scalar", "value": "b"}),
                ),
            ],
            false,
            true,
        ),
        (
            "merge-order-gap",
            vec![
                definition(
                    0,
                    Some(0),
                    "active_surviving",
                    json!({"kind": "scalar", "value": "a"}),
                ),
                definition(
                    1,
                    Some(2),
                    "active_surviving",
                    json!({"kind": "scalar", "value": "b"}),
                ),
            ],
            false,
            false,
        ),
        (
            "discarded-with-order",
            vec![
                definition(
                    0,
                    Some(0),
                    "active_surviving",
                    json!({"kind": "scalar", "value": "a"}),
                ),
                definition(1, Some(1), "priority_discarded", Value::Null),
            ],
            true,
            false,
        ),
        (
            "override-mismatch",
            vec![
                definition(
                    0,
                    Some(0),
                    "active_surviving",
                    json!({"kind": "scalar", "value": "a"}),
                ),
                definition(1, None, "priority_discarded", Value::Null),
            ],
            false,
            false,
        ),
    ] {
        let snapshot = insert_snapshot(
            &pool,
            name,
            2,
            local_payload(definitions, override_state),
            Some(global_available(enrichment_available(1, PROVENANCE_DIGEST))),
            Some(true),
            Some(TARGET_KEY),
            Some("/nix/store/source"),
            Some("/nix/store/carrier.drv"),
            Some(
                &format!("{}{}", "b", name.len())
                    .chars()
                    .cycle()
                    .take(64)
                    .collect::<String>(),
            ),
            Some(&["services", "test", "merge"]),
            Some(override_state),
        )
        .await;
        assert_eq!(certify(&pool, snapshot).await, expected, "{name}");
    }

    let snapshot = insert_snapshot(
        &pool,
        "exact-identity",
        2,
        local_payload(vec![], false),
        Some(global_available(enrichment_available(1, PROVENANCE_DIGEST))),
        Some(true),
        Some(TARGET_KEY),
        Some("/nix/store/source"),
        Some("/nix/store/carrier.drv"),
        Some("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"),
        Some(&["services", "test", "exact"]),
        Some(false),
    )
    .await;
    let duplicate_path = sqlx::query(
        "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden, option_key, path_components) SELECT $1, 'services.test.other', content_digest, false, '1111111111111111111111111111111111111111111111111111111111111111', path_components FROM evaluation_snapshot_options WHERE snapshot_id = $1",
    )
    .bind(snapshot)
    .execute(&pool)
    .await;
    assert!(
        duplicate_path.is_err(),
        "duplicate path identity must be rejected"
    );
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
async fn v2_malformed_payloads_rejected(pool: PgPool) {
    MIGRATOR.run(&pool).await.expect("apply migrations");

    // Metadata failed error cases
    let metadata_failed_no_code = json!({
        "state": "failed",
        "error": {"message": "test"},
    });
    let metadata_failed_no_message = json!({
        "state": "failed",
        "error": {"code": "test"},
    });

    for (metadata, label) in [
        (metadata_failed_no_code, "metadata failed missing code"),
        (
            metadata_failed_no_message,
            "metadata failed missing message",
        ),
    ] {
        let payload = json!({
            "metadata": metadata,
            "effective_value": {"kind": "scalar", "value": "test"},
            "provenance": {"state": "unavailable"},
        });
        let snapshot = insert_snapshot(
            &pool,
            label,
            2,
            payload,
            Some(global_unavailable()),
            Some(false),
            Some(TARGET_KEY),
            Some("/nix/store/source"),
            Some("/nix/store/carrier.drv"),
            Some(&format!("{:064x}", label.len())),
            Some(&["test", label]),
            None,
        )
        .await;
        assert!(
            !certify(&pool, snapshot).await,
            "{} must fail certification",
            label
        );
    }

    // Global provenance unavailable malformed diagnostic
    let bad_diagnostic = json!({
        "state": "unavailable",
        "reason_code": "test",
        "diagnostic": {"code": "x"},
    });
    let snapshot = insert_snapshot(
        &pool,
        "bad-global-diagnostic",
        2,
        unavailable_payload(),
        Some(bad_diagnostic),
        Some(false),
        Some(TARGET_KEY),
        Some("/nix/store/source"),
        Some("/nix/store/carrier.drv"),
        Some("dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"),
        Some(&["test", "diagnostic"]),
        None,
    )
    .await;
    assert!(
        !certify(&pool, snapshot).await,
        "bad global diagnostic must fail"
    );

    // Stage-2 unavailable malformed diagnostic
    let bad_enrichment = json!({
        "state": "available",
        "adapter_version": 1,
        "target_lib_version": null,
        "target_module_system_path": null,
        "provenance_digest": PROVENANCE_DIGEST,
        "definition_value_enrichment": {
            "state": "unavailable",
            "reason_code": "test",
            "diagnostic": {"message": "x"},
        },
    });
    let snapshot2 = insert_snapshot(
        &pool,
        "bad-enrichment-diagnostic",
        2,
        local_payload(vec![], false),
        Some(bad_enrichment),
        Some(false),
        Some(TARGET_KEY),
        Some("/nix/store/source"),
        Some("/nix/store/carrier.drv"),
        Some("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"),
        Some(&["test", "enrichment"]),
        Some(false),
    )
    .await;
    assert!(
        !certify(&pool, snapshot2).await,
        "bad enrichment diagnostic must fail"
    );

    // Missing definition nullable key
    let bad_def_payload = json!({
        "metadata": metadata(),
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
    let snapshot3 = insert_snapshot(
        &pool,
        "missing-def-keys",
        2,
        bad_def_payload,
        Some(global_available(enrichment_available(1, PROVENANCE_DIGEST))),
        Some(true),
        Some(TARGET_KEY),
        Some("/nix/store/source"),
        Some("/nix/store/carrier.drv"),
        Some("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"),
        Some(&["test", "def"]),
        Some(false),
    )
    .await;
    assert!(
        !certify(&pool, snapshot3).await,
        "missing definition keys must fail"
    );

    // Malformed surviving_definition_sources element
    let bad_metadata = json!({
        "state": "available",
        "option_type": "string",
        "loc": [],
        "declared_type": "str",
        "declarations": [],
        "declaration_positions": [],
        "highest_prio": 100,
        "is_defined": true,
        "surviving_definition_sources": [{"source_path": "test.nix"}],
    });
    let payload4 = json!({
        "metadata": bad_metadata,
        "effective_value": {"kind": "scalar", "value": "test"},
        "provenance": {"state": "unavailable"},
    });
    let snapshot4 = insert_snapshot(
        &pool,
        "bad-surviving-sources",
        2,
        payload4,
        Some(global_unavailable()),
        Some(false),
        Some(TARGET_KEY),
        Some("/nix/store/source"),
        Some("/nix/store/carrier.drv"),
        Some("1111111111111111111111111111111111111111111111111111111111111111"),
        Some(&["test", "sources"]),
        None,
    )
    .await;
    assert!(
        !certify(&pool, snapshot4).await,
        "malformed surviving_definition_sources must fail"
    );

    // Non-contiguous definition ordinal
    let bad_ordinal = local_payload(
        vec![
            definition(
                0,
                Some(0),
                "active_surviving",
                json!({"kind": "scalar", "value": "a"}),
            ),
            definition(
                2,
                Some(1),
                "active_surviving",
                json!({"kind": "scalar", "value": "b"}),
            ),
        ],
        false,
    );
    let snapshot5 = insert_snapshot(
        &pool,
        "non-contiguous-ordinal",
        2,
        bad_ordinal,
        Some(global_available(enrichment_available(1, PROVENANCE_DIGEST))),
        Some(true),
        Some(TARGET_KEY),
        Some("/nix/store/source"),
        Some("/nix/store/carrier.drv"),
        Some("2222222222222222222222222222222222222222222222222222222222222222"),
        Some(&["test", "ordinal"]),
        Some(false),
    )
    .await;
    assert!(
        !certify(&pool, snapshot5).await,
        "non-contiguous ordinal must fail"
    );

    // Duplicate survivor merge order
    let bad_merge = local_payload(
        vec![
            definition(
                0,
                Some(0),
                "active_surviving",
                json!({"kind": "scalar", "value": "a"}),
            ),
            definition(
                1,
                Some(0),
                "active_surviving",
                json!({"kind": "scalar", "value": "b"}),
            ),
        ],
        false,
    );
    let snapshot6 = insert_snapshot(
        &pool,
        "duplicate-merge-order",
        2,
        bad_merge,
        Some(global_available(enrichment_available(1, PROVENANCE_DIGEST))),
        Some(true),
        Some(TARGET_KEY),
        Some("/nix/store/source"),
        Some("/nix/store/carrier.drv"),
        Some("3333333333333333333333333333333333333333333333333333333333333333"),
        Some(&["test", "merge"]),
        Some(false),
    )
    .await;
    assert!(
        !certify(&pool, snapshot6).await,
        "duplicate merge order must fail"
    );
}
