//! Disposable-database coverage of real resolver inputs and historical capture.

use std::collections::HashMap;

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::policy_capture;
use super::{
    evaluation_policy_digest, load_and_capture_policies_for_eval,
    load_policies_by_configuration_for_eval,
};
use crate::api::eval_policy_matrix::*;
use crate::compliance::resolver::{
    ResolutionOutcome, resolve_systems_effective_policies_and_contexts_in_tx,
};

async fn policy(pool: &PgPool, name: &str, enabled: bool) -> (Uuid, Uuid) {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies (name, policy_type, config, enabled)
         VALUES ($1, 'custom_check', $2, $3) RETURNING id",
    )
    .bind(name)
    .bind(json!({"expression": "true", "strict": true}))
    .bind(enabled)
    .fetch_one(pool)
    .await
    .unwrap();
    let version: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET description='Exact version description', compliance_metadata=$2 WHERE id=$1")
        .bind(version)
        .bind(json!({"source_rule_id":"SV-123r1_rule", "source_group_id":"V-123", "config":{"secret":"never-copy"}, "assertion":"never-copy-expression"}))
        .execute(pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET publication_state = 'accepted',
                 published_at = NOW(), trust_state = 'trusted', semantic_digest = 'test-policy-digest'
                 WHERE id = $1")
        .bind(version)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2")
        .bind(version)
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (id, version)
}

async fn bundle(pool: &PgPool, name: &str, policies: &[Uuid]) -> (Uuid, Uuid) {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundles (name, framework, version, layer, owner)
         VALUES ($1, 'Captured framework', '2026', 'os', 'Tests') RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap();
    let version: Uuid =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    for (index, policy) in policies.iter().enumerate() {
        sqlx::query("INSERT INTO compliance_bundle_version_policies
                     (bundle_version_id, policy_version_id, policy_order, selected) VALUES ($1, $2, $3, TRUE)")
            .bind(version)
            .bind(policy)
            .bind(index as i32)
            .execute(pool)
            .await
            .unwrap();
    }
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE compliance_bundle_versions SET publication_state = 'accepted',
                 published_at = NOW(), trust_state = 'trusted', semantic_digest = 'test-bundle-digest'
                 WHERE id = $1")
        .bind(version)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
        .bind(version)
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (id, version)
}

async fn assignment(
    pool: &PgPool,
    bundle: (Uuid, Uuid),
    scope: &str,
    target: Uuid,
) -> (Uuid, Uuid) {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_assignments (bundle_id, bundle_version_id, scope_type,
             system_id, environment_id, enforcement_mode, assignment_overlay_digest)
         VALUES ($1, $2, $3, CASE WHEN $3 = 'system' THEN $4 END,
             CASE WHEN $3 = 'environment' THEN $4 END, 'enforce', 'test-overlay') RETURNING id",
    )
    .bind(bundle.0)
    .bind(bundle.1)
    .bind(scope)
    .bind(target)
    .fetch_one(pool)
    .await
    .unwrap();
    let version: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_assignment_versions
             (assignment_id, version_number, bundle_version_id, enforcement_mode, assignment_overlay_digest)
         VALUES ($1, 1, $2, 'enforce', 'test-overlay') RETURNING id",
    )
    .bind(id)
    .bind(bundle.1)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2")
        .bind(version)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    (id, version)
}

async fn environment(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
        .bind(format!("capture-env-{}", Uuid::new_v4()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn system(pool: &PgPool, flake: i32, env: Uuid, config: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO systems (hostname, environment_id, flake_id, system_configuration_name, public_key, derivation)
         VALUES ($1, $2, $3, $4, 'test-key', '/nix/store/test') RETURNING id",
    )
    .bind(format!("capture-{config}-{}", Uuid::new_v4()))
    .bind(env)
    .bind(flake)
    .bind(config)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn flake(pool: &PgPool) -> i32 {
    sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, 'https://example.invalid/capture') RETURNING id")
        .bind(format!("capture-{}", Uuid::new_v4()))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn real_loader_captures_overlap_empty_contexts_and_immutable_history(pool: PgPool) {
    let flake = flake(&pool).await;
    let env = environment(&pool).await;
    let alpha = system(&pool, flake, env, "alpha").await;
    let beta = system(&pool, flake, env, "beta").await;
    let actor = crate::queries::users::insert_user(&pool, "capture-actor@example.invalid", None)
        .await
        .unwrap()
        .id;
    sqlx::query("INSERT INTO system_evaluation_lifecycle(system_id,greenfield,classified_by) VALUES($1,true,$2),($3,false,$2)")
        .bind(alpha).bind(actor).bind(beta).execute(&pool).await.unwrap();
    let (policy_id, policy_version) = policy(&pool, "Exact policy version", true).await;
    let (_, disabled_version) = policy(&pool, "Disabled check", false).await;
    let first = bundle(&pool, "First bundle", &[policy_version]).await;
    let second = bundle(&pool, "Second bundle", &[policy_version]).await;
    let empty = bundle(&pool, "Empty assigned bundle", &[]).await;
    let disabled = bundle(&pool, "No eligible controls", &[disabled_version]).await;
    let first_assignment = assignment(&pool, first, "system", alpha).await;
    let second_assignment = assignment(&pool, second, "system", alpha).await;
    let empty_assignment = assignment(&pool, empty, "environment", env).await;
    assignment(&pool, disabled, "system", beta).await;

    // Compare the existing execution-only loader with the capturing path. Empty
    // contexts must not enable a policy or add evaluator input/digest entries.
    let expected = load_policies_by_configuration_for_eval(&pool, flake)
        .await
        .unwrap();
    assert_eq!(expected.len(), 1);
    assert_eq!(expected["alpha"].len(), 1);
    let commit: i32 = sqlx::query_scalar("INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, NOW()) RETURNING id")
        .bind(flake)
        .bind(Uuid::new_v4().to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    let attempt = match crate::queries::commits::mark_commit_evaluation_started(&pool, commit)
        .await
        .unwrap()
    {
        crate::queries::commits::EvalStartOutcome::Started { attempt } => attempt,
        other => panic!("fixture must start: {other:?}"),
    };
    let actual = load_and_capture_policies_for_eval(&pool, flake, Some((commit, attempt)))
        .await
        .unwrap();
    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>()
    );
    assert_eq!(actual["alpha"][0].policy_id, policy_version);
    assert_eq!(
        actual["alpha"][0].policy_name,
        expected["alpha"][0].policy_name
    );
    assert_eq!(
        actual["alpha"][0].enforcement_mode,
        expected["alpha"][0].enforcement_mode
    );
    assert_eq!(
        serde_json::to_value(&actual["alpha"][0].policy).unwrap(),
        serde_json::to_value(&expected["alpha"][0].policy).unwrap()
    );
    let captured = crate::queries::eval_policy_plans::fetch_matrix(&pool, commit)
        .await
        .unwrap()
        .1;
    assert!(captured.empty_bundle_metadata_complete);
    assert_eq!(captured.columns.len(), 1);
    assert_eq!(captured.columns[0].policy_version_id, policy_version);
    assert_eq!(captured.columns[0].policy_id, policy_id);
    assert_eq!(
        captured.columns[0].description.as_deref(),
        Some("Exact version description")
    );
    assert_eq!(
        captured.columns[0].identifiers,
        vec!["SV-123r1_rule", "V-123"]
    );
    assert!(captured.columns[0].assertion.is_none());
    assert!(captured.columns[0].attribute_path.is_none());
    assert!(
        !serde_json::to_string(&captured)
            .unwrap()
            .contains("never-copy")
    );
    assert_eq!(captured.bundles.len(), 4);
    let alpha_row = captured
        .rows
        .iter()
        .find(|r| r.system_id == Some(alpha))
        .unwrap();
    let beta_row = captured
        .rows
        .iter()
        .find(|r| r.system_id == Some(beta))
        .unwrap();
    assert_eq!(alpha_row.assigned_bundles.len(), 3);
    assert_eq!(alpha_row.greenfield, Some(true));
    assert_eq!(beta_row.greenfield, Some(false));
    assert_eq!(beta_row.assigned_bundles.len(), 2);
    assert!(beta_row.policies.is_empty());
    assert_eq!(beta_row.resolution, EvalMatrixResolution::Resolved);
    assert_eq!(beta_row.cells, vec![EvalMatrixCell::NotAssigned]);
    assert_eq!(alpha_row.policies[0].memberships.len(), 2);
    for (assignment_id, assignment_version_id) in [first_assignment, second_assignment] {
        assert!(alpha_row.policies[0].memberships.iter().any(|m| {
            m.assignment_id == Some(assignment_id)
                && m.assignment_version_id == Some(assignment_version_id)
        }));
    }
    assert!(
        alpha_row
            .assigned_bundles
            .iter()
            .any(|m| m.assignment_version_id == Some(empty_assignment.1))
    );
    assert_eq!(
        alpha_row.evaluation_digest.as_deref(),
        Some(evaluation_policy_digest(&expected["alpha"], &[]).as_str())
    );
    assert_eq!(
        beta_row.evaluation_digest.as_deref(),
        Some(evaluation_policy_digest(&[], &[]).as_str())
    );

    // Create mutable successor drafts through the normal pointer model, then
    // change today's labels, framework, enabled state, and assignment activity.
    sqlx::query("UPDATE system_evaluation_lifecycle SET greenfield=NULL WHERE system_id=$1")
        .bind(alpha)
        .execute(&pool)
        .await
        .unwrap();
    let draft: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policy_versions (policy_id, version, name, policy_type, config, semantic_digest)
         SELECT policy_id, '0.2.0', name, policy_type, config, 'pending'
         FROM deployment_policy_versions WHERE id = $1 RETURNING id",
    )
    .bind(policy_version)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = $1 WHERE id = $2")
        .bind(draft)
        .bind(policy_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE deployment_policies SET name = 'Today policy label', description='Today description', enabled = FALSE WHERE id = $1",
    )
    .bind(policy_id)
    .execute(&pool)
    .await
    .unwrap();
    let draft: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_versions (bundle_id, version, name, framework, layer, owner, semantic_digest)
         SELECT bundle_id, '0.2.0', name, framework, layer, owner, 'pending'
         FROM compliance_bundle_versions WHERE id = $1 RETURNING id",
    )
    .bind(first.1)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = $1 WHERE id = $2")
        .bind(draft)
        .bind(first.0)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE compliance_bundles SET name = 'Today bundle label', framework = 'Today framework' WHERE id = $1")
        .bind(first.0)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE compliance_bundle_assignments SET active = FALSE WHERE id = $1")
        .bind(second_assignment.0)
        .execute(&pool)
        .await
        .unwrap();
    let historical = crate::queries::eval_policy_plans::fetch_matrix(&pool, commit)
        .await
        .unwrap()
        .1;
    assert_eq!(
        serde_json::to_value(&captured).unwrap(),
        serde_json::to_value(&historical).unwrap()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn real_virtual_baseline_capture_preserves_bundle_without_assignment_uuids(pool: PgPool) {
    let flake = flake(&pool).await;
    let env = environment(&pool).await;
    let system = system(&pool, flake, env, "virtual").await;
    let (_, policy) = policy(&pool, "Virtual baseline policy", true).await;
    let bundle = bundle(&pool, "Virtual published bundle", &[policy]).await;
    sqlx::query(
        "INSERT INTO compliance_bundle_environments (bundle_id, environment_id) VALUES ($1, $2)",
    )
    .bind(bundle.0)
    .bind(env)
    .execute(&pool)
    .await
    .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_assignments WHERE bundle_id = $1",
    )
    .bind(bundle.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
    let requests = HashMap::from([(bundle.1, vec![system])]);
    let mut contexts = HashMap::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await
        .unwrap();
    let mut outcomes = resolve_systems_effective_policies_and_contexts_in_tx(
        &mut tx,
        &[system],
        Some(&requests),
        true,
        &mut contexts,
    )
    .await
    .unwrap();
    // Select this exact-version request for a single-system capture projection.
    // Normal evaluation still passes no version requests and gains no implicit
    // virtual assignments from this test or the new metadata path.
    let outcome = outcomes.remove(&(Some(bundle.1), system)).unwrap();
    let contexts = HashMap::from([(
        (None, system),
        contexts.remove(&(Some(bundle.1), system)).unwrap(),
    )]);
    let outcomes = HashMap::from([((None, system), outcome)]);
    let mut capture = policy_capture::new_snapshot();
    policy_capture::initialize_rows(
        &mut capture,
        &[(system, "virtual".into(), "virtual-host".into(), None)],
        &outcomes,
    );
    let metadata = policy_capture::load_metadata(&mut tx, &[policy], &contexts)
        .await
        .unwrap();
    policy_capture::bind_contexts(&mut capture, &metadata, &contexts).unwrap();
    let ResolutionOutcome::Resolved(set) = &outcomes[&(None, system)] else {
        panic!("virtual baseline must resolve");
    };
    let effective = set
        .policies
        .iter()
        .find(|p| p.policy_version_id == policy)
        .unwrap();
    policy_capture::bind_policy(&metadata, &mut capture, system, effective, true).unwrap();
    assert_eq!(capture.bundles.len(), 1);
    assert_eq!(capture.bundles[0].bundle_version_id, bundle.1);
    assert!(capture.empty_bundle_metadata_complete);
    for membership in capture.rows[0]
        .assigned_bundles
        .iter()
        .chain(capture.rows[0].policies[0].memberships.iter())
    {
        assert_eq!(membership.bundle_version_id, bundle.1);
        assert_eq!(membership.source, EvalBundleSource::VirtualBaseline);
        assert!(membership.assignment_id.is_none() && membership.assignment_version_id.is_none());
    }
    tx.rollback().await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn capture_display_metadata_and_classification_share_repeatable_read_snapshot(pool: PgPool) {
    let flake = flake(&pool).await;
    let env = environment(&pool).await;
    let system = system(&pool, flake, env, "greenfield-is-only-a-name").await;
    let actor = crate::queries::users::insert_user(&pool, "rr-capture@example.invalid", None)
        .await
        .unwrap()
        .id;
    let lineage: Uuid = sqlx::query_scalar("INSERT INTO deployment_policies(name,policy_type,config) VALUES('Name is not REQ-FAKE','custom_check','{}') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    let policy: Uuid =
        sqlx::query_scalar("SELECT current_draft_version_id FROM deployment_policies WHERE id=$1")
            .bind(lineage)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET description='Version description before', compliance_metadata=$2 WHERE id=$1")
        .bind(policy).bind(json!({"source_rule_id":"RULE-before", "assertion":"secret", "checks":[{"content":"secret"}]}))
        .execute(&pool).await.unwrap();
    let framework: Uuid = sqlx::query_scalar("INSERT INTO compliance_frameworks(name,canonical_source_key) VALUES('Capture framework','capture-rr') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    let release: Uuid = sqlx::query_scalar("INSERT INTO compliance_framework_versions(framework_id,version,canonical_release_key) VALUES($1,'v1','v1') RETURNING id")
        .bind(framework).fetch_one(&pool).await.unwrap();
    for (external_id, trust) in [("REQ-trusted", "trusted"), ("REQ-suggested", "suggested")] {
        let requirement: Uuid = sqlx::query_scalar("INSERT INTO compliance_requirements(framework_id,canonical_requirement_key) VALUES($1,$2) RETURNING id")
            .bind(framework).bind(external_id).fetch_one(&pool).await.unwrap();
        let version: Uuid = sqlx::query_scalar("INSERT INTO compliance_requirement_versions(requirement_id,framework_version_id,external_id,kind) VALUES($1,$2,$3,'control') RETURNING id")
            .bind(requirement).bind(release).bind(external_id).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO policy_requirement_mappings(policy_version_id,requirement_version_id,relationship,coverage,trust_state) VALUES($1,$2,'supports','partial',$3)")
            .bind(policy).bind(version).bind(trust).execute(&pool).await.unwrap();
    }
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await
        .unwrap();
    // Establish the loader snapshot before concurrent operator/catalog edits.
    // No classification row exists: the name and absence of deployment state
    // must not invent an established or greenfield classification.
    let rows: Vec<(Uuid, String, String, Option<bool>)> = sqlx::query_as(
        "SELECT s.id,s.system_configuration_name,s.hostname,l.greenfield FROM systems s LEFT JOIN system_evaluation_lifecycle l ON l.system_id=s.id WHERE s.id=$1")
        .bind(system).fetch_all(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO system_evaluation_lifecycle(system_id,greenfield,classified_by) VALUES($1,true,$2)")
        .bind(system).bind(actor).execute(&pool).await.unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET description='Version description after',compliance_metadata='{}' WHERE id=$1")
        .bind(policy).execute(&pool).await.unwrap();
    sqlx::query(
        "UPDATE policy_requirement_mappings SET trust_state='suggested' WHERE policy_version_id=$1",
    )
    .bind(policy)
    .execute(&pool)
    .await
    .unwrap();
    let metadata = policy_capture::load_metadata(&mut tx, &[policy], &HashMap::new())
        .await
        .unwrap();
    let mut capture = policy_capture::new_snapshot();
    policy_capture::initialize_rows(&mut capture, &rows, &HashMap::new());
    assert_eq!(capture.rows[0].greenfield, None);
    let column = &metadata.policies[&policy];
    assert_eq!(
        column.description.as_deref(),
        Some("Version description before")
    );
    assert_eq!(column.identifiers, vec!["REQ-trusted", "RULE-before"]);
    assert!(column.assertion.is_none() && column.attribute_path.is_none());
    assert!(!serde_json::to_string(column).unwrap().contains("secret"));
    tx.commit().await.unwrap();
    let mut fresh = pool.begin().await.unwrap();
    let today = policy_capture::load_metadata(&mut fresh, &[policy], &HashMap::new())
        .await
        .unwrap();
    assert_eq!(
        today.policies[&policy].description.as_deref(),
        Some("Version description after")
    );
    assert!(today.policies[&policy].identifiers.is_empty());
    fresh.rollback().await.unwrap();
}
