use crystal_forge::compliance::digest::PolicyVersionCanonical;
use crystal_forge::models::deployment_policies::{
    CreateDeploymentPolicyRequest, UpdateDeploymentPolicyRequest,
};
use crystal_forge::queries::deployment_policies::{
    create_deployment_policy, get_deployment_policy_by_version, update_deployment_policy,
};
use sqlx::PgPool;
use uuid::Uuid;

fn composite_config() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "mode": "all",
        "rules": [
            {
                "id": "30000000-0000-0000-0000-000000000001",
                "kind": "nixos_option",
                "config": {
                    "path": "services.openssh.banner",
                    "operator": "==",
                    "value_type": "string",
                    "value": "Authorized use only\nSecond line"
                }
            },
            {
                "id": "30000000-0000-0000-0000-000000000002",
                "kind": "cve_block",
                "config": {"severity": "high", "max_allowed": 2}
            }
        ]
    })
}

#[sqlx::test]
async fn derived_composite_draft_preserves_legacy_ancestor_and_exact_version_pairing(pool: PgPool) {
    let created = create_deployment_policy(
        &pool,
        &CreateDeploymentPolicyRequest {
            name: format!("composite-regression-{}", Uuid::new_v4()),
            policy_type: "custom_check".into(),
            config: serde_json::json!({"mode": "all", "rules": []}),
            enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let ancestor_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(created.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let ancestor_digest: String =
        sqlx::query_scalar("SELECT semantic_digest FROM deployment_policy_versions WHERE id = $1")
            .bind(ancestor_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let mut publish_tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(created.id)
        .execute(&mut *publish_tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE deployment_policy_versions SET publication_state = 'accepted', trust_state = 'trusted' WHERE id = $1",
    )
    .bind(ancestor_id)
    .execute(&mut *publish_tx)
    .await
    .unwrap();
    sqlx::query("UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2")
        .bind(ancestor_id)
        .bind(created.id)
        .execute(&mut *publish_tx)
        .await
        .unwrap();
    publish_tx.commit().await.unwrap();

    let config = composite_config();
    update_deployment_policy(
        &pool,
        &created.id,
        &UpdateDeploymentPolicyRequest {
            policy_type: Some("composite".into()),
            config: Some(config.clone()),
            ..Default::default()
        },
        None,
    )
    .await
    .unwrap()
    .unwrap();

    let draft_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(created.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let ancestor = get_deployment_policy_by_version(&pool, &ancestor_id)
        .await
        .unwrap()
        .unwrap();
    let draft = get_deployment_policy_by_version(&pool, &draft_id)
        .await
        .unwrap()
        .unwrap();
    let (stored_digest, derived_from, execution_phase): (String, Option<Uuid>, String) = sqlx::query_as(
        "SELECT semantic_digest, derived_from_version_id, execution_phase FROM deployment_policy_versions WHERE id = $1",
    )
    .bind(draft_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(ancestor.policy_type, "custom_check");
    assert_eq!(
        ancestor.config,
        serde_json::json!({"mode": "all", "rules": []})
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT semantic_digest FROM deployment_policy_versions WHERE id = $1",
        )
        .bind(ancestor_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        ancestor_digest
    );
    assert_eq!(draft.policy_type, "composite");
    assert_eq!(draft.config, config);
    assert_eq!(derived_from, Some(ancestor_id));
    assert_eq!(execution_phase, "multi-phase");

    let expected = PolicyVersionCanonical {
        name: draft.name,
        description: draft.description,
        policy_type: draft.policy_type,
        implementation_state: "native".into(),
        execution_phase: "multi-phase".into(),
        config: draft.config,
        compliance_metadata: serde_json::json!({}),
        dependencies: serde_json::json!([]),
        opaque_xml_digest: None,
        enabled_by_default: Some(false),
    };
    assert_eq!(stored_digest, expected.compute_digest());
}

#[sqlx::test]
async fn query_layer_rejects_invalid_composite_when_handler_is_bypassed(pool: PgPool) {
    let error = create_deployment_policy(
        &pool,
        &CreateDeploymentPolicyRequest {
            name: format!("invalid-composite-{}", Uuid::new_v4()),
            policy_type: "composite".into(),
            config: serde_json::json!({"schema_version": 1, "mode": "all", "rules": []}),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("must not be empty"));
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deployment_policies WHERE policy_type = 'composite'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test]
async fn query_layer_persists_target_specific_nixos_option_semantics(pool: PgPool) {
    let config = serde_json::json!({
        "schema_version": 1,
        "mode": "all",
        "rules": [
            {
                "id": "30000000-0000-0000-0000-000000000003",
                "kind": "nixos_option",
                "config": {
                    "path": "networking.firewall.backend",
                    "operator": "==",
                    "value_type": "unknown",
                    "value": "target-specific-unknown"
                }
            },
            {
                "id": "30000000-0000-0000-0000-000000000004",
                "kind": "nixos_option",
                "config": {
                    "path": "networking.firewall.backend",
                    "operator": "==",
                    "value_type": "enum",
                    "value": "target-specific-enum"
                }
            }
        ]
    });
    let created = create_deployment_policy(
        &pool,
        &CreateDeploymentPolicyRequest {
            name: format!("target-specific-composite-{}", Uuid::new_v4()),
            policy_type: "composite".into(),
            config: config.clone(),
            enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(created.id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let persisted = get_deployment_policy_by_version(&pool, &version_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.policy_type, "composite");
    assert_eq!(persisted.config, config);
}
