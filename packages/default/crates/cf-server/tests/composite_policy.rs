use chrono::{TimeZone, Utc};
use crystal_forge::compliance::digest::PolicyVersionCanonical;
use crystal_forge::compliance::resolver::{
    EffectivePolicySet, ResolutionOutcome, resolve_system_effective_policies,
};
use crystal_forge::models::deployment_policies::{
    AssignedPolicy, CompositePolicyConfig, CompositeRuleOutcome, CreateDeploymentPolicyRequest,
    DeploymentPolicy, EnforcementOutcome, EnforcementPhase, PolicyCheckResult,
    UpdateDeploymentPolicyRequest, composite_rule_result_key, policy_results_json,
};
use crystal_forge::queries::cve_scans::{
    complete_cve_scan, create_cve_scan, mark_cve_scan_failed_by_id,
};
use crystal_forge::queries::deployment_policies::{
    create_deployment_policy, get_deployment_policy_by_version, update_deployment_policy,
};
use crystal_forge::queries::derivations::{
    SuccessfulEvalWrite, record_successful_eval_result, record_synthetic_eval_failure_in_tx,
};
use crystal_forge::queries::{
    commits::{insert_commit, mark_commit_evaluation_failed},
    flakes::insert_flake,
};
use crystal_forge::services::composite_enforcement::{
    authorize_and_claim_desired_target, authorize_and_set_system_target, authorize_deployment_at,
    initialize_eval_passed_attempt, persist_eval_passed_for_system_in_tx,
    persist_eval_passed_terminal_checks_in_tx, persist_evaluation_assessments_in_tx,
};
use sqlx::PgPool;
use std::collections::BTreeMap;
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

fn phase_config() -> CompositePolicyConfig {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "mode": "all",
        "rules": [
            {
                "id": "40000000-0000-0000-0000-000000000001",
                "kind": "nixos_option",
                "config": {
                    "path": "networking.firewall.enable",
                    "operator": "==",
                    "value_type": "boolean",
                    "value": true
                }
            },
            {
                "id": "40000000-0000-0000-0000-000000000002",
                "kind": "cve_block",
                "config": {"severity": "critical", "max_allowed": 0}
            },
            {
                "id": "40000000-0000-0000-0000-000000000003",
                "kind": "time_window",
                "config": {"days": ["mon", "tue", "wed", "thu", "fri", "sat", "sun"], "from": "00:00", "to": "23:59", "tz": "UTC"}
            }
        ]
    }))
    .unwrap()
}

fn single_rule_config(kind: &str, config: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "mode": "all",
        "rules": [{
            "id": Uuid::new_v4(),
            "kind": kind,
            "config": config
        }]
    })
}

fn ac3_crud_cases() -> Vec<(&'static str, serde_json::Value, serde_json::Value)> {
    vec![
        (
            "nixos_option",
            serde_json::json!({"path": "networking.firewall.enable", "operator": "==", "value_type": "boolean", "value": true}),
            serde_json::json!({"path": "networking.firewall.enable", "operator": "!=", "value_type": "boolean", "value": false}),
        ),
        (
            "packages_installed",
            serde_json::json!({"packages": ["openssh"]}),
            serde_json::json!({"packages": ["openssh", "auditd"]}),
        ),
        (
            "packages_absent",
            serde_json::json!({"packages": ["telnet"]}),
            serde_json::json!({"packages": ["telnet", "rsh"]}),
        ),
        (
            "custom_eval",
            serde_json::json!({"expression": "config.networking.firewall.enable", "message": "firewall"}),
            serde_json::json!({"expression": "config.services.openssh.enable", "message": "ssh"}),
        ),
        (
            "cve_block",
            serde_json::json!({"severity": "critical", "max_allowed": 0}),
            serde_json::json!({"severity": "high", "max_allowed": 1}),
        ),
        ("eval_passed", serde_json::json!({}), serde_json::json!({})),
        ("pin_required", serde_json::json!({}), serde_json::json!({})),
        (
            "time_window",
            serde_json::json!({"days": ["mon"], "from": "09:00", "to": "17:00", "tz": "UTC"}),
            serde_json::json!({"days": ["tue"], "from": "10:00", "to": "18:00", "tz": "UTC"}),
        ),
    ]
}

#[sqlx::test]
async fn ac3_create_validate_persist_reload_and_edit_matrix_covers_every_exposed_kind(
    pool: PgPool,
) {
    for (kind, initial_rule, edited_rule) in ac3_crud_cases() {
        let initial = single_rule_config(kind, initial_rule);
        let created = create_deployment_policy(
            &pool,
            &CreateDeploymentPolicyRequest {
                name: format!("ac3-{kind}-{}", Uuid::new_v4()),
                policy_type: "composite".into(),
                config: initial.clone(),
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap_or_else(|error| panic!("AC3 create/validate/persist [{kind}]: {error}"));
        let initial_version: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
        )
        .bind(created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let reloaded = get_deployment_policy_by_version(&pool, &initial_version)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reloaded.config, initial, "AC3 reload [{kind}]");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT execution_phase FROM deployment_policy_versions WHERE id = $1",
            )
            .bind(initial_version)
            .fetch_one(&pool)
            .await
            .unwrap(),
            "multi-phase",
            "AC3 phase metadata [{kind}]"
        );

        let mut edited = single_rule_config(kind, edited_rule);
        edited["rules"][0]["id"] = initial["rules"][0]["id"].clone();
        let edited_name = format!("ac3-{kind}-edited-{}", Uuid::new_v4());
        update_deployment_policy(
            &pool,
            &created.id,
            &UpdateDeploymentPolicyRequest {
                name: Some(edited_name.clone()),
                config: Some(edited.clone()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap_or_else(|error| panic!("AC3 edit [{kind}]: {error}"))
        .expect("policy must still exist");
        let edited_version: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
        )
        .bind(created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let edited_reload = get_deployment_policy_by_version(&pool, &edited_version)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(edited_reload.config, edited, "AC3 edit/reload [{kind}]");
        assert_eq!(edited_reload.name, edited_name, "AC3 edit/name [{kind}]");
    }
}

fn policy_results(
    version_id: Uuid,
    config: &CompositePolicyConfig,
    value: EnforcementOutcome,
) -> serde_json::Value {
    let rule = &config.rules[0];
    let outcome = CompositeRuleOutcome {
        rule_id: rule.id,
        kind: rule.rule.kind().to_string(),
        phase: EnforcementPhase::Evaluation,
        outcome: value,
        blocking: value != EnforcementOutcome::Pass,
        detail: format!("evaluation is {value:?}"),
        evidence: serde_json::json!({"test": true}),
    };
    serde_json::json!({
        "assigned": {
            version_id.to_string(): {
                "config_digest": crystal_forge::models::deployment_policies::composite_config_digest(config),
                "rule_outcomes": [outcome]
            }
        }
    })
}

fn policy_results_for_versions(
    version_ids: &[Uuid],
    config: &CompositePolicyConfig,
    values: &[EnforcementOutcome],
) -> serde_json::Value {
    let mut assigned = serde_json::Map::new();
    for (version_id, value) in version_ids.iter().zip(values) {
        assigned.insert(
            version_id.to_string(),
            policy_results(*version_id, config, *value)["assigned"][version_id.to_string()].clone(),
        );
    }
    serde_json::json!({"assigned": assigned})
}

struct AssessmentContext {
    system_id: Uuid,
    version_id: Uuid,
    derivation_id: i32,
    store_path: String,
    resolved: EffectivePolicySet,
}

async fn assessment_context_with_config(
    pool: &PgPool,
    config: CompositePolicyConfig,
) -> AssessmentContext {
    assessment_context_with_config_and_build(pool, config, true).await
}

async fn assessment_context_with_config_and_build(
    pool: &PgPool,
    config: CompositePolicyConfig,
    built: bool,
) -> AssessmentContext {
    let system_id = Uuid::new_v4();
    let hostname = format!("composite-assessment-{system_id}");
    let repo_url = format!("https://example.invalid/{system_id}.git");
    let commit_hash = system_id.simple().to_string();
    let flake = insert_flake(
        pool,
        &format!("composite-assessment-flake-{system_id}"),
        &repo_url,
        "main",
        "all_configs",
    )
    .await
    .unwrap();
    insert_commit(pool, &commit_hash, &repo_url, chrono::Utc::now())
        .await
        .unwrap();
    let commit_id: i32 =
        sqlx::query_scalar("SELECT id FROM commits WHERE flake_id = $1 AND git_commit_hash = $2")
            .bind(flake.id)
            .bind(&commit_hash)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO systems (id, hostname, is_active, public_key, derivation, reachability, flake_id, system_configuration_name) VALUES ($1, $2, true, $3, $3, 'direct', $4, $2)",
    )
    .bind(system_id)
    .bind(&hostname)
    .bind(format!("ssh-key-{system_id}"))
    .bind(flake.id)
    .execute(pool)
    .await
    .unwrap();

    let config = serde_json::to_value(config).unwrap();
    let policy = create_deployment_policy(
        pool,
        &CreateDeploymentPolicyRequest {
            name: format!("composite-assessment-policy-{}", Uuid::new_v4()),
            policy_type: "composite".to_string(),
            config,
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy.id)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET trust_state = 'trusted' WHERE id = $1")
        .bind(version_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
        .bind(system_id)
        .bind(policy.id)
        .execute(pool)
        .await
        .unwrap();

    let store_path = format!("/nix/store/{system_id}-target");
    let write = record_successful_eval_result(
        pool,
        Some(commit_id),
        &hostname,
        "nixos",
        None,
        &format!("/nix/store/{system_id}-target.drv"),
        Some(&store_path),
        Some(true),
        true,
        &serde_json::json!({}),
    )
    .await
    .unwrap();
    let derivation_id = match write {
        SuccessfulEvalWrite::Inserted { derivation_id }
        | SuccessfulEvalWrite::UpdatedEvaluationState { derivation_id }
        | SuccessfulEvalWrite::PreservedBuildState { derivation_id, .. } => derivation_id,
    };
    if built {
        sqlx::query("UPDATE derivations SET store_path = $1, status_id = 10 WHERE id = $2")
            .bind(&store_path)
            .bind(derivation_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO cache_push_jobs (derivation_id, status, store_path, completed_at) VALUES ($1, 'completed', $2, NOW())",
        )
        .bind(derivation_id)
        .bind(&store_path)
        .execute(pool)
        .await
        .unwrap();
    }
    let resolved = match resolve_system_effective_policies(pool, system_id)
        .await
        .unwrap()
    {
        ResolutionOutcome::Resolved(resolved) => resolved,
        ResolutionOutcome::Conflict(conflicts) => panic!("unexpected conflict: {conflicts:?}"),
    };
    AssessmentContext {
        system_id,
        version_id,
        derivation_id,
        store_path,
        resolved,
    }
}

#[sqlx::test]
async fn expected_only_target_persists_and_failed_reevaluation_invalidates_prior_pass(
    pool: PgPool,
) {
    let context = assessment_context_with_config_and_build(&pool, phase_config(), false).await;
    sqlx::query(
        "UPDATE derivations SET store_path = '/nix/store/older-built-target' WHERE id = $1",
    )
    .bind(context.derivation_id)
    .execute(&pool)
    .await
    .unwrap();
    persist_evaluation(&pool, &context, EnforcementOutcome::Pass).await;

    let persisted: (String, String) = sqlx::query_as(
        "SELECT target_store_path, overall_outcome FROM composite_policy_assessments",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        persisted,
        (context.store_path.clone(), "not_checked".into())
    );

    let stale_target = sqlx::query(
        "UPDATE derivations SET expected_store_path = '/nix/store/stale-new-target' WHERE id = $1",
    )
    .bind(context.derivation_id)
    .execute(&pool)
    .await;
    assert!(
        stale_target.is_err(),
        "the assessment FK must remain tied to the fresh expected target"
    );

    let commit_id: i32 = sqlx::query_scalar("SELECT commit_id FROM derivations WHERE id = $1")
        .bind(context.derivation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let hostname: String = sqlx::query_scalar("SELECT hostname FROM systems WHERE id = $1")
        .bind(context.system_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    record_synthetic_eval_failure_in_tx(
        &mut tx,
        Some(commit_id),
        &hostname,
        "nixos",
        None,
        "fresh reevaluation failed",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let assessment_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM composite_policy_assessments")
            .fetch_one(&pool)
            .await
            .unwrap();
    let derivation: (Option<String>, bool) = sqlx::query_as(
        "SELECT expected_store_path, policy_requirements_met FROM derivations WHERE id = $1",
    )
    .bind(context.derivation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(assessment_count, 0);
    assert_eq!(derivation, (None, false));
}

async fn assessment_context(pool: &PgPool) -> AssessmentContext {
    assessment_context_with_config(pool, phase_config()).await
}

fn parsed_evaluation_results(
    version_id: Uuid,
    config: &CompositePolicyConfig,
    value: EnforcementOutcome,
) -> serde_json::Value {
    let assigned = AssignedPolicy {
        policy_id: version_id,
        policy_name: "mixed lifecycle".to_string(),
        policy: DeploymentPolicy::Composite {
            config: config.clone(),
        },
    };
    let rule = &config.rules[0];
    let metadata_value = match value {
        EnforcementOutcome::Pass => serde_json::json!({"success": true, "value": true}),
        EnforcementOutcome::Fail => serde_json::json!({"success": true, "value": false}),
        EnforcementOutcome::Error | EnforcementOutcome::NotChecked => {
            serde_json::json!({"success": false, "value": false})
        }
    };
    let check = PolicyCheckResult::from_assigned(
        "mixed-host".to_string(),
        &serde_json::json!({
            "cfAgentEnabled": true,
            composite_rule_result_key(&version_id, &rule.id): metadata_value,
        }),
        std::slice::from_ref(&assigned),
    )
    .expect("real evaluation parser must accept generated metadata");
    policy_results_json(&check, std::slice::from_ref(&assigned))
}

async fn persist_evaluation(pool: &PgPool, context: &AssessmentContext, value: EnforcementOutcome) {
    let config = phase_config();
    let mut tx = pool.begin().await.unwrap();
    persist_evaluation_assessments_in_tx(
        &mut tx,
        context.system_id,
        context.derivation_id,
        &context.store_path,
        &parsed_evaluation_results(context.version_id, &config, value),
        &context.resolved,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

fn evaluation_matrix_config() -> CompositePolicyConfig {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "mode": "all",
        "rules": [
            {"id": "41000000-0000-0000-0000-000000000001", "kind": "nixos_option", "config": {"path": "networking.firewall.enable", "operator": "==", "value_type": "boolean", "value": true}},
            {"id": "41000000-0000-0000-0000-000000000002", "kind": "packages_installed", "config": {"packages": ["openssh"]}},
            {"id": "41000000-0000-0000-0000-000000000003", "kind": "packages_absent", "config": {"packages": ["telnet"]}},
            {"id": "41000000-0000-0000-0000-000000000004", "kind": "custom_eval", "config": {"expression": "config.networking.firewall.enable", "message": "firewall"}},
            {"id": "41000000-0000-0000-0000-000000000005", "kind": "eval_passed", "config": {}},
            {"id": "41000000-0000-0000-0000-000000000006", "kind": "pin_required", "config": {}}
        ]
    }))
    .unwrap()
}

#[sqlx::test]
async fn ac3_evaluation_matrix_uses_parser_and_normalized_persistence_for_all_six_kinds(
    pool: PgPool,
) {
    let config = evaluation_matrix_config();
    let context = assessment_context_with_config(&pool, config.clone()).await;
    let assigned = AssignedPolicy {
        policy_id: context.version_id,
        policy_name: "AC3 evaluation matrix".to_string(),
        policy: DeploymentPolicy::Composite {
            config: config.clone(),
        },
    };
    let revision = "0123456789abcdef0123456789abcdef01234567";
    let mut metadata = serde_json::json!({
        "cfAgentEnabled": true,
        "requestedSourceRevision": revision,
        "resolvedSourceRevision": revision,
    });
    for rule in &config.rules[..4] {
        metadata[composite_rule_result_key(&context.version_id, &rule.id)] =
            serde_json::json!({"success": true, "value": true});
    }
    let parsed = PolicyCheckResult::from_assigned(
        "matrix-host".to_string(),
        &metadata,
        std::slice::from_ref(&assigned),
    )
    .expect("AC3 evaluation parser");
    let results = policy_results_json(&parsed, std::slice::from_ref(&assigned));
    let mut tx = pool.begin().await.unwrap();
    persist_evaluation_assessments_in_tx(
        &mut tx,
        context.system_id,
        context.derivation_id,
        &context.store_path,
        &results,
        &context.resolved,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let rows: Vec<(String, String, String, serde_json::Value)> = sqlx::query_as(
        r#"
        SELECT kind, phase, outcome, evidence
        FROM composite_policy_rule_results
        ORDER BY ordinal
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let expected_kinds = [
        "nixos_option",
        "packages_installed",
        "packages_absent",
        "custom_eval",
        "eval_passed",
        "pin_required",
    ];
    assert_eq!(rows.len(), expected_kinds.len());
    for ((kind, phase, outcome, evidence), expected_kind) in rows.iter().zip(expected_kinds) {
        assert_eq!(kind, expected_kind, "AC3 normalized kind [{expected_kind}]");
        assert_eq!(phase, "evaluation", "AC3 correct phase [{expected_kind}]");
        assert_eq!(outcome, "pass", "AC3 persisted pass [{expected_kind}]");
        assert_ne!(
            evidence,
            &serde_json::json!({}),
            "AC3 evidence [{expected_kind}]"
        );
    }
    assert_eq!(
        rows.last().unwrap().3["resolved_revision"],
        revision,
        "AC3 pin_required immutable revision evidence"
    );
}

async fn persisted_phase_outcomes(pool: &PgPool, system_id: Uuid) -> Vec<(String, String)> {
    sqlx::query_as(
        r#"
        SELECT result.phase, result.outcome
        FROM composite_policy_rule_results result
        JOIN composite_policy_assessments assessment ON assessment.id = result.assessment_id
        WHERE assessment.system_id = $1
        ORDER BY result.ordinal
        "#,
    )
    .bind(system_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn completed_scan(pool: &PgPool, context: &AssessmentContext, critical: i32) -> Uuid {
    let scan_id = create_cve_scan(pool, context.derivation_id, "ac3-matrix", None)
        .await
        .unwrap()
        .id();
    complete_cve_scan(pool, scan_id, 1, critical, critical, 0, 0, 0, Some(1), None)
        .await
        .unwrap();
    scan_id
}

#[sqlx::test]
async fn ac3_mixed_lifecycle_failure_permutations_preserve_earlier_outcomes_and_block_final_pass(
    pool: PgPool,
) {
    let inside = Utc.with_ymd_and_hms(2026, 8, 24, 12, 0, 0).unwrap();
    let outside = Utc.with_ymd_and_hms(2026, 8, 24, 23, 59, 59).unwrap();

    let nix_fail = assessment_context(&pool).await;
    persist_evaluation(&pool, &nix_fail, EnforcementOutcome::Fail).await;
    completed_scan(&pool, &nix_fail, 0).await;
    let authorization = authorize_deployment_at(
        &pool,
        nix_fail.system_id,
        nix_fail.derivation_id,
        &nix_fail.store_path,
        inside,
    )
    .await
    .unwrap();
    assert_eq!(
        authorization.outcome,
        EnforcementOutcome::Fail,
        "AC3 Nix fail"
    );
    assert_eq!(
        persisted_phase_outcomes(&pool, nix_fail.system_id).await,
        [
            ("evaluation".into(), "fail".into()),
            ("scan".into(), "pass".into()),
            ("deployment".into(), "pass".into())
        ],
        "AC3 Nix fail preserves later constituent evidence"
    );

    let cve_fail = assessment_context(&pool).await;
    persist_evaluation(&pool, &cve_fail, EnforcementOutcome::Pass).await;
    completed_scan(&pool, &cve_fail, 1).await;
    let authorization = authorize_deployment_at(
        &pool,
        cve_fail.system_id,
        cve_fail.derivation_id,
        &cve_fail.store_path,
        inside,
    )
    .await
    .unwrap();
    assert_eq!(
        authorization.outcome,
        EnforcementOutcome::Fail,
        "AC3 CVE fail"
    );
    assert_eq!(
        persisted_phase_outcomes(&pool, cve_fail.system_id).await[0].1,
        "pass"
    );

    let cve_error = assessment_context(&pool).await;
    persist_evaluation(&pool, &cve_error, EnforcementOutcome::Pass).await;
    let failed_scan = create_cve_scan(&pool, cve_error.derivation_id, "ac3-matrix", None)
        .await
        .unwrap()
        .id();
    mark_cve_scan_failed_by_id(
        &pool,
        failed_scan,
        cve_error.derivation_id,
        "scanner failed",
    )
    .await
    .unwrap();
    let authorization = authorize_deployment_at(
        &pool,
        cve_error.system_id,
        cve_error.derivation_id,
        &cve_error.store_path,
        inside,
    )
    .await
    .unwrap();
    assert_eq!(
        authorization.outcome,
        EnforcementOutcome::Error,
        "AC3 CVE error"
    );
    assert_eq!(
        persisted_phase_outcomes(&pool, cve_error.system_id).await[0].1,
        "pass"
    );

    let cve_not_checked = assessment_context(&pool).await;
    persist_evaluation(&pool, &cve_not_checked, EnforcementOutcome::Pass).await;
    let authorization = authorize_deployment_at(
        &pool,
        cve_not_checked.system_id,
        cve_not_checked.derivation_id,
        &cve_not_checked.store_path,
        inside,
    )
    .await
    .unwrap();
    assert_eq!(
        authorization.outcome,
        EnforcementOutcome::NotChecked,
        "AC3 CVE notchecked"
    );
    assert_eq!(
        persisted_phase_outcomes(&pool, cve_not_checked.system_id).await[0].1,
        "pass"
    );

    let time_fail = assessment_context(&pool).await;
    persist_evaluation(&pool, &time_fail, EnforcementOutcome::Pass).await;
    completed_scan(&pool, &time_fail, 0).await;
    let authorization = authorize_deployment_at(
        &pool,
        time_fail.system_id,
        time_fail.derivation_id,
        &time_fail.store_path,
        outside,
    )
    .await
    .unwrap();
    assert_eq!(
        authorization.outcome,
        EnforcementOutcome::Fail,
        "AC3 time fail"
    );
    assert_eq!(
        persisted_phase_outcomes(&pool, time_fail.system_id).await,
        [
            ("evaluation".into(), "pass".into()),
            ("scan".into(), "pass".into()),
            ("deployment".into(), "fail".into())
        ],
        "AC3 time fail preserves evaluation and scan outcomes"
    );
}

async fn add_phase_policy(pool: &PgPool, system_id: Uuid) -> Uuid {
    let policy = create_deployment_policy(
        pool,
        &CreateDeploymentPolicyRequest {
            name: format!("composite-assessment-policy-{}", Uuid::new_v4()),
            policy_type: "composite".to_string(),
            config: serde_json::to_value(phase_config()).unwrap(),
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy.id)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET trust_state = 'trusted' WHERE id = $1")
        .bind(version_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
        .bind(system_id)
        .bind(policy.id)
        .execute(pool)
        .await
        .unwrap();
    version_id
}

#[sqlx::test]
async fn phase_lifecycle_persists_ordered_placeholders_and_authorizes_only_after_all_pass(
    pool: PgPool,
) {
    let context = assessment_context(&pool).await;
    persist_evaluation(&pool, &context, EnforcementOutcome::Pass).await;
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT result.phase, result.outcome
        FROM composite_policy_rule_results result
        JOIN composite_policy_assessments assessment ON assessment.id = result.assessment_id
        WHERE assessment.system_id = $1 AND assessment.policy_version_id = $2
        ORDER BY result.ordinal
        "#,
    )
    .bind(context.system_id)
    .bind(context.version_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            ("evaluation".into(), "pass".into()),
            ("scan".into(), "not_checked".into()),
            ("deployment".into(), "not_checked".into())
        ]
    );

    let before_scan = authorize_deployment_at(
        &pool,
        context.system_id,
        context.derivation_id,
        &context.store_path,
        Utc.with_ymd_and_hms(2026, 8, 24, 12, 0, 0).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(before_scan.outcome, EnforcementOutcome::NotChecked);

    let scan_id = create_cve_scan(&pool, context.derivation_id, "test", None)
        .await
        .unwrap()
        .id();
    complete_cve_scan(&pool, scan_id, 1, 0, 0, 0, 0, 0, Some(1), None)
        .await
        .unwrap();
    let authorized = authorize_deployment_at(
        &pool,
        context.system_id,
        context.derivation_id,
        &context.store_path,
        Utc.with_ymd_and_hms(2026, 8, 24, 12, 0, 0).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(authorized.outcome, EnforcementOutcome::Pass);
}

#[sqlx::test]
async fn newer_pending_and_failed_scan_cannot_be_reversed_by_older_completion(pool: PgPool) {
    let context = assessment_context(&pool).await;
    persist_evaluation(&pool, &context, EnforcementOutcome::Pass).await;
    let first = create_cve_scan(&pool, context.derivation_id, "test", None)
        .await
        .unwrap()
        .id();
    complete_cve_scan(&pool, first, 1, 0, 0, 0, 0, 0, Some(1), None)
        .await
        .unwrap();
    let second = create_cve_scan(&pool, context.derivation_id, "test", None)
        .await
        .unwrap()
        .id();

    complete_cve_scan(&pool, first, 1, 1, 1, 0, 0, 0, Some(1), None)
        .await
        .unwrap();
    let current: (Option<Uuid>, String) = sqlx::query_as(
        "SELECT source_scan_id, outcome FROM composite_policy_rule_results WHERE phase = 'scan'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(current, (Some(second), "not_checked".to_string()));

    mark_cve_scan_failed_by_id(&pool, second, context.derivation_id, "scanner failed")
        .await
        .unwrap();
    complete_cve_scan(&pool, second, 1, 1, 1, 0, 0, 0, Some(1), None)
        .await
        .unwrap();
    let terminal: (String, Option<Uuid>, String) = sqlx::query_as(
        r#"
        SELECT scan.status, result.source_scan_id, result.outcome
        FROM cve_scans scan
        JOIN composite_policy_rule_results result ON result.source_scan_id = scan.id
        WHERE scan.id = $1
        "#,
    )
    .bind(second)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(terminal, ("failed".into(), Some(second), "error".into()));
    let blocked = authorize_deployment_at(
        &pool,
        context.system_id,
        context.derivation_id,
        &context.store_path,
        Utc.with_ymd_and_hms(2026, 8, 24, 12, 0, 0).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(blocked.outcome, EnforcementOutcome::Error);
}

#[sqlx::test]
async fn final_authorization_is_exact_and_target_update_is_guarded_by_all_phase_outcomes(
    pool: PgPool,
) {
    let context = assessment_context(&pool).await;
    persist_evaluation(&pool, &context, EnforcementOutcome::Fail).await;
    let scan_id = create_cve_scan(&pool, context.derivation_id, "test", None)
        .await
        .unwrap()
        .id();
    complete_cve_scan(&pool, scan_id, 1, 0, 0, 0, 0, 0, Some(1), None)
        .await
        .unwrap();
    let blocked = authorize_and_set_system_target(
        &pool,
        context.system_id,
        &context.store_path,
        "manual_deploy",
    )
    .await
    .unwrap();
    assert_eq!(blocked.outcome, EnforcementOutcome::Fail);
    let desired: Option<String> =
        sqlx::query_scalar("SELECT desired_target FROM systems WHERE id = $1")
            .bind(context.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(desired, None);

    persist_evaluation(&pool, &context, EnforcementOutcome::Pass).await;
    sqlx::query("UPDATE derivations SET status_id = 14 WHERE id = $1")
        .bind(context.derivation_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE cache_push_jobs SET store_path = '/nix/store/different-cached-target' WHERE derivation_id = $1",
    )
    .bind(context.derivation_id)
    .execute(&pool)
    .await
    .unwrap();
    let mismatched_cache = authorize_and_set_system_target(
        &pool,
        context.system_id,
        &context.store_path,
        "manual_deploy",
    )
    .await
    .unwrap_err();
    assert!(
        mismatched_cache
            .to_string()
            .contains("could not resolve exact system target")
    );
    sqlx::query("UPDATE cache_push_jobs SET store_path = $1 WHERE derivation_id = $2")
        .bind(&context.store_path)
        .bind(context.derivation_id)
        .execute(&pool)
        .await
        .unwrap();
    let allowed = authorize_and_set_system_target(
        &pool,
        context.system_id,
        &context.store_path,
        "manual_deploy",
    )
    .await
    .unwrap();
    assert_eq!(allowed.outcome, EnforcementOutcome::Pass);
    let desired: Option<String> =
        sqlx::query_scalar("SELECT desired_target FROM systems WHERE id = $1")
            .bind(context.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(desired.as_deref(), Some(context.store_path.as_str()));

    let wrong_derivation = authorize_deployment_at(
        &pool,
        context.system_id,
        context.derivation_id + 1,
        &context.store_path,
        Utc.with_ymd_and_hms(2026, 8, 24, 12, 0, 0).unwrap(),
    )
    .await
    .unwrap_err();
    assert!(wrong_derivation.to_string().contains("derivation changed"));

    sqlx::query("DELETE FROM cache_push_jobs WHERE derivation_id = $1")
        .bind(context.derivation_id)
        .execute(&pool)
        .await
        .unwrap();
    let uncached = authorize_deployment_at(
        &pool,
        context.system_id,
        context.derivation_id,
        &context.store_path,
        Utc.with_ymd_and_hms(2026, 8, 24, 12, 0, 0).unwrap(),
    )
    .await
    .unwrap_err();
    assert!(
        uncached
            .to_string()
            .contains("could not resolve exact system target")
    );
    sqlx::query(
        "INSERT INTO cache_push_jobs (derivation_id, status, store_path, completed_at) VALUES ($1, 'completed', $2, NOW())",
    )
    .bind(context.derivation_id)
    .bind(&context.store_path)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("UPDATE composite_policy_assessments SET effective_set_digest = 'stale'")
        .execute(&pool)
        .await
        .unwrap();
    let stale = authorize_deployment_at(
        &pool,
        context.system_id,
        context.derivation_id,
        &context.store_path,
        Utc.with_ymd_and_hms(2026, 8, 24, 12, 0, 0).unwrap(),
    )
    .await
    .unwrap_err();
    assert!(stale.to_string().contains("incomplete or stale"));
}

#[sqlx::test]
async fn historical_store_path_without_derivation_is_preserved_without_composite_policy(
    pool: PgPool,
) {
    let system_id = Uuid::new_v4();
    let hostname = format!("legacy-rollback-{system_id}");
    let store_path = format!("/nix/store/{system_id}-historical");
    sqlx::query(
        "INSERT INTO systems (id, hostname, is_active, public_key, derivation, reachability) VALUES ($1, $2, true, $3, $3, 'direct')",
    )
    .bind(system_id)
    .bind(&hostname)
    .bind(format!("ssh-key-{system_id}"))
    .execute(&pool)
    .await
    .unwrap();

    let authorization = authorize_and_set_system_target(
        &pool,
        system_id,
        &store_path,
        "manual_rollback_generation",
    )
    .await
    .unwrap();
    assert_eq!(authorization.outcome, EnforcementOutcome::Pass);
    let desired: Option<String> =
        sqlx::query_scalar("SELECT desired_target FROM systems WHERE id = $1")
            .bind(system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(desired.as_deref(), Some(store_path.as_str()));

    let delivery = authorize_and_claim_desired_target(&pool, system_id, &store_path)
        .await
        .unwrap();
    assert_eq!(delivery.target.as_deref(), Some(store_path.as_str()));
}

#[sqlx::test]
async fn known_uncached_target_is_blocked_without_composite_policy(pool: PgPool) {
    let system_id = Uuid::new_v4();
    let hostname = format!("uncached-target-{system_id}");
    let repo_url = format!("https://example.invalid/{system_id}.git");
    let commit_hash = system_id.simple().to_string();
    let flake = insert_flake(
        &pool,
        &format!("uncached-target-flake-{system_id}"),
        &repo_url,
        "main",
        "all_configs",
    )
    .await
    .unwrap();
    insert_commit(&pool, &commit_hash, &repo_url, Utc::now())
        .await
        .unwrap();
    let commit_id: i32 =
        sqlx::query_scalar("SELECT id FROM commits WHERE flake_id = $1 AND git_commit_hash = $2")
            .bind(flake.id)
            .bind(&commit_hash)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO systems (id, hostname, is_active, public_key, derivation, reachability, flake_id, system_configuration_name) VALUES ($1, $2, true, $3, $3, 'direct', $4, $2)",
    )
    .bind(system_id)
    .bind(&hostname)
    .bind(format!("ssh-key-{system_id}"))
    .bind(flake.id)
    .execute(&pool)
    .await
    .unwrap();

    let store_path = format!("/nix/store/{system_id}-uncached");
    let write = record_successful_eval_result(
        &pool,
        Some(commit_id),
        &hostname,
        "nixos",
        None,
        &format!("{store_path}.drv"),
        Some(&store_path),
        Some(true),
        true,
        &serde_json::json!({}),
    )
    .await
    .unwrap();
    let derivation_id = match write {
        SuccessfulEvalWrite::Inserted { derivation_id }
        | SuccessfulEvalWrite::UpdatedEvaluationState { derivation_id }
        | SuccessfulEvalWrite::PreservedBuildState { derivation_id, .. } => derivation_id,
    };
    sqlx::query("UPDATE derivations SET store_path = $1, status_id = 10 WHERE id = $2")
        .bind(&store_path)
        .bind(derivation_id)
        .execute(&pool)
        .await
        .unwrap();

    let blocked = authorize_and_set_system_target(
        &pool,
        system_id,
        &store_path,
        "manual_rollback_generation",
    )
    .await
    .unwrap();
    assert_eq!(blocked.outcome, EnforcementOutcome::Fail);
    let desired: Option<String> =
        sqlx::query_scalar("SELECT desired_target FROM systems WHERE id = $1")
            .bind(system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(desired, None);

    sqlx::query("UPDATE systems SET desired_target = $1 WHERE id = $2")
        .bind(&store_path)
        .bind(system_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO pending_system_deployments (system_id, target_store_path, source) VALUES ($1, $2, 'test_previously_authorized')",
    )
    .bind(system_id)
    .bind(&store_path)
    .execute(&pool)
    .await
    .unwrap();
    let delivery = authorize_and_claim_desired_target(&pool, system_id, &store_path)
        .await
        .unwrap();
    assert_eq!(delivery.authorization.outcome, EnforcementOutcome::Fail);
    assert_eq!(delivery.target, None);
}

#[sqlx::test]
async fn heartbeat_claim_requires_unchanged_desired_target_and_live_pending_delivery(pool: PgPool) {
    let context = assessment_context(&pool).await;
    persist_evaluation(&pool, &context, EnforcementOutcome::Pass).await;
    let scan_id = create_cve_scan(&pool, context.derivation_id, "test", None)
        .await
        .unwrap()
        .id();
    complete_cve_scan(&pool, scan_id, 1, 0, 0, 0, 0, 0, Some(1), None)
        .await
        .unwrap();
    authorize_and_set_system_target(
        &pool,
        context.system_id,
        &context.store_path,
        "manual_deploy",
    )
    .await
    .unwrap();

    sqlx::query("UPDATE systems SET desired_target = NULL WHERE id = $1")
        .bind(context.system_id)
        .execute(&pool)
        .await
        .unwrap();
    let changed = authorize_and_claim_desired_target(&pool, context.system_id, &context.store_path)
        .await
        .unwrap();
    assert_eq!(changed.target, None);

    sqlx::query("UPDATE systems SET desired_target = $1 WHERE id = $2")
        .bind(&context.store_path)
        .bind(context.system_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM pending_system_deployments WHERE system_id = $1")
        .bind(context.system_id)
        .execute(&pool)
        .await
        .unwrap();
    let missing = authorize_and_claim_desired_target(&pool, context.system_id, &context.store_path)
        .await
        .unwrap();
    assert_eq!(missing.target, None);
    assert_eq!(missing.authorization.outcome, EnforcementOutcome::Pass);
}

#[sqlx::test]
async fn upgrade_target_gets_pending_delivery_only_after_exact_authorization(pool: PgPool) {
    let context = assessment_context(&pool).await;
    persist_evaluation(&pool, &context, EnforcementOutcome::Pass).await;
    let scan_id = create_cve_scan(&pool, context.derivation_id, "test", None)
        .await
        .unwrap()
        .id();
    complete_cve_scan(&pool, scan_id, 1, 0, 0, 0, 0, 0, Some(1), None)
        .await
        .unwrap();
    sqlx::query("UPDATE systems SET desired_target = $1 WHERE id = $2")
        .bind(&context.store_path)
        .bind(context.system_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO composite_legacy_desired_targets (system_id, target_store_path) VALUES ($1, $2)",
    )
    .bind(context.system_id)
    .bind(&context.store_path)
    .execute(&pool)
    .await
    .unwrap();

    let before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pending_system_deployments WHERE system_id = $1")
            .bind(context.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, 0);

    let claimed = authorize_and_claim_desired_target(&pool, context.system_id, &context.store_path)
        .await
        .unwrap();
    assert_eq!(claimed.target.as_deref(), Some(context.store_path.as_str()));
    let pending: (String, bool, bool) = sqlx::query_as(
        r#"
        SELECT source, delivered_at IS NOT NULL,
               metadata ->> 'upgrade_authorized' = 'true'
        FROM pending_system_deployments
        WHERE system_id = $1 AND target_store_path = $2
        "#,
    )
    .bind(context.system_id)
    .bind(&context.store_path)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        pending,
        ("legacy_authorized_desired_target".into(), true, true)
    );
    let marker_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM composite_legacy_desired_targets WHERE system_id = $1",
    )
    .bind(context.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(marker_count, 0);
}

#[sqlx::test]
async fn eval_passed_attempt_evidence_survives_missing_target_and_new_attempt_supersedes_it(
    pool: PgPool,
) {
    let config: CompositePolicyConfig =
        serde_json::from_value(single_rule_config("eval_passed", serde_json::json!({}))).unwrap();
    let context = assessment_context_with_config(&pool, config.clone()).await;
    let (commit_id, hostname): (i32, String) = sqlx::query_as(
        r#"
        SELECT derivation.commit_id, system.hostname
        FROM derivations derivation
        JOIN systems system ON system.id = $2
        WHERE derivation.id = $1
        "#,
    )
    .bind(context.derivation_id)
    .bind(context.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE evaluation_attempts SET status = 'in_progress', started_at = NOW() WHERE commit_id = $1 AND attempt_number = 1",
    )
    .bind(commit_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE commits SET evaluation_status = 'in_progress', evaluation_attempt_count = 1 WHERE id = $1",
    )
    .bind(commit_id)
    .execute(&pool)
    .await
    .unwrap();
    let assigned = AssignedPolicy {
        policy_id: context.version_id,
        policy_name: "eval attempt evidence".into(),
        policy: DeploymentPolicy::Composite {
            config: config.clone(),
        },
    };
    let policies = BTreeMap::from([(hostname.clone(), vec![assigned.clone()])]);
    initialize_eval_passed_attempt(&pool, commit_id, 1, &policies)
        .await
        .unwrap();
    let pending: (String, Option<Uuid>) =
        sqlx::query_as("SELECT outcome, system_id FROM composite_eval_attempt_rule_results")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, ("not_checked".into(), Some(context.system_id)));

    let mut tx = pool.begin().await.unwrap();
    persist_eval_passed_for_system_in_tx(
        &mut tx,
        commit_id,
        1,
        context.system_id,
        &hostname,
        std::slice::from_ref(&assigned),
        EnforcementOutcome::Pass,
        "Configuration evaluation completed",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let passed: String = sqlx::query_scalar(
        "SELECT outcome FROM composite_eval_attempt_rule_results WHERE superseded_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(passed, "pass");

    sqlx::query("UPDATE evaluation_attempts SET status = 'complete' WHERE commit_id = $1")
        .bind(commit_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO evaluation_attempts (commit_id, attempt_number, status, started_at) VALUES ($1, 2, 'in_progress', NOW())",
    )
    .bind(commit_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE commits SET evaluation_attempt_count = 2 WHERE id = $1")
        .bind(commit_id)
        .execute(&pool)
        .await
        .unwrap();
    initialize_eval_passed_attempt(&pool, commit_id, 2, &policies)
        .await
        .unwrap();
    let states: Vec<(i32, String, bool)> = sqlx::query_as(
        r#"
        SELECT attempt.attempt_number, result.outcome, result.superseded_at IS NOT NULL
        FROM composite_eval_attempt_rule_results result
        JOIN evaluation_attempts attempt ON attempt.id = result.evaluation_attempt_id
        ORDER BY attempt.attempt_number
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        states,
        vec![(1, "pass".into(), true), (2, "not_checked".into(), false)]
    );

    let failure = PolicyCheckResult::for_evaluation_terminal(
        hostname,
        std::slice::from_ref(&assigned),
        crystal_forge::models::deployment_policies::EvaluationTerminalOutcome::ConfirmedFailure,
        "target evaluation failed",
    );
    let mut tx = pool.begin().await.unwrap();
    persist_eval_passed_terminal_checks_in_tx(&mut tx, commit_id, 2, &[failure])
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let failed: String = sqlx::query_scalar(
        "SELECT outcome FROM composite_eval_attempt_rule_results WHERE superseded_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(failed, "fail");

    sqlx::query(
        "UPDATE evaluation_attempts SET status = 'complete' WHERE commit_id = $1 AND attempt_number = 2",
    )
    .bind(commit_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO evaluation_attempts (commit_id, attempt_number, status, started_at) VALUES ($1, 3, 'in_progress', NOW())",
    )
    .bind(commit_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE commits SET evaluation_status = 'in_progress', evaluation_attempt_count = 3 WHERE id = $1",
    )
    .bind(commit_id)
    .execute(&pool)
    .await
    .unwrap();
    initialize_eval_passed_attempt(&pool, commit_id, 3, &policies)
        .await
        .unwrap();
    mark_commit_evaluation_failed(
        &pool,
        commit_id,
        "evaluator transport failed",
        3,
        crystal_forge::models::retry_policy::RetryFailureClass::Transient,
    )
    .await
    .unwrap();
    let error: (String, String) = sqlx::query_as(
        "SELECT outcome, detail FROM composite_eval_attempt_rule_results WHERE superseded_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(error, ("error".into(), "evaluator transport failed".into()));
}

#[sqlx::test]
async fn scan_before_evaluation_is_merged_and_later_evaluation_preserves_newest_scan(pool: PgPool) {
    let context = assessment_context(&pool).await;
    let scan_id = create_cve_scan(&pool, context.derivation_id, "test", None)
        .await
        .unwrap()
        .id();
    complete_cve_scan(&pool, scan_id, 1, 0, 0, 0, 0, 0, Some(1), None)
        .await
        .unwrap();

    persist_evaluation(&pool, &context, EnforcementOutcome::Fail).await;
    let after_first_eval: (String, Option<Uuid>) = sqlx::query_as(
        "SELECT outcome, source_scan_id FROM composite_policy_rule_results WHERE phase = 'scan'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_first_eval, ("pass".to_string(), Some(scan_id)));

    persist_evaluation(&pool, &context, EnforcementOutcome::Pass).await;
    let after_second_eval: Vec<(String, String, Option<Uuid>)> = sqlx::query_as(
        r#"
        SELECT phase, outcome, source_scan_id
        FROM composite_policy_rule_results
        ORDER BY ordinal
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        after_second_eval,
        vec![
            ("evaluation".into(), "pass".into(), None),
            ("scan".into(), "pass".into(), Some(scan_id)),
            ("deployment".into(), "not_checked".into(), None),
        ]
    );
}

#[sqlx::test]
async fn final_authorization_aggregates_multiple_exact_policy_versions_atomically(pool: PgPool) {
    let mut context = assessment_context(&pool).await;
    let second_version = add_phase_policy(&pool, context.system_id).await;
    context.resolved = match resolve_system_effective_policies(&pool, context.system_id)
        .await
        .unwrap()
    {
        ResolutionOutcome::Resolved(resolved) => resolved,
        ResolutionOutcome::Conflict(conflicts) => panic!("unexpected conflict: {conflicts:?}"),
    };
    let config = phase_config();
    let versions = [context.version_id, second_version];
    let mut tx = pool.begin().await.unwrap();
    persist_evaluation_assessments_in_tx(
        &mut tx,
        context.system_id,
        context.derivation_id,
        &context.store_path,
        &policy_results_for_versions(
            &versions,
            &config,
            &[EnforcementOutcome::Pass, EnforcementOutcome::Fail],
        ),
        &context.resolved,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let scan_id = create_cve_scan(&pool, context.derivation_id, "test", None)
        .await
        .unwrap()
        .id();
    complete_cve_scan(&pool, scan_id, 1, 0, 0, 0, 0, 0, Some(1), None)
        .await
        .unwrap();

    let blocked = authorize_and_set_system_target(
        &pool,
        context.system_id,
        &context.store_path,
        "manual_deploy",
    )
    .await
    .unwrap();
    assert_eq!(blocked.outcome, EnforcementOutcome::Fail);
    assert_eq!(blocked.assessments.len(), 2);
    let desired: Option<String> =
        sqlx::query_scalar("SELECT desired_target FROM systems WHERE id = $1")
            .bind(context.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(desired, None);

    let mut tx = pool.begin().await.unwrap();
    persist_evaluation_assessments_in_tx(
        &mut tx,
        context.system_id,
        context.derivation_id,
        &context.store_path,
        &policy_results_for_versions(
            &versions,
            &config,
            &[EnforcementOutcome::Pass, EnforcementOutcome::Pass],
        ),
        &context.resolved,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let allowed = authorize_and_set_system_target(
        &pool,
        context.system_id,
        &context.store_path,
        "manual_deploy",
    )
    .await
    .unwrap();
    assert_eq!(allowed.outcome, EnforcementOutcome::Pass);
    assert_eq!(allowed.assessments.len(), 2);
}

#[sqlx::test]
async fn assessment_identity_rejects_mismatched_derivation_path_and_duplicate_context(
    pool: PgPool,
) {
    let context = assessment_context(&pool).await;
    persist_evaluation(&pool, &context, EnforcementOutcome::Pass).await;
    let assessment: (Uuid, Uuid, Uuid, String, String, serde_json::Value) = sqlx::query_as(
        r#"
        SELECT policy_lineage_id, policy_version_id, system_id,
               effective_set_digest, effective_config_digest, effective_config
        FROM composite_policy_assessments
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let mismatch = sqlx::query(
        r#"
        INSERT INTO composite_policy_assessments (
            system_id, derivation_id, target_store_path, policy_lineage_id,
            policy_version_id, effective_set_digest, effective_config_digest, effective_config
        ) VALUES ($1, $2, '/nix/store/not-the-derivation-target', $3, $4, 'other-set', $5, $6)
        "#,
    )
    .bind(assessment.2)
    .bind(context.derivation_id)
    .bind(assessment.0)
    .bind(assessment.1)
    .bind(&assessment.4)
    .bind(&assessment.5)
    .execute(&pool)
    .await;
    assert!(mismatch.is_err());

    let duplicate = sqlx::query(
        r#"
        INSERT INTO composite_policy_assessments (
            system_id, derivation_id, target_store_path, policy_lineage_id,
            policy_version_id, effective_set_digest, effective_config_digest, effective_config
        ) VALUES ($1, $2, $3, $4, $5, $6, 'different-config-digest', $7)
        "#,
    )
    .bind(assessment.2)
    .bind(context.derivation_id)
    .bind(&context.store_path)
    .bind(assessment.0)
    .bind(assessment.1)
    .bind(&assessment.3)
    .bind(&assessment.5)
    .execute(&pool)
    .await;
    assert!(duplicate.is_err());

    let scan_id = create_cve_scan(&pool, context.derivation_id, "test", None)
        .await
        .unwrap()
        .id();
    let (scan_order,): (i64,) =
        sqlx::query_as("SELECT composite_phase_order FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let assessment_id: Uuid =
        sqlx::query_scalar("SELECT id FROM composite_policy_assessments LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let inconsistent_scan_source = sqlx::query(
        r#"
        UPDATE composite_policy_rule_results
        SET source_scan_id = $1, source_scan_order = $2, source_scan_derivation_id = $3
        WHERE assessment_id = $4 AND phase = 'scan'
        "#,
    )
    .bind(scan_id)
    .bind(scan_order + 1)
    .bind(context.derivation_id)
    .bind(assessment_id)
    .execute(&pool)
    .await;
    assert!(inconsistent_scan_source.is_err());
}
