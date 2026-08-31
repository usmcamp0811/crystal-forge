use axum::{Router, routing::get};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use crystal_forge::auth::session::{
    CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
};
use crystal_forge::compliance::canonical::semantic_digest;
use crystal_forge::compliance::resolver::{
    EffectivePolicySet, ResolutionOutcome, resolve_system_effective_policies,
};
use crystal_forge::handlers::agent_request::CFState;
use crystal_forge::handlers::api::poam as poam_handlers;
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::models::deployment_policies::{
    CompositePolicyConfig, CompositeRuleOutcome, CreateDeploymentPolicyRequest, EnforcementOutcome,
    EnforcementPhase, composite_config_digest,
};
use crystal_forge::models::poam::{
    AddFindingRequest, AssignmentReferenceRequest, CreatePoamRequest, CreateWaiverRequest,
    FindingObservationReference, FindingObservationSource, PoamDetailQuery, PoamListQuery,
    PoamRisk, PoamStatus, TransitionPoamRequest, UpdatePoamRequest, WaiverDecision,
    WaiverDecisionRequest,
};
use crystal_forge::models::system_states::SystemState;
use crystal_forge::queries::compliance::nix_policy_observation_reference;
use crystal_forge::queries::poam;
use crystal_forge::queries::users::insert_user;
use crystal_forge::queries::{
    auth_identity::{create_user_session, sync_user_role},
    commits::insert_commit,
    deployment_policies::create_deployment_policy,
    derivations::{SuccessfulEvalWrite, record_successful_eval_result},
    flakes::insert_flake,
    system_states::insert_system_state,
};
use crystal_forge::queue::QueueNotifier;
use crystal_forge::server::jobs::BackgroundJobRegistry;
use crystal_forge::services::composite_enforcement::persist_evaluation_assessments_in_tx;
use crystal_forge::services::poam::{self as poam_service, PoamActor, PoamClock, PoamError};
use sqlx::{PgPool, Postgres, Transaction};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

#[derive(Clone)]
struct FixedClock(DateTime<Utc>);

impl PoamClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

struct Fixture {
    user_id: Uuid,
    system_id: Uuid,
    policy_id: Uuid,
    finding_id: Uuid,
}

struct AssessmentFixture {
    user_id: Uuid,
    system_id: Uuid,
    version_id: Uuid,
    derivation_id: i32,
    store_path: String,
    config: CompositePolicyConfig,
    resolved: EffectivePolicySet,
}

fn assessment_config() -> CompositePolicyConfig {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "mode": "all",
        "rules": [{
            "id": "43360000-0000-0000-0000-000000000001",
            "kind": "nixos_option",
            "config": {
                "path": "networking.firewall.enable",
                "operator": "==",
                "value_type": "boolean",
                "value": true
            }
        }]
    }))
    .unwrap()
}

fn policy_results(
    version_id: Uuid,
    config: &CompositePolicyConfig,
    outcome: EnforcementOutcome,
) -> serde_json::Value {
    let rule = &config.rules[0];
    let result = CompositeRuleOutcome {
        rule_id: rule.id,
        kind: rule.rule.kind().to_string(),
        phase: EnforcementPhase::Evaluation,
        outcome,
        blocking: outcome != EnforcementOutcome::Pass,
        detail: format!("evaluation is {outcome:?}"),
        evidence: serde_json::json!({"source":"poam-race-test"}),
    };
    serde_json::json!({
        "assigned": {
            version_id.to_string(): {
                "config_digest": composite_config_digest(config),
                "rule_outcomes": [result]
            }
        }
    })
}

async fn persist_assessment(
    tx: &mut Transaction<'_, Postgres>,
    fixture: &AssessmentFixture,
    outcome: EnforcementOutcome,
) {
    persist_evaluation_assessments_in_tx(
        tx,
        fixture.system_id,
        fixture.derivation_id,
        &fixture.store_path,
        &policy_results(fixture.version_id, &fixture.config, outcome),
        &fixture.resolved,
    )
    .await
    .unwrap();
}

async fn current_assessment_id(pool: &PgPool, fixture: &AssessmentFixture) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM composite_policy_assessments WHERE system_id=$1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fixture.system_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn finding_id(pool: &PgPool, fixture: &AssessmentFixture) -> Uuid {
    sqlx::query_scalar("SELECT id FROM poam_findings WHERE system_id=$1 AND policy_lineage_id=$2")
        .bind(fixture.system_id)
        .bind(fixture.resolved.policies[0].policy_lineage_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn assessment_evidence_snapshot(pool: &PgPool, system_ids: &[Uuid]) -> serde_json::Value {
    sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(jsonb_build_object(
          'assessment',to_jsonb(a),
          'rules',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.ordinal,r.rule_id)
            FROM composite_policy_rule_results r WHERE r.assessment_id=a.id),'[]'::jsonb)
        ) ORDER BY a.system_id,a.policy_lineage_id,a.id),'[]'::jsonb)
        FROM composite_policy_assessments a WHERE a.system_id=ANY($1)"#,
    )
    .bind(system_ids)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn admin_actor(user_id: Uuid) -> PoamActor {
    PoamActor {
        user_id,
        identifier: format!("poam-admin-{user_id}@example.invalid"),
        is_admin: true,
        can_mutate: true,
        environment_ids: Vec::new(),
        request_origin: Some("poam-matrix".into()),
    }
}

async fn create_service_poam(
    pool: &PgPool,
    fixture: &AssessmentFixture,
    actor: &PoamActor,
    clock: &FixedClock,
    title: &str,
) -> crystal_forge::models::poam::PoamDetail {
    poam_service::create(
        pool,
        actor,
        CreatePoamRequest {
            assessment_id: Some(current_assessment_id(pool, fixture).await),
            finding_id: None,
            observation: None,
            title: title.into(),
            plan: "Matrix remediation".into(),
            owner: "Security Matrix".into(),
            target_date: Some(clock.today() + chrono::Duration::days(30)),
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: Vec::new(),
        },
        clock,
    )
    .await
    .unwrap()
}

async fn legacy_fail_fixture(
    pool: &PgPool,
) -> (
    AssessmentFixture,
    Uuid,
    FindingObservationReference,
    serde_json::Value,
) {
    let fixture = assessment_fixture(pool).await;
    let policy_lineage_id = fixture.resolved.policies[0].policy_lineage_id;
    sqlx::query(
        "UPDATE deployment_policies SET policy_type='custom_check',config='{}'::jsonb WHERE id=$1",
    )
    .bind(policy_lineage_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE deployment_policy_versions SET policy_type='custom_check',config='{}'::jsonb,trust_state='trusted' WHERE id=$1",
    )
    .bind(fixture.version_id)
    .execute(pool)
    .await
    .unwrap();
    let persisted_result = serde_json::json!({
        "assigned": {
            fixture.version_id.to_string(): {
                "passed": false,
                "details": "legacy custom check failed"
            }
        }
    });
    sqlx::query("UPDATE derivations SET policy_results=$1 WHERE id=$2")
        .bind(&persisted_result)
        .bind(fixture.derivation_id)
        .execute(pool)
        .await
        .unwrap();
    let finding_id: Uuid = sqlx::query_scalar(
        "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
    )
    .bind(fixture.system_id)
    .bind(policy_lineage_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let current_resolved = match resolve_system_effective_policies(pool, fixture.system_id)
        .await
        .unwrap()
    {
        ResolutionOutcome::Resolved(resolved) => resolved,
        ResolutionOutcome::Conflict(conflict) => panic!("unexpected policy conflict: {conflict:?}"),
    };
    let effective_policy = current_resolved
        .policies
        .iter()
        .find(|policy| policy.policy_lineage_id == policy_lineage_id)
        .unwrap();
    let effective_config_digest = semantic_digest(&effective_policy.effective_config);
    let observation = nix_policy_observation_reference(
        fixture.system_id,
        policy_lineage_id,
        fixture.version_id,
        &current_resolved.effective_set_digest,
        &effective_config_digest,
        fixture.derivation_id,
        &fixture.store_path,
        false,
        Some("legacy custom check failed"),
    );
    (fixture, finding_id, observation, persisted_result)
}

fn legacy_create_request(
    finding_id: Uuid,
    observation: FindingObservationReference,
) -> CreatePoamRequest {
    CreatePoamRequest {
        assessment_id: None,
        finding_id: Some(finding_id),
        observation: Some(observation),
        title: "Legacy policy remediation".into(),
        plan: "Correct the custom check".into(),
        owner: "Security".into(),
        target_date: None,
        risk: PoamRisk::High,
        default_milestones: false,
        assignment_version_ids: Vec::new(),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_fail_can_create_poam_without_fabricating_composite_assessment(pool: PgPool) {
    let (fixture, finding_id, observation, persisted_result) = legacy_fail_fixture(&pool).await;
    assert_eq!(
        observation.source,
        FindingObservationSource::NixPolicyResult
    );
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 28, 12, 0, 0).unwrap());
    let detail = poam_service::create(
        &pool,
        &admin_actor(fixture.user_id),
        legacy_create_request(finding_id, observation),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(detail.findings.len(), 1);
    assert_eq!(detail.findings[0].id, finding_id);
    let composite_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM composite_policy_assessments WHERE system_id=$1")
            .bind(fixture.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(composite_count, 0);
    let result_after: serde_json::Value =
        sqlx::query_scalar("SELECT policy_results FROM derivations WHERE id=$1")
            .bind(fixture.derivation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(result_after, persisted_result);
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_fail_pass_verification_close_and_rollups_retain_source_neutral_history(
    pool: PgPool,
) {
    let (fixture, finding_id, observation, _) = legacy_fail_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap());
    let created = poam_service::create(
        &pool,
        &actor,
        legacy_create_request(finding_id, observation),
        &clock,
    )
    .await
    .unwrap();

    let (assignment_id, _assignment_version_id, bundle_id) =
        immutable_assignment_fixture_for_version(
            &pool,
            fixture.system_id,
            fixture.user_id,
            fixture.version_id,
        )
        .await;
    let assignment_environment_id: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!(
                "poam-env-{}",
                &fixture.system_id.simple().to_string()[..8]
            ))
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(fixture.system_id)
        .bind(assignment_environment_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE compliance_bundle_assignments
         SET scope_type='environment',environment_id=$2,system_id=NULL
         WHERE id=$1",
    )
    .bind(assignment_id)
    .bind(assignment_environment_id)
    .execute(&pool)
    .await
    .unwrap();
    let bundle_version_id: Uuid = sqlx::query_scalar(
        "SELECT bundle_version_id FROM compliance_bundle_version_policies WHERE policy_version_id=$1",
    )
    .bind(fixture.version_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut publish = pool.begin().await.unwrap();
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id=NULL WHERE id=$1")
        .bind(fixture.resolved.policies[0].policy_lineage_id)
        .execute(&mut *publish)
        .await
        .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET publication_state='accepted',semantic_digest='legacy-poam-policy-v1',trust_state='trusted',implementation_state='native' WHERE id=$1")
        .bind(fixture.version_id).execute(&mut *publish).await.unwrap();
    sqlx::query("UPDATE deployment_policies SET current_published_version_id=$2 WHERE id=$1")
        .bind(fixture.resolved.policies[0].policy_lineage_id)
        .bind(fixture.version_id)
        .execute(&mut *publish)
        .await
        .unwrap();
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id=NULL WHERE id=$1")
        .bind(bundle_id)
        .execute(&mut *publish)
        .await
        .unwrap();
    sqlx::query("UPDATE compliance_bundle_versions SET publication_state='accepted',semantic_digest='legacy-poam-bundle-v1',trust_state='trusted' WHERE id=$1")
        .bind(bundle_version_id).execute(&mut *publish).await.unwrap();
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id=$2 WHERE id=$1")
        .bind(bundle_id)
        .bind(bundle_version_id)
        .execute(&mut *publish)
        .await
        .unwrap();
    publish.commit().await.unwrap();
    sqlx::query("UPDATE compliance_bundle_assignments SET active=true WHERE id=$1")
        .bind(assignment_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM system_policies WHERE system_id=$1 AND policy_id=$2")
        .bind(fixture.system_id)
        .bind(fixture.resolved.policies[0].policy_lineage_id)
        .execute(&pool)
        .await
        .unwrap();

    let system = poam_service::system_rollups(&pool, &actor, &[fixture.system_id], &clock)
        .await
        .unwrap();
    assert_eq!(
        (
            system[0].open_findings,
            system[0].on_poam_findings,
            system[0].no_poam_findings,
        ),
        (1, 1, 0)
    );
    let bundle = poam_service::bundle_rollups(&pool, &actor, &[bundle_id], &clock)
        .await
        .unwrap();
    assert_eq!(
        (
            bundle[0].total,
            bundle[0].active,
            bundle[0].open_findings,
            bundle[0].on_poam_findings,
            bundle[0].no_poam_findings,
        ),
        (1, 1, 1, 1, 0)
    );

    let awaiting = poam_service::transition(
        &pool,
        &actor,
        created.poam.id,
        TransitionPoamRequest {
            revision: created.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: Some("Remediation deployed".into()),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(awaiting.poam.status, "awaiting_verification");

    let passing_result = serde_json::json!({
        "assigned": {
            fixture.version_id.to_string(): {
                "passed": true,
                "details": "legacy custom check now passes"
            }
        }
    });
    sqlx::query("UPDATE derivations SET policy_results=$1 WHERE id=$2")
        .bind(&passing_result)
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .unwrap();

    let verified = poam_service::verify(
        &pool,
        &actor,
        awaiting.poam.id,
        awaiting.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(verified["outcome"], "accepted");
    assert_eq!(verified["items"][0]["result"], "pass");
    assert!(verified["items"][0]["assessment_id"].is_null());

    let mut malformed_context = pool.begin().await.unwrap();
    let malformed_context_attempt_id: Uuid = sqlx::query_scalar(
        "INSERT INTO poam_verification_attempts(poam_id,attempted_by,outcome,poam_revision)
         VALUES($1,$2,'accepted',$3) RETURNING id",
    )
    .bind(created.poam.id)
    .bind(actor.user_id)
    .bind(verified["revision"].as_i64().unwrap())
    .fetch_one(&mut *malformed_context)
    .await
    .unwrap();
    let malformed_attestation = sqlx::query(
        r#"INSERT INTO poam_effective_context_attestations(
             attempt_id,finding_id,system_id,policy_lineage_id,policy_version_id,
             derivation_id,target_store_path,effective_set_digest,
             effective_config_digest,effective_config,observed_outcome,
             observation_token,observation_snapshot)
           SELECT $2,finding_id,system_id,policy_lineage_id,policy_version_id,
             derivation_id,target_store_path,'caller-chosen-effective-set',
             effective_config_digest,effective_config,observed_outcome,
             observation_token,observation_snapshot
           FROM poam_effective_context_attestations WHERE attempt_id=$1"#,
    )
    .bind(Uuid::parse_str(verified["attempt_id"].as_str().unwrap()).unwrap())
    .bind(malformed_context_attempt_id)
    .execute(&mut *malformed_context)
    .await
    .unwrap_err();
    assert!(
        malformed_attestation
            .as_database_error()
            .is_some_and(|error| error.message().contains("database-held resolver context"))
    );
    malformed_context.rollback().await.unwrap();

    // INVARIANT: Source-neutral closure evidence must still match the
    // authoritative deployed result when the deferred closure constraint runs.
    // Copying a valid-looking Pass item into a new accepted attempt cannot close
    // the POA&M after the exact derivation result changes to Fail.
    let failing_result = serde_json::json!({
        "assigned": {
            fixture.version_id.to_string(): {
                "passed": false,
                "details": "authoritative result still fails"
            }
        }
    });
    sqlx::query("UPDATE derivations SET policy_results=$1 WHERE id=$2")
        .bind(&failing_result)
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .unwrap();
    let legitimate_attempt_id = Uuid::parse_str(verified["attempt_id"].as_str().unwrap()).unwrap();
    let mut forged = pool.begin().await.unwrap();
    let forged_attempt_id: Uuid = sqlx::query_scalar(
        "INSERT INTO poam_verification_attempts(poam_id,attempted_by,outcome,poam_revision) VALUES($1,$2,'accepted',$3) RETURNING id",
    )
    .bind(created.poam.id)
    .bind(actor.user_id)
    .bind(verified["revision"].as_i64().unwrap())
    .fetch_one(&mut *forged)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO poam_verification_items(
             attempt_id,finding_id,system_id,policy_lineage_id,result,policy_version_id,
             assessment_id,derivation_id,target_store_path,effective_set_digest,
             effective_config_digest,effective_config,observed_outcome,observation_token,
             observation_snapshot,assessment_updated_at,bundle_ids,bundle_version_ids,
             requirement_version_ids,waiver_id,observed_at,detail)
           SELECT $1,finding_id,system_id,policy_lineage_id,result,policy_version_id,
             assessment_id,derivation_id,target_store_path,effective_set_digest,
             effective_config_digest,effective_config,observed_outcome,observation_token,
             observation_snapshot,assessment_updated_at,bundle_ids,bundle_version_ids,
             requirement_version_ids,waiver_id,observed_at,'forged source-neutral Pass'
           FROM poam_verification_items WHERE attempt_id=$2"#,
    )
    .bind(forged_attempt_id)
    .bind(legitimate_attempt_id)
    .execute(&mut *forged)
    .await
    .unwrap();
    sqlx::query("UPDATE poam_verification_attempts SET sealed_at=CURRENT_TIMESTAMP WHERE id=$1")
        .bind(forged_attempt_id)
        .execute(&mut *forged)
        .await
        .unwrap();
    sqlx::query("UPDATE poam_finding_links SET retired_at=CURRENT_TIMESTAMP,retired_by=$2,retirement_reason='closed:'||$3::uuid::text WHERE poam_id=$1 AND retired_at IS NULL")
        .bind(created.poam.id)
        .bind(actor.user_id)
        .bind(forged_attempt_id)
        .execute(&mut *forged)
        .await
        .unwrap();
    sqlx::query("UPDATE poams SET status='completed',closed_at=CURRENT_TIMESTAMP,closure_attempt_id=$2 WHERE id=$1")
        .bind(created.poam.id)
        .bind(forged_attempt_id)
        .execute(&mut *forged)
        .await
        .unwrap();
    let forged_error = forged.commit().await.unwrap_err();
    assert_eq!(
        forged_error.as_database_error().unwrap().constraint(),
        Some("poams_authoritative_closure_evidence")
    );

    sqlx::query("UPDATE derivations SET policy_results=$1 WHERE id=$2")
        .bind(&passing_result)
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .unwrap();

    // INVARIANT: The active assignment pins immutable v1 even after the policy
    // lineage publishes v2. Closure must validate the exact assigned version,
    // not substitute the lineage's current pointer.
    sqlx::query("UPDATE deployment_policy_versions SET publication_state='deprecated' WHERE id=$1")
        .bind(fixture.version_id)
        .execute(&pool)
        .await
        .unwrap();
    let current_v2: Uuid = sqlx::query_scalar(
        r#"INSERT INTO deployment_policy_versions(
             policy_id,version,publication_state,published_at,name,description,
             policy_type,implementation_state,execution_phase,config,
             compliance_metadata,dependencies,semantic_digest,trust_state,
             derived_from_version_id,created_by)
           SELECT policy_id,'2.0.0','draft',NULL,name,description,
             policy_type,implementation_state,execution_phase,config,
             compliance_metadata,dependencies,'legacy-poam-policy-v2',trust_state,
             id,created_by
           FROM deployment_policy_versions WHERE id=$1 RETURNING id"#,
    )
    .bind(fixture.version_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut publish_v2 = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE deployment_policy_versions \
         SET publication_state='accepted',published_at=CURRENT_TIMESTAMP WHERE id=$1",
    )
    .bind(current_v2)
    .execute(&mut *publish_v2)
    .await
    .unwrap();
    sqlx::query("UPDATE deployment_policies SET current_published_version_id=$2 WHERE id=$1")
        .bind(fixture.resolved.policies[0].policy_lineage_id)
        .bind(current_v2)
        .execute(&mut *publish_v2)
        .await
        .unwrap();
    publish_v2.commit().await.unwrap();
    let pinned = match resolve_system_effective_policies(&pool, fixture.system_id)
        .await
        .unwrap()
    {
        ResolutionOutcome::Resolved(resolved) => resolved,
        ResolutionOutcome::Conflict(conflict) => panic!("unexpected policy conflict: {conflict:?}"),
    };
    assert_eq!(pinned.policies[0].policy_version_id, fixture.version_id);
    assert_ne!(pinned.policies[0].policy_version_id, current_v2);

    let closed = poam_service::close(
        &pool,
        &actor,
        awaiting.poam.id,
        verified["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(closed.poam.status, "completed");
    assert_eq!(closed.verification_attempts.len(), 2);
    assert!(closed.verification_attempts.iter().all(|attempt| {
        attempt.outcome == "accepted"
            && attempt.items.len() == 1
            && attempt.items[0].result == "pass"
            && attempt.items[0].assessment_id.is_none()
            && attempt.items[0]
                .observation_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot["source"] == "nix_policy_result")
    }));
    assert!(closed.activity.iter().any(|activity| {
        activity.kind == "status_changed"
            && activity.payload["from"] == "open"
            && activity.payload["to"] == "awaiting_verification"
    }));

    let composite_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM composite_policy_assessments WHERE system_id=$1")
            .bind(fixture.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(composite_count, 0);
    let result_after: serde_json::Value =
        sqlx::query_scalar("SELECT policy_results FROM derivations WHERE id=$1")
            .bind(fixture.derivation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(result_after, passing_result);

    let system = poam_service::system_rollups(&pool, &actor, &[fixture.system_id], &clock)
        .await
        .unwrap();
    assert_eq!(
        (
            system[0].total,
            system[0].completed,
            system[0].open_findings,
            system[0].on_poam_findings,
        ),
        (1, 1, 0, 0)
    );
    let bundle = poam_service::bundle_rollups(&pool, &actor, &[bundle_id], &clock)
        .await
        .unwrap();
    assert_eq!(
        (
            bundle[0].total,
            bundle[0].completed,
            bundle[0].open_findings,
            bundle[0].on_poam_findings,
        ),
        (1, 1, 0, 0)
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_finding_authorizes_before_observation_validation(pool: PgPool) {
    let (fixture, finding_id, mut observation, _) = legacy_fail_fixture(&pool).await;
    let hidden_environment: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("hidden-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(fixture.system_id)
        .bind(hidden_environment)
        .execute(&pool)
        .await
        .unwrap();
    observation.token = "invalid-token-that-must-not-be-validated".into();
    let actor = PoamActor {
        user_id: fixture.user_id,
        identifier: "out-of-scope@example.invalid".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: Vec::new(),
        request_origin: None,
    };
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 28, 12, 0, 0).unwrap());
    assert!(matches!(
        poam_service::create(
            &pool,
            &actor,
            legacy_create_request(finding_id, observation),
            &clock,
        )
        .await,
        Err(PoamError::NotFound)
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn closure_constraint_rejects_malformed_same_transaction_snapshot(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 29, 13, 0, 0).unwrap());
    let mut failed = pool.begin().await.unwrap();
    persist_assessment(&mut failed, &fixture, EnforcementOutcome::Fail).await;
    failed.commit().await.unwrap();
    let created = create_service_poam(
        &pool,
        &fixture,
        &actor,
        &clock,
        "Composite closure validation",
    )
    .await;
    let awaiting = awaiting_verification(&pool, &actor, created, &clock).await;
    let mut passing = pool.begin().await.unwrap();
    persist_assessment(&mut passing, &fixture, EnforcementOutcome::Pass).await;
    passing.commit().await.unwrap();
    let verified = poam_service::verify(
        &pool,
        &actor,
        awaiting.poam.id,
        awaiting.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(verified["outcome"], "accepted");
    let legitimate_attempt_id = Uuid::parse_str(verified["attempt_id"].as_str().unwrap()).unwrap();

    let mut forged = pool.begin().await.unwrap();
    let forged_attempt_id: Uuid = sqlx::query_scalar(
        "INSERT INTO poam_verification_attempts(poam_id,attempted_by,outcome,poam_revision)
         VALUES($1,$2,'accepted',$3) RETURNING id",
    )
    .bind(awaiting.poam.id)
    .bind(actor.user_id)
    .bind(verified["revision"].as_i64().unwrap())
    .fetch_one(&mut *forged)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO poam_verification_items(
             attempt_id,finding_id,system_id,policy_lineage_id,result,policy_version_id,
             assessment_id,derivation_id,target_store_path,effective_set_digest,
             effective_config_digest,effective_config,observed_outcome,observation_token,
             observation_snapshot,assessment_updated_at,bundle_ids,bundle_version_ids,
             requirement_version_ids,waiver_id,observed_at,detail)
           SELECT $1,finding_id,system_id,policy_lineage_id,result,policy_version_id,
             assessment_id,derivation_id,target_store_path,'forged-effective-set',
             effective_config_digest,effective_config,observed_outcome,
             encode(digest(canonical_poam_observation_json(
                 jsonb_set(observation_snapshot,'{rules}','[]'::jsonb)), 'sha256'),'hex'),
             jsonb_set(observation_snapshot,'{rules}','[]'::jsonb),assessment_updated_at,
             bundle_ids,bundle_version_ids,requirement_version_ids,waiver_id,
             observed_at,'forged composite Pass'
           FROM poam_verification_items WHERE attempt_id=$2"#,
    )
    .bind(forged_attempt_id)
    .bind(legitimate_attempt_id)
    .execute(&mut *forged)
    .await
    .unwrap();
    sqlx::query("UPDATE poam_verification_attempts SET sealed_at=CURRENT_TIMESTAMP WHERE id=$1")
        .bind(forged_attempt_id)
        .execute(&mut *forged)
        .await
        .unwrap();
    sqlx::query("UPDATE poam_finding_links SET retired_at=CURRENT_TIMESTAMP,retired_by=$2,retirement_reason='closed:'||$3::uuid::text WHERE poam_id=$1 AND retired_at IS NULL")
        .bind(awaiting.poam.id)
        .bind(actor.user_id)
        .bind(forged_attempt_id)
        .execute(&mut *forged)
        .await
        .unwrap();
    sqlx::query("UPDATE poams SET status='completed',closed_at=CURRENT_TIMESTAMP,closure_attempt_id=$2 WHERE id=$1")
        .bind(awaiting.poam.id)
        .bind(forged_attempt_id)
        .execute(&mut *forged)
        .await
        .unwrap();
    let error = forged.commit().await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().constraint(),
        Some("poams_authoritative_closure_evidence")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_observation_rejects_changed_effective_config(pool: PgPool) {
    let (fixture, finding_id, observation, _) = legacy_fail_fixture(&pool).await;
    let changed = serde_json::json!({"changed_after_observation": true});
    sqlx::query("UPDATE deployment_policies SET config=$2 WHERE id=$1")
        .bind(fixture.resolved.policies[0].policy_lineage_id)
        .bind(&changed)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET config=$2 WHERE id=$1")
        .bind(fixture.version_id)
        .bind(&changed)
        .execute(&pool)
        .await
        .unwrap();
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 28, 12, 0, 0).unwrap());
    assert!(matches!(
        poam_service::create(
            &pool,
            &admin_actor(fixture.user_id),
            legacy_create_request(finding_id, observation),
            &clock,
        )
        .await,
        Err(PoamError::Precondition("stale_finding", _, _))
    ));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM poams")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn legacy_create_waits_for_newer_derivation_evidence(pool: PgPool) {
    let (fixture, finding_id, observation, _) = legacy_fail_fixture(&pool).await;
    let policy_lineage_id = fixture.resolved.policies[0].policy_lineage_id;
    let (commit_id, derivation_name, derivation_path): (Option<i32>, String, String) =
        sqlx::query_as(
            "SELECT commit_id,derivation_name,derivation_path FROM derivations WHERE id=$1",
        )
        .bind(fixture.derivation_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT lock_poam_finding_key($1,$2)")
        .bind(fixture.system_id)
        .bind(policy_lineage_id)
        .execute(&mut *blocker)
        .await
        .unwrap();

    let writer_pool = pool.clone();
    let version_id = fixture.version_id;
    let store_path = fixture.store_path.clone();
    let passing_result = serde_json::json!({
        "assigned": {
            version_id.to_string(): {
                "passed": true,
                "details": "legacy custom check now passes"
            }
        }
    });
    let writer = tokio::spawn(async move {
        record_successful_eval_result(
            &writer_pool,
            commit_id,
            &derivation_name,
            "nixos",
            None,
            &derivation_path,
            Some(&store_path),
            Some(true),
            true,
            &passing_result,
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !writer.is_finished(),
        "the evidence writer must wait for the finding key"
    );

    let action_pool = pool.clone();
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 28, 12, 0, 0).unwrap());
    let action = tokio::spawn(async move {
        poam_service::create(
            &action_pool,
            &actor,
            legacy_create_request(finding_id, observation),
            &clock,
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !action.is_finished(),
        "POA&M creation must wait behind the queued evidence writer"
    );

    blocker.commit().await.unwrap();
    writer.await.unwrap().unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), action)
        .await
        .expect("POA&M creation remained blocked")
        .unwrap();
    assert!(
        matches!(&result, Err(PoamError::Precondition("stale_finding", _, _))),
        "unexpected create result: {result:?}"
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM poams")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

async fn awaiting_verification(
    pool: &PgPool,
    actor: &PoamActor,
    detail: crystal_forge::models::poam::PoamDetail,
    clock: &FixedClock,
) -> crystal_forge::models::poam::PoamDetail {
    let progress = poam_service::transition(
        pool,
        actor,
        detail.poam.id,
        TransitionPoamRequest {
            revision: detail.poam.revision,
            status: PoamStatus::InProgress,
            note: None,
        },
        clock,
    )
    .await
    .unwrap();
    poam_service::transition(
        pool,
        actor,
        progress.poam.id,
        TransitionPoamRequest {
            revision: progress.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        clock,
    )
    .await
    .unwrap()
}

async fn immutable_assignment_fixture(
    pool: &PgPool,
    system_id: Uuid,
    created_by: Uuid,
) -> (Uuid, Uuid, Uuid) {
    let policy_version_id: Uuid = sqlx::query_scalar(
        r#"SELECT assessment.policy_version_id
           FROM composite_policy_assessments assessment
           WHERE assessment.system_id=$1
           ORDER BY assessment.updated_at DESC,assessment.id DESC LIMIT 1"#,
    )
    .bind(system_id)
    .fetch_one(pool)
    .await
    .unwrap();
    immutable_assignment_fixture_for_version(pool, system_id, created_by, policy_version_id).await
}

async fn immutable_assignment_fixture_for_version(
    pool: &PgPool,
    system_id: Uuid,
    created_by: Uuid,
    policy_version_id: Uuid,
) -> (Uuid, Uuid, Uuid) {
    let bundle_id = Uuid::new_v4();
    let bundle_version_id = Uuid::new_v4();
    sqlx::query("INSERT INTO compliance_bundles(id,name,framework,version,description,layer,owner) VALUES($1,$2,'NIST','1.0','POAM assignment fixture','fleet','Security')")
        .bind(bundle_id).bind(format!("poam-assignment-{bundle_id}")).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO compliance_bundle_versions(id,bundle_id,version,publication_state,name,framework,framework_version,description,layer,owner,semantic_digest,trust_state) VALUES($1,$2,'1.0','draft',$3,'NIST','1.0','Immutable fixture','fleet','Security','bundle-semantic-v1','trusted')")
        .bind(bundle_version_id).bind(bundle_id).bind(format!("POAM bundle {bundle_id}")).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO compliance_bundle_version_policies(bundle_version_id,policy_version_id,policy_order,selected) VALUES($1,$2,0,true)")
        .bind(bundle_version_id).bind(policy_version_id).execute(pool).await.unwrap();
    let assignment_id = Uuid::new_v4();
    let assignment_version_id = Uuid::new_v4();
    sqlx::query("INSERT INTO compliance_bundle_assignments(id,bundle_id,bundle_version_id,system_id,scope_type,active,enforcement_mode,assignment_overlay_digest,provenance,created_by) VALUES($1,$2,$3,$4,'system',false,'report_only','assignment-digest-v1',$5,$6)")
        .bind(assignment_id).bind(bundle_id).bind(bundle_version_id).bind(system_id)
        .bind(serde_json::json!({"source":"poam-test","content":{"exception":"documented"}})).bind(created_by)
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO compliance_bundle_assignment_versions(id,assignment_id,version_number,bundle_version_id,enforcement_mode,assignment_overlay_digest,provenance,created_by) VALUES($1,$2,1,$3,'report_only','assignment-digest-v1',$4,$5)")
        .bind(assignment_version_id).bind(assignment_id).bind(bundle_version_id)
        .bind(serde_json::json!({"source":"poam-test","content":{"exception":"documented"}})).bind(created_by)
        .execute(pool).await.unwrap();
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id=$2 WHERE id=$1")
        .bind(assignment_id)
        .bind(assignment_version_id)
        .execute(pool)
        .await
        .unwrap();
    (assignment_id, assignment_version_id, bundle_id)
}

async fn assignment_snapshot(pool: &PgPool, assignment_version_id: Uuid) -> serde_json::Value {
    sqlx::query_scalar(
        r#"SELECT jsonb_build_object(
      'version',to_jsonb(av),'assignment_id',av.assignment_id,'bundle_version',to_jsonb(bv),
      'publication_state',bv.publication_state,'bundle_semantic_digest',bv.semantic_digest,
      'effective_mode',av.enforcement_mode,'assignment_digest',av.assignment_overlay_digest)
      FROM compliance_bundle_assignment_versions av
      JOIN compliance_bundle_versions bv ON bv.id=av.bundle_version_id WHERE av.id=$1"#,
    )
    .bind(assignment_version_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn assessment_fixture(pool: &PgPool) -> AssessmentFixture {
    assessment_fixture_with_policy(pool, None).await
}

async fn assessment_fixture_for_policy(
    pool: &PgPool,
    source: &AssessmentFixture,
) -> AssessmentFixture {
    assessment_fixture_with_policy(
        pool,
        Some((
            source.resolved.policies[0].policy_lineage_id,
            source.version_id,
            source.config.clone(),
        )),
    )
    .await
}

async fn assessment_fixture_with_policy(
    pool: &PgPool,
    existing_policy: Option<(Uuid, Uuid, CompositePolicyConfig)>,
) -> AssessmentFixture {
    let suffix = Uuid::new_v4();
    let hostname = format!("poam-assessment-{suffix}");
    let repository = format!("https://example.invalid/{suffix}.git");
    let commit_hash = suffix.simple().to_string();
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users(username,first_name,last_name,email) VALUES($1,'POAM','Race',$2) RETURNING id",
    )
    .bind(format!("poam-race-{suffix}"))
    .bind(format!("poam-race-{suffix}@example.invalid"))
    .fetch_one(pool)
    .await
    .unwrap();
    let flake = insert_flake(
        pool,
        &format!("poam-race-{suffix}"),
        &repository,
        "main",
        "all_configs",
    )
    .await
    .unwrap();
    insert_commit(pool, &commit_hash, &repository, Utc::now())
        .await
        .unwrap();
    let commit_id: i32 =
        sqlx::query_scalar("SELECT id FROM commits WHERE flake_id=$1 AND git_commit_hash=$2")
            .bind(flake.id)
            .bind(&commit_hash)
            .fetch_one(pool)
            .await
            .unwrap();
    let system_id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,is_active,public_key,derivation,reachability,flake_id,system_configuration_name) VALUES($1,true,$2,$2,'direct',$3,$1) RETURNING id",
    )
    .bind(&hostname)
    .bind(format!("poam-race-key-{suffix}"))
    .bind(flake.id)
    .fetch_one(pool)
    .await
    .unwrap();
    let (policy_id, version_id, config) = if let Some(existing) = existing_policy {
        existing
    } else {
        let config = assessment_config();
        let policy = create_deployment_policy(
            pool,
            &CreateDeploymentPolicyRequest {
                name: format!("poam-race-policy-{suffix}"),
                policy_type: "composite".into(),
                config: serde_json::to_value(&config).unwrap(),
                enabled: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let version_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM deployment_policies WHERE id=$1",
        )
        .bind(policy.id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query("UPDATE deployment_policy_versions SET trust_state='trusted' WHERE id=$1")
            .bind(version_id)
            .execute(pool)
            .await
            .unwrap();
        (policy.id, version_id, config)
    };
    sqlx::query("INSERT INTO system_policies(system_id,policy_id) VALUES($1,$2)")
        .bind(system_id)
        .bind(policy_id)
        .execute(pool)
        .await
        .unwrap();
    let store_path = format!("/nix/store/{suffix}-poam-target");
    let write = record_successful_eval_result(
        pool,
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
    sqlx::query("UPDATE derivations SET store_path=$1,status_id=10 WHERE id=$2")
        .bind(&store_path)
        .bind(derivation_id)
        .execute(pool)
        .await
        .unwrap();
    let resolved = match resolve_system_effective_policies(pool, system_id)
        .await
        .unwrap()
    {
        ResolutionOutcome::Resolved(resolved) => resolved,
        ResolutionOutcome::Conflict(conflicts) => panic!("unexpected conflict: {conflicts:?}"),
    };
    insert_system_state(
        pool,
        &SystemState {
            id: None,
            hostname: hostname.clone(),
            change_reason: "cf_deployment".into(),
            timestamp: None,
            store_path: Some(store_path.clone()),
            generation: Some(1),
            generation_matches_current_store_path: Some(true),
            os: None,
            kernel: None,
            memory_gb: None,
            uptime_secs: None,
            cpu_brand: None,
            cpu_cores: None,
            board_serial: None,
            product_uuid: None,
            rootfs_uuid: None,
            chassis_serial: None,
            bios_version: None,
            cpu_microcode: None,
            network_interfaces: None,
            primary_mac_address: None,
            primary_ip_address: None,
            gateway_ip: None,
            selinux_status: None,
            tpm_present: None,
            secure_boot_enabled: None,
            fips_mode: None,
            agent_version: None,
            agent_build_hash: None,
            nixos_version: None,
            agent_compatible: Some(true),
            partial_data: Some(false),
            boot_id: None,
        },
        true,
        None,
        None,
    )
    .await
    .unwrap();
    AssessmentFixture {
        user_id,
        system_id,
        version_id,
        derivation_id,
        store_path,
        config,
        resolved,
    }
}

async fn deploy_store_path(pool: &PgPool, fixture: &AssessmentFixture, store_path: &str) {
    let hostname: String = sqlx::query_scalar("SELECT hostname FROM systems WHERE id=$1")
        .bind(fixture.system_id)
        .fetch_one(pool)
        .await
        .unwrap();
    insert_system_state(
        pool,
        &SystemState {
            id: None,
            hostname: hostname.clone(),
            change_reason: "cf_deployment".into(),
            timestamp: None,
            store_path: Some(store_path.into()),
            generation: Some(2),
            generation_matches_current_store_path: Some(true),
            os: None,
            kernel: None,
            memory_gb: None,
            uptime_secs: None,
            cpu_brand: None,
            cpu_cores: None,
            board_serial: None,
            product_uuid: None,
            rootfs_uuid: None,
            chassis_serial: None,
            bios_version: None,
            cpu_microcode: None,
            network_interfaces: None,
            primary_mac_address: None,
            primary_ip_address: None,
            gateway_ip: None,
            selinux_status: None,
            tpm_present: None,
            secure_boot_enabled: None,
            fips_mode: None,
            agent_version: None,
            agent_build_hash: None,
            nixos_version: None,
            agent_compatible: Some(true),
            partial_data: Some(false),
            boot_id: None,
        },
        true,
        None,
        None,
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE system_states SET timestamp=clock_timestamp()+INTERVAL '1 day' WHERE hostname=$1 AND store_path=$2",
    )
    .bind(hostname)
    .bind(store_path)
    .execute(pool)
    .await
    .unwrap();
}

async fn session(pool: &PgPool, user_id: Uuid, role: AuthRole) -> String {
    sync_user_role(pool, user_id, role).await.unwrap();
    let token = format!("poam-http-session-{}", Uuid::new_v4().simple());
    create_user_session(
        pool,
        user_id,
        hash_token(&token),
        Utc::now() + chrono::Duration::hours(1),
        Some("poam-http-test".into()),
        Some("127.0.0.1".into()),
        "local".into(),
    )
    .await
    .unwrap();
    token
}

async fn role_session(pool: &PgPool, role: AuthRole) -> (Uuid, String) {
    let suffix = Uuid::new_v4().simple();
    let user = insert_user(
        pool,
        &format!("poam-http-{suffix}@example.invalid"),
        Some("POAM HTTP Test"),
    )
    .await
    .unwrap();
    let token = session(pool, user.id, role).await;
    (user.id, token)
}

async fn poam_http_server(pool: PgPool) -> String {
    let state = CFState::new(
        pool,
        crystal_forge::config::ServerConfig::default(),
        Arc::new(QueueNotifier::new()),
        BackgroundJobRegistry::new(),
    );
    let app = Router::new()
        .route(
            "/api/v1/poams",
            get(poam_handlers::list).post(poam_handlers::create),
        )
        .route("/api/v1/poams/dashboard", get(poam_handlers::dashboard))
        .route(
            "/api/v1/poams/dashboard/watchlist",
            get(poam_handlers::watchlist),
        )
        .route(
            "/api/v1/poams/rollups/systems",
            get(poam_handlers::system_rollups),
        )
        .route(
            "/api/v1/poams/rollups/bundles",
            get(poam_handlers::bundle_rollups),
        )
        .route(
            "/api/v1/poams/relationships/findings",
            get(poam_handlers::finding_relationships),
        )
        .route(
            "/api/v1/poams/relationships/assignments",
            get(poam_handlers::assignment_relationships),
        )
        .route(
            "/api/v1/poams/compatible",
            get(poam_handlers::compatible_poams),
        )
        .route(
            "/api/v1/poams/:id",
            get(poam_handlers::get).patch(poam_handlers::update),
        )
        .route(
            "/api/v1/poams/:id/transition",
            axum::routing::post(poam_handlers::transition),
        )
        .route(
            "/api/v1/poams/:id/notes",
            axum::routing::post(poam_handlers::note),
        )
        .route(
            "/api/v1/poams/:id/milestones",
            axum::routing::post(poam_handlers::add_milestone),
        )
        .route(
            "/api/v1/poams/:id/milestones/:milestone_id",
            axum::routing::patch(poam_handlers::update_milestone)
                .delete(poam_handlers::remove_milestone),
        )
        .route(
            "/api/v1/poams/:id/findings",
            axum::routing::post(poam_handlers::link_finding),
        )
        .route(
            "/api/v1/poams/:id/findings/:finding_id",
            axum::routing::delete(poam_handlers::unlink_finding),
        )
        .route(
            "/api/v1/poams/:id/assignments",
            axum::routing::post(poam_handlers::link_assignment),
        )
        .route(
            "/api/v1/poams/:id/assignments/:assignment_version_id",
            axum::routing::delete(poam_handlers::unlink_assignment),
        )
        .route(
            "/api/v1/poams/:id/compatible",
            get(poam_handlers::compatible),
        )
        .route(
            "/api/v1/poams/:id/verify",
            axum::routing::post(poam_handlers::verify),
        )
        .route(
            "/api/v1/poams/:id/close",
            axum::routing::post(poam_handlers::close),
        )
        .route(
            "/api/v1/poams/:id/reopen",
            axum::routing::post(poam_handlers::reopen),
        )
        .route(
            "/api/v1/finding-waivers",
            axum::routing::post(poam_handlers::create_waiver),
        )
        .route(
            "/api/v1/finding-waivers/:id/status",
            axum::routing::post(poam_handlers::decide_waiver),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

fn http_request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: String,
    token: &str,
    csrf: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut request = client.request(method, url);
    if let Some(csrf) = csrf {
        request = request
            .header(
                "cookie",
                format!("{SESSION_COOKIE_NAME}={token}; {CSRF_COOKIE_NAME}={csrf}"),
            )
            .header(CSRF_HEADER_NAME.as_str(), csrf);
    } else {
        request = request.header("cookie", format!("{SESSION_COOKIE_NAME}={token}"));
    }
    request
}

async fn fixture(pool: &PgPool) -> Fixture {
    let suffix = Uuid::new_v4();
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users(username,first_name,last_name,email) VALUES($1,'POAM','Test',$2) RETURNING id",
    )
    .bind(format!("poam-{suffix}"))
    .bind(format!("poam-{suffix}@example.invalid"))
    .fetch_one(pool)
    .await
    .unwrap();
    let system_id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,public_key,derivation) VALUES($1,$2,$2) RETURNING id",
    )
    .bind(format!("poam-{suffix}"))
    .bind(format!("test-key-{suffix}"))
    .fetch_one(pool)
    .await
    .unwrap();
    let policy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies(name,policy_type,config,enabled) VALUES($1,'custom_check','{}',false) RETURNING id",
    )
    .bind(format!("poam-policy-{suffix}"))
    .fetch_one(pool)
    .await
    .unwrap();
    let finding_id: Uuid = sqlx::query_scalar(
        "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
    )
    .bind(system_id)
    .bind(policy_id)
    .fetch_one(pool)
    .await
    .unwrap();
    Fixture {
        user_id,
        system_id,
        policy_id,
        finding_id,
    }
}

async fn create_poam(
    tx: &mut Transaction<'_, Postgres>,
    fixture: &Fixture,
    title: &str,
    target_date: NaiveDate,
) -> (Uuid, i64) {
    let (poam_id, human_number): (Uuid, i64) = sqlx::query_as(
        "INSERT INTO poams(title,target_date,risk,created_by) VALUES($1,$2,'high',$3) RETURNING id,human_number",
    )
    .bind(title)
    .bind(target_date)
    .bind(fixture.user_id)
    .fetch_one(&mut **tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)")
        .bind(poam_id)
        .bind(fixture.finding_id)
        .bind(fixture.user_id)
        .execute(&mut **tx)
        .await
        .unwrap();
    (poam_id, human_number)
}

#[sqlx::test]
async fn schema_enforces_real_active_finding_and_immutable_history(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let mut invalid = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO poams(title,risk,created_by) VALUES('orphan','low',$1)")
        .bind(fixture.user_id)
        .execute(&mut *invalid)
        .await
        .unwrap();
    let error = invalid.commit().await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().constraint(),
        Some("poams_active_finding_required")
    );

    let mut tx = pool.begin().await.unwrap();
    let (poam_id, _) = create_poam(
        &mut tx,
        &fixture,
        "managed failure",
        NaiveDate::from_ymd_opt(2026, 8, 25).unwrap(),
    )
    .await;
    sqlx::query("INSERT INTO poam_activity(poam_id,actor_user_id,kind,payload) VALUES($1,$2,'created','{}')")
        .bind(poam_id)
        .bind(fixture.user_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert!(
        sqlx::query("UPDATE poam_activity SET payload='{}' WHERE poam_id=$1")
            .bind(poam_id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE poam_finding_links SET retired_at=NOW(),retired_by=$2,retirement_reason='bad' WHERE poam_id=$1")
            .bind(poam_id)
            .bind(fixture.user_id)
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test]
async fn concurrent_creates_use_unique_human_ids_and_one_active_link(pool: PgPool) {
    let left = fixture(&pool).await;
    let right = fixture(&pool).await;
    let today = NaiveDate::from_ymd_opt(2026, 8, 26).unwrap();
    let mut tx1 = pool.begin().await.unwrap();
    let mut tx2 = pool.begin().await.unwrap();
    let (_, human1) = create_poam(&mut tx1, &left, "left", today).await;
    let (_, human2) = create_poam(&mut tx2, &right, "right", today).await;
    tx1.commit().await.unwrap();
    tx2.commit().await.unwrap();
    assert_ne!(human1, human2);

    let shared_finding = left.finding_id;
    let left_poam: Uuid = sqlx::query_scalar(
        "SELECT poam_id FROM poam_finding_links WHERE finding_id=$1 AND retired_at IS NULL",
    )
    .bind(shared_finding)
    .fetch_one(&pool)
    .await
    .unwrap();
    let right_poam: Uuid = sqlx::query_scalar("SELECT id FROM poams WHERE title='right'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let conflict = sqlx::query(
        "INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)",
    )
    .bind(right_poam)
    .bind(shared_finding)
    .bind(right.user_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(
        conflict.as_database_error().unwrap().constraint(),
        Some("poam_finding_links_one_active_remediation")
    );
    assert_ne!(left_poam, right_poam);
}

#[sqlx::test]
async fn filters_dashboard_watchlist_and_overdue_use_strict_server_date(pool: PgPool) {
    let overdue = fixture(&pool).await;
    let due_today = fixture(&pool).await;
    let today = NaiveDate::from_ymd_opt(2026, 8, 26).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let (overdue_id, _) = create_poam(
        &mut tx,
        &overdue,
        "searchable overdue owner",
        today.pred_opt().unwrap(),
    )
    .await;
    let (today_id, _) = create_poam(&mut tx, &due_today, "due today", today).await;
    sqlx::query(
        "UPDATE poams SET owner='Security Team',status='awaiting_verification' WHERE id=$1",
    )
    .bind(today_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let listed = poam::list(
        &pool,
        &PoamListQuery {
            q: Some("searchable overdue".into()),
            overdue: Some(true),
            system_id: Some(overdue.system_id),
            policy_lineage_id: Some(overdue.policy_id),
            ..Default::default()
        },
        today,
        true,
        &[],
    )
    .await
    .unwrap();
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].id, overdue_id);
    assert!(listed.items[0].overdue);

    let mut detail_tx = pool.begin().await.unwrap();
    let detail = poam::detail(
        &mut detail_tx,
        today_id,
        today,
        true,
        &[],
        100,
        None,
        None,
        100,
        None,
        None,
        10,
        None,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    detail_tx.commit().await.unwrap();
    assert!(
        !detail.poam.overdue,
        "a target date equal to today is not overdue"
    );
    let dashboard = poam::dashboard(&pool, today, true, &[]).await.unwrap();
    assert_eq!(
        (dashboard.total, dashboard.active, dashboard.overdue),
        (2, 2, 1)
    );
    let watchlist = poam::watchlist(&pool, today, true, &[], 10, 0)
        .await
        .unwrap();
    assert_eq!(watchlist.items.len(), 2);
    assert_eq!(watchlist.items[0].id, overdue_id);
    let rollups = poam::system_rollups(
        &pool,
        &[overdue.system_id, due_today.system_id],
        today,
        true,
        &[],
    )
    .await
    .unwrap();
    assert_eq!(rollups.len(), 2);
    assert!(rollups.iter().all(|rollup| rollup.total == 1));
}

#[sqlx::test]
async fn list_filter_and_pagination_matrix_is_deterministic(pool: PgPool) {
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let fixtures = [
        assessment_fixture(&pool).await,
        assessment_fixture(&pool).await,
        assessment_fixture(&pool).await,
        assessment_fixture(&pool).await,
    ];
    let actor = admin_actor(fixtures[0].user_id);
    for fixture in &fixtures {
        let mut tx = pool.begin().await.unwrap();
        persist_assessment(&mut tx, fixture, EnforcementOutcome::Fail).await;
        tx.commit().await.unwrap();
    }
    let mut details = Vec::new();
    for (index, fixture) in fixtures.iter().enumerate() {
        details.push(
            create_service_poam(
                &pool,
                fixture,
                &actor,
                &clock,
                &format!("Matrix title {index}"),
            )
            .await,
        );
    }
    let owners = ["Alpha Owner", "Beta Owner", "Gamma Owner", "Delta Owner"];
    let plans = ["needle-plan", "second plan", "third plan", "fourth plan"];
    let risks = ["high", "medium", "low", "high"];
    let targets = [
        clock.today() - chrono::Duration::days(1),
        clock.today(),
        clock.today() + chrono::Duration::days(1),
        clock.today() - chrono::Duration::days(2),
    ];
    for index in 0..4 {
        sqlx::query("UPDATE poams SET owner=$2,plan=$3,risk=$4,target_date=$5 WHERE id=$1")
            .bind(details[index].poam.id)
            .bind(owners[index])
            .bind(plans[index])
            .bind(risks[index])
            .bind(targets[index])
            .execute(&pool)
            .await
            .unwrap();
    }
    details[1] = poam_service::transition(
        &pool,
        &actor,
        details[1].poam.id,
        TransitionPoamRequest {
            revision: details[1].poam.revision,
            status: PoamStatus::InProgress,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let fourth = details.pop().unwrap();
    let third = details.pop().unwrap();
    details.push(awaiting_verification(&pool, &actor, third, &clock).await);
    let completed_progress = awaiting_verification(&pool, &actor, fourth, &clock).await;
    let mut pass = pool.begin().await.unwrap();
    persist_assessment(&mut pass, &fixtures[3], EnforcementOutcome::Pass).await;
    pass.commit().await.unwrap();
    let completed = poam_service::close(
        &pool,
        &actor,
        completed_progress.poam.id,
        completed_progress.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    details.push(completed);

    let (assignment_id, assignment_version_id, bundle_id) =
        immutable_assignment_fixture(&pool, fixtures[0].system_id, fixtures[0].user_id).await;
    sqlx::query("INSERT INTO poam_assignment_references(poam_id,assignment_id,assignment_version_id,added_by) VALUES($1,$2,$3,$4)")
        .bind(details[0].poam.id)
        .bind(assignment_id)
        .bind(assignment_version_id)
        .bind(fixtures[0].user_id)
        .execute(&pool)
        .await
        .unwrap();

    let framework_id = Uuid::new_v4();
    let framework_version_id = Uuid::new_v4();
    let requirement_id = Uuid::new_v4();
    let requirement_version_id = Uuid::new_v4();
    sqlx::query("INSERT INTO compliance_frameworks(id,name,canonical_source_key) VALUES($1,$2,$3)")
        .bind(framework_id)
        .bind(format!("Filter framework {framework_id}"))
        .bind(format!("filter-{framework_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO compliance_framework_versions(id,framework_id,version,canonical_release_key,title) VALUES($1,$2,'1.0',$3,'Filter release')")
        .bind(framework_version_id).bind(framework_id).bind(format!("filter-release-{framework_id}")).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO compliance_requirements(id,framework_id,canonical_requirement_key) VALUES($1,$2,'FILTER-CONTROL')")
        .bind(requirement_id).bind(framework_id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO compliance_requirement_versions(id,requirement_id,framework_version_id,external_id,title,kind) VALUES($1,$2,$3,'AC-TEST-433','Distinct filter requirement','control')")
        .bind(requirement_version_id).bind(requirement_id).bind(framework_version_id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO policy_requirement_mappings(policy_version_id,requirement_version_id,relationship,coverage,provenance,trust_state) VALUES($1,$2,'implements','full','manual','trusted')")
        .bind(fixtures[1].version_id).bind(requirement_version_id).execute(&pool).await.unwrap();

    async fn only_id(pool: &PgPool, query: PoamListQuery, clock: &FixedClock) -> Uuid {
        let page = poam_service::list(pool, &admin_actor(Uuid::nil()), &query, clock)
            .await
            .unwrap();
        assert_eq!(
            page.items.len(),
            1,
            "query should identify exactly one POA&M: {query:?}"
        );
        page.items[0].id
    }
    let expected = [
        (
            PoamListQuery {
                status: Some("open".into()),
                ..Default::default()
            },
            details[0].poam.id,
        ),
        (
            PoamListQuery {
                status: Some("in_progress".into()),
                ..Default::default()
            },
            details[1].poam.id,
        ),
        (
            PoamListQuery {
                status: Some("awaiting_verification".into()),
                ..Default::default()
            },
            details[2].poam.id,
        ),
        (
            PoamListQuery {
                status: Some("completed".into()),
                ..Default::default()
            },
            details[3].poam.id,
        ),
        (
            PoamListQuery {
                risk: Some("medium".into()),
                ..Default::default()
            },
            details[1].poam.id,
        ),
        (
            PoamListQuery {
                owner: Some("gamma".into()),
                ..Default::default()
            },
            details[2].poam.id,
        ),
        (
            PoamListQuery {
                system_id: Some(fixtures[1].system_id),
                ..Default::default()
            },
            details[1].poam.id,
        ),
        (
            PoamListQuery {
                policy_lineage_id: Some(fixtures[2].resolved.policies[0].policy_lineage_id),
                ..Default::default()
            },
            details[2].poam.id,
        ),
        (
            PoamListQuery {
                bundle_id: Some(bundle_id),
                ..Default::default()
            },
            details[0].poam.id,
        ),
        (
            PoamListQuery {
                requirement: Some("AC-TEST-433".into()),
                ..Default::default()
            },
            details[1].poam.id,
        ),
        (
            PoamListQuery {
                q: Some("needle-plan".into()),
                ..Default::default()
            },
            details[0].poam.id,
        ),
        (
            PoamListQuery {
                q: Some(details[1].poam.human_id.clone()),
                ..Default::default()
            },
            details[1].poam.id,
        ),
        (
            PoamListQuery {
                q: Some("Matrix title 2".into()),
                ..Default::default()
            },
            details[2].poam.id,
        ),
    ];
    for (query, id) in expected {
        assert_eq!(only_id(&pool, query, &clock).await, id);
    }

    sqlx::query("UPDATE poams SET updated_at='2000-01-01T00:00:00Z' WHERE id=$1")
        .bind(details[0].poam.id)
        .execute(&pool)
        .await
        .unwrap();
    let contextual_page = poam_service::list(
        &pool,
        &actor,
        &PoamListQuery {
            bundle_id: Some(bundle_id),
            limit: Some(1),
            ..Default::default()
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(contextual_page.items[0].id, details[0].poam.id);
    assert!(!contextual_page.has_more);
    let overdue = poam::list(
        &pool,
        &PoamListQuery {
            overdue: Some(true),
            ..Default::default()
        },
        clock.today(),
        true,
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        overdue.items.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![details[0].poam.id]
    );

    let tie = Utc.with_ymd_and_hms(2026, 8, 26, 13, 0, 0).unwrap();
    sqlx::query("UPDATE poams SET updated_at=$1")
        .bind(tie)
        .execute(&pool)
        .await
        .unwrap();
    let mut expected_order = details
        .iter()
        .map(|detail| detail.poam.id)
        .collect::<Vec<_>>();
    expected_order.sort();
    let first = poam::list(
        &pool,
        &PoamListQuery {
            limit: Some(2),
            offset: Some(0),
            ..Default::default()
        },
        clock.today(),
        true,
        &[],
    )
    .await
    .unwrap();
    let second = poam::list(
        &pool,
        &PoamListQuery {
            limit: Some(2),
            offset: Some(2),
            ..Default::default()
        },
        clock.today(),
        true,
        &[],
    )
    .await
    .unwrap();
    assert_eq!(first.limit, 2);
    assert_eq!(second.offset, 2);
    assert_eq!(
        first
            .items
            .into_iter()
            .chain(second.items)
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        expected_order
    );

    let reopened = poam_service::reopen(
        &pool,
        &actor,
        details[3].poam.id,
        details[3].poam.revision,
        &clock,
    )
    .await
    .unwrap();
    assert!(reopened.poam.overdue);
    let reopened_overdue = poam::list(
        &pool,
        &PoamListQuery {
            overdue: Some(true),
            ..Default::default()
        },
        clock.today(),
        true,
        &[],
    )
    .await
    .unwrap();
    assert!(
        reopened_overdue
            .items
            .iter()
            .any(|item| item.id == reopened.poam.id)
    );
}

#[sqlx::test]
async fn every_linked_environment_must_be_visible_for_reads_and_mutations(pool: PgPool) {
    let visible = fixture(&pool).await;
    let hidden = fixture(&pool).await;
    let visible_environment: Uuid =
        sqlx::query_scalar("SELECT id FROM environments WHERE name='dev'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let hidden_environment: Uuid =
        sqlx::query_scalar("SELECT id FROM environments WHERE name='prod'")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(visible.system_id)
        .bind(visible_environment)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(hidden.system_id)
        .bind(hidden_environment)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(visible.user_id)
        .bind(visible_environment)
        .execute(&pool)
        .await
        .unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 8, 26).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let (poam_id, _) = create_poam(&mut tx, &visible, "partially hidden", today).await;
    sqlx::query("INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)")
        .bind(poam_id)
        .bind(hidden.finding_id)
        .bind(visible.user_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let actor = PoamActor {
        user_id: visible.user_id,
        identifier: "limited-operator".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: vec![visible_environment],
        request_origin: Some("test".into()),
    };
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());

    assert!(matches!(
        poam_service::detail(&pool, &actor, poam_id, &clock).await,
        Err(PoamError::NotFound)
    ));
    assert!(
        poam_service::list(&pool, &actor, &PoamListQuery::default(), &clock)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(matches!(
        poam_service::update(
            &pool,
            &actor,
            poam_id,
            UpdatePoamRequest {
                revision: 1,
                title: Some("must remain hidden".into()),
                ..Default::default()
            },
            &clock,
        )
        .await,
        Err(PoamError::NotFound)
    ));
    let rollups = poam_service::system_rollups(&pool, &actor, &[visible.system_id], &clock)
        .await
        .unwrap();
    assert_eq!(rollups.len(), 1);
    let rollup = &rollups[0];
    assert_eq!(rollup.scope_id, visible.system_id);
    assert_eq!(
        (
            rollup.total,
            rollup.active,
            rollup.overdue,
            rollup.awaiting_verification,
            rollup.completed,
            rollup.open_findings,
            rollup.on_poam_findings,
            rollup.no_poam_findings,
        ),
        (0, 0, 0, 0, 0, 0, 0, 0)
    );
}

#[sqlx::test]
async fn retired_hidden_finding_does_not_deny_current_poam_access(pool: PgPool) {
    let visible = fixture(&pool).await;
    let hidden = fixture(&pool).await;
    let visible_environment: Uuid =
        sqlx::query_scalar("SELECT id FROM environments WHERE name='dev'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let hidden_environment: Uuid =
        sqlx::query_scalar("SELECT id FROM environments WHERE name='prod'")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(visible.system_id)
        .bind(visible_environment)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(hidden.system_id)
        .bind(hidden_environment)
        .execute(&pool)
        .await
        .unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 8, 26).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let (poam_id, _) = create_poam(&mut tx, &visible, "retired hidden context", today).await;
    sqlx::query(
        "INSERT INTO poam_finding_links(poam_id,finding_id,linked_by)
         VALUES($1,$2,$3)",
    )
    .bind(poam_id)
    .bind(hidden.finding_id)
    .bind(visible.user_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE poam_finding_links
         SET retired_at=NOW(),retired_by=$3,retirement_reason='unlinked'
         WHERE poam_id=$1 AND finding_id=$2",
    )
    .bind(poam_id)
    .bind(hidden.finding_id)
    .bind(visible.user_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let actor = PoamActor {
        user_id: visible.user_id,
        identifier: "retired-hidden-operator".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: vec![visible_environment],
        request_origin: Some("test".into()),
    };
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());

    let detail = poam_service::detail(&pool, &actor, poam_id, &clock)
        .await
        .unwrap();
    assert_eq!(detail.poam.id, poam_id);
    assert_eq!(detail.findings.len(), 1);
    assert_eq!(detail.findings[0].id, visible.finding_id);
    let listed = poam_service::list(&pool, &actor, &PoamListQuery::default(), &clock)
        .await
        .unwrap();
    assert!(listed.items.iter().any(|item| item.id == poam_id));
}

#[sqlx::test]
async fn close_waits_for_and_rejects_a_superseding_failed_assessment(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let mut initial = pool.begin().await.unwrap();
    persist_assessment(&mut initial, &fixture, EnforcementOutcome::Fail).await;
    initial.commit().await.unwrap();
    let assessment_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM composite_policy_assessments WHERE system_id=$1 AND policy_version_id=$2",
    )
    .bind(fixture.system_id)
    .bind(fixture.version_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let actor = PoamActor {
        user_id: fixture.user_id,
        identifier: "poam-race-admin".into(),
        is_admin: true,
        can_mutate: true,
        environment_ids: Vec::new(),
        request_origin: Some("test".into()),
    };
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let created = poam_service::create(
        &pool,
        &actor,
        CreatePoamRequest {
            assessment_id: Some(assessment_id),
            finding_id: None,
            observation: None,
            title: "Race-safe remediation".into(),
            plan: "Deploy and verify".into(),
            owner: "Security".into(),
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: true,
            assignment_version_ids: Vec::new(),
        },
        &clock,
    )
    .await
    .unwrap();
    let in_progress = poam_service::transition(
        &pool,
        &actor,
        created.poam.id,
        TransitionPoamRequest {
            revision: created.poam.revision,
            status: PoamStatus::InProgress,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        created.poam.id,
        TransitionPoamRequest {
            revision: in_progress.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();

    let mut passing = pool.begin().await.unwrap();
    persist_assessment(&mut passing, &fixture, EnforcementOutcome::Pass).await;
    passing.commit().await.unwrap();

    let mut superseding_fail = pool.begin().await.unwrap();
    persist_assessment(&mut superseding_fail, &fixture, EnforcementOutcome::Fail).await;

    let close_pool = pool.clone();
    let close_actor = actor.clone();
    let close_clock = clock.clone();
    let poam_id = awaiting.poam.id;
    let revision = awaiting.poam.revision;
    let mut close_task = tokio::spawn(async move {
        poam_service::close(&close_pool, &close_actor, poam_id, revision, &close_clock).await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut close_task)
            .await
            .is_err(),
        "closure must wait for the authoritative assessment writer lock"
    );
    superseding_fail.commit().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), close_task)
        .await
        .expect("closure did not resume after assessment commit")
        .unwrap();
    assert!(
        matches!(
            &result,
            Err(PoamError::Precondition("closure_not_ready", _, _))
        ),
        "unexpected closure result: {result:?}"
    );
    let state: (String, String) = sqlx::query_as(
        "SELECT p.status,a.overall_outcome FROM poams p JOIN composite_policy_assessments a ON a.system_id=$2 AND a.policy_version_id=$3 WHERE p.id=$1",
    )
    .bind(awaiting.poam.id)
    .bind(fixture.system_id)
    .bind(fixture.version_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, ("awaiting_verification".into(), "fail".into()));
    let rejected: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_verification_attempts WHERE poam_id=$1 AND outcome='rejected'",
    )
    .bind(awaiting.poam.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rejected, 1);

    let verification_revision: i64 = sqlx::query_scalar("SELECT revision FROM poams WHERE id=$1")
        .bind(awaiting.poam.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut passing_for_verification = pool.begin().await.unwrap();
    persist_assessment(
        &mut passing_for_verification,
        &fixture,
        EnforcementOutcome::Pass,
    )
    .await;
    passing_for_verification.commit().await.unwrap();

    let mut superseding_verification_fail = pool.begin().await.unwrap();
    persist_assessment(
        &mut superseding_verification_fail,
        &fixture,
        EnforcementOutcome::Fail,
    )
    .await;
    let verify_pool = pool.clone();
    let verify_actor = actor.clone();
    let verify_clock = clock.clone();
    let mut verify_task = tokio::spawn(async move {
        poam_service::verify(
            &verify_pool,
            &verify_actor,
            poam_id,
            verification_revision,
            &verify_clock,
        )
        .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut verify_task)
            .await
            .is_err(),
        "verification must wait for the authoritative assessment writer lock"
    );
    superseding_verification_fail.commit().await.unwrap();
    let verification = tokio::time::timeout(Duration::from_secs(5), verify_task)
        .await
        .expect("verification did not resume after assessment commit")
        .unwrap()
        .unwrap();
    assert_eq!(verification["outcome"], "rejected");
    assert_eq!(verification["items"][0]["result"], "fail");

    let rejected: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_verification_attempts WHERE poam_id=$1 AND outcome='rejected'",
    )
    .bind(awaiting.poam.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rejected, 2);

    let mut remediated = pool.begin().await.unwrap();
    persist_assessment(&mut remediated, &fixture, EnforcementOutcome::Pass).await;
    remediated.commit().await.unwrap();
    let current_revision: i64 = sqlx::query_scalar("SELECT revision FROM poams WHERE id=$1")
        .bind(awaiting.poam.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let closed = poam_service::close(&pool, &actor, awaiting.poam.id, current_revision, &clock)
        .await
        .unwrap();
    assert_eq!(closed.poam.status, "completed");
    assert_eq!(closed.findings.len(), 1);
    let closed_state: (bool, bool) = sqlx::query_as(
        "SELECT closure_attempt_id IS NOT NULL,NOT EXISTS(SELECT 1 FROM poam_finding_links WHERE poam_id=$1 AND retired_at IS NULL) FROM poams WHERE id=$1",
    )
    .bind(closed.poam.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(closed_state, (true, true));
    assert_eq!(
        closed
            .verification_attempts
            .iter()
            .filter(|attempt| attempt.outcome == "accepted")
            .count(),
        1
    );

    let reopened =
        poam_service::reopen(&pool, &actor, closed.poam.id, closed.poam.revision, &clock)
            .await
            .unwrap();
    assert_eq!(reopened.poam.status, "in_progress");
    assert_eq!(reopened.findings.len(), 2);
    assert_eq!(
        reopened
            .findings
            .iter()
            .filter(|finding| finding.link_active)
            .count(),
        1
    );
    assert_eq!(
        reopened
            .findings
            .iter()
            .filter(|finding| !finding.link_active)
            .count(),
        1
    );
    let reopened_state: (bool, bool) = sqlx::query_as(
        "SELECT closure_attempt_id IS NULL,EXISTS(SELECT 1 FROM poam_finding_links WHERE poam_id=$1 AND retired_at IS NULL) FROM poams WHERE id=$1",
    )
    .bind(reopened.poam.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reopened_state, (true, true));
}

#[sqlx::test]
async fn close_rechecks_applicability_after_waiting_for_direct_policy_change(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let mut initial = pool.begin().await.unwrap();
    persist_assessment(&mut initial, &fixture, EnforcementOutcome::Fail).await;
    initial.commit().await.unwrap();
    let detail = create_service_poam(&pool, &fixture, &actor, &clock, "Applicability race").await;
    let awaiting = awaiting_verification(&pool, &actor, detail, &clock).await;
    let mut passing = pool.begin().await.unwrap();
    persist_assessment(&mut passing, &fixture, EnforcementOutcome::Pass).await;
    passing.commit().await.unwrap();

    let mut applicability = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM system_policies WHERE system_id=$1 AND policy_id=$2")
        .bind(fixture.system_id)
        .bind(fixture.resolved.policies[0].policy_lineage_id)
        .execute(&mut *applicability)
        .await
        .unwrap();
    let close_pool = pool.clone();
    let close_actor = actor.clone();
    let close_clock = clock.clone();
    let mut close_task = tokio::spawn(async move {
        poam_service::close(
            &close_pool,
            &close_actor,
            awaiting.poam.id,
            awaiting.poam.revision,
            &close_clock,
        )
        .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut close_task)
            .await
            .is_err(),
        "closure must wait for the applicability writer"
    );
    applicability.commit().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), close_task)
        .await
        .unwrap()
        .unwrap();
    let Err(PoamError::Precondition("closure_not_ready", _, Some(details))) = result else {
        panic!("unexpected closure result: {result:?}");
    };
    assert_eq!(details["items"][0]["result"], "stale");
    assert!(details["revision"].as_i64().unwrap() > awaiting.poam.revision);
}

#[sqlx::test]
async fn close_cannot_observe_rule_result_before_aggregate_writer_commit(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let mut initial = pool.begin().await.unwrap();
    persist_assessment(&mut initial, &fixture, EnforcementOutcome::Fail).await;
    initial.commit().await.unwrap();
    let detail = create_service_poam(&pool, &fixture, &actor, &clock, "Mid-write race").await;
    let awaiting = awaiting_verification(&pool, &actor, detail, &clock).await;
    let mut passing = pool.begin().await.unwrap();
    persist_assessment(&mut passing, &fixture, EnforcementOutcome::Pass).await;
    passing.commit().await.unwrap();
    let assessment_id = current_assessment_id(&pool, &fixture).await;

    let mut writer = pool.begin().await.unwrap();
    sqlx::query("SELECT lock_poam_finding_key($1,$2)")
        .bind(fixture.system_id)
        .bind(fixture.resolved.policies[0].policy_lineage_id)
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query("UPDATE composite_policy_rule_results SET outcome='fail',blocking=true,detail='paused writer' WHERE assessment_id=$1")
        .bind(assessment_id).execute(&mut *writer).await.unwrap();
    let close_pool = pool.clone();
    let close_actor = actor.clone();
    let close_clock = clock.clone();
    let mut close_task = tokio::spawn(async move {
        poam_service::close(
            &close_pool,
            &close_actor,
            awaiting.poam.id,
            awaiting.poam.revision,
            &close_clock,
        )
        .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut close_task)
            .await
            .is_err()
    );
    sqlx::query("UPDATE composite_policy_assessments SET overall_outcome='fail',updated_at=NOW() WHERE id=$1")
        .bind(assessment_id).execute(&mut *writer).await.unwrap();
    writer.commit().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), close_task)
        .await
        .unwrap()
        .unwrap();
    let Err(PoamError::Precondition("closure_not_ready", _, Some(details))) = result else {
        panic!("unexpected closure result: {result:?}");
    };
    assert_eq!(details["items"][0]["result"], "fail");
}

#[sqlx::test]
async fn elapsed_waiver_replacement_and_verification_snapshot_cleanup_are_exact(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let mut initial = pool.begin().await.unwrap();
    persist_assessment(&mut initial, &fixture, EnforcementOutcome::Fail).await;
    initial.commit().await.unwrap();
    let assessment_id = current_assessment_id(&pool, &fixture).await;
    let finding_id = finding_id(&pool, &fixture).await;
    let first = poam_service::create_waiver(
        &pool,
        &actor,
        CreateWaiverRequest {
            finding_id,
            assessment_id,
            justification: "First bounded waiver".into(),
        },
    )
    .await
    .unwrap();
    let second = poam_service::create_waiver(
        &pool,
        &actor,
        CreateWaiverRequest {
            finding_id,
            assessment_id,
            justification: "Replacement waiver".into(),
        },
    )
    .await
    .unwrap();
    let first_id = Uuid::parse_str(first["waiver_id"].as_str().unwrap()).unwrap();
    let second_id = Uuid::parse_str(second["waiver_id"].as_str().unwrap()).unwrap();
    poam_service::decide_waiver(
        &pool,
        &actor,
        first_id,
        WaiverDecisionRequest {
            status: WaiverDecision::Accepted,
            expires_at: Some(clock.now() + chrono::Duration::minutes(1)),
        },
        &clock,
    )
    .await
    .unwrap();
    let later = FixedClock(clock.now() + chrono::Duration::minutes(2));
    poam_service::decide_waiver(
        &pool,
        &actor,
        second_id,
        WaiverDecisionRequest {
            status: WaiverDecision::Accepted,
            expires_at: None,
        },
        &later,
    )
    .await
    .unwrap();
    let waiver_states: Vec<(Uuid,String,bool,bool)> = sqlx::query_as(
        "SELECT id,status,accepted_by IS NULL,accepted_at IS NULL FROM finding_waivers WHERE id=ANY($1) ORDER BY id")
        .bind(&[first_id,second_id]).fetch_all(&pool).await.unwrap();
    assert!(
        waiver_states
            .iter()
            .any(|row| row.0 == first_id && row.1 == "expired" && !row.2 && !row.3)
    );
    assert!(
        waiver_states
            .iter()
            .any(|row| row.0 == second_id && row.1 == "accepted" && !row.2 && !row.3)
    );

    let detail = create_service_poam(
        &pool,
        &fixture,
        &actor,
        &later,
        "Immutable cleanup snapshot",
    )
    .await;
    let awaiting = awaiting_verification(&pool, &actor, detail, &later).await;
    let closed = poam_service::close(
        &pool,
        &actor,
        awaiting.poam.id,
        awaiting.poam.revision,
        &later,
    )
    .await
    .unwrap();
    let closure_id = closed.poam.closure_attempt_id.unwrap();
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(item) FROM poam_verification_items item WHERE attempt_id=$1",
    )
    .bind(closure_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM composite_policy_assessments WHERE derivation_id=$1")
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM derivations WHERE id=$1")
        .bind(fixture.derivation_id)
        .execute(&pool)
        .await
        .unwrap();
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(item) FROM poam_verification_items item WHERE attempt_id=$1",
    )
    .bind(closure_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, before);
    let closed_detail = poam_service::detail(&pool, &actor, closed.poam.id, &later)
        .await
        .unwrap();
    assert_eq!(
        closed_detail.findings[0].current_assessment_id,
        Some(assessment_id)
    );
}

#[sqlx::test]
async fn history_and_batch_inputs_are_bounded_with_continuation(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let mut tx = pool.begin().await.unwrap();
    let (poam_id, _) = create_poam(&mut tx, &fixture, "History paging", clock.today()).await;
    for index in 0..3 {
        sqlx::query(
            "INSERT INTO poam_activity(poam_id,actor_user_id,kind,payload) VALUES($1,$2,'note',$3)",
        )
        .bind(poam_id)
        .bind(fixture.user_id)
        .bind(serde_json::json!({"index":index}))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let first = poam_service::detail_with_history(
        &pool,
        &actor,
        poam_id,
        &PoamDetailQuery {
            activity_limit: Some(2),
            ..Default::default()
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(first.activity.len(), 2);
    assert!(first.activity_has_more);
    let cursor = first.activity_next_cursor.as_ref().unwrap();
    sqlx::query(
        "INSERT INTO poam_activity(poam_id,actor_user_id,kind,payload) VALUES($1,$2,'note','{\"after_snapshot\":true}')",
    )
    .bind(poam_id)
    .bind(fixture.user_id)
    .execute(&pool)
    .await
    .unwrap();
    let second = poam_service::detail_with_history(
        &pool,
        &actor,
        poam_id,
        &PoamDetailQuery {
            activity_limit: Some(2),
            activity_before_at: Some(cursor.at),
            activity_before_id: Some(cursor.id),
            ..Default::default()
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(second.activity.len(), 1);
    assert!(!second.activity_has_more);
    assert!(matches!(
        poam_service::list(
            &pool,
            &actor,
            &PoamListQuery {
                limit: Some(101),
                ..Default::default()
            },
            &clock
        )
        .await,
        Err(PoamError::Validation("invalid_limit", _))
    ));
    assert!(matches!(
        poam_service::system_rollups(&pool, &actor, &[], &clock).await,
        Err(PoamError::Validation("invalid_batch_size", _))
    ));
}

#[sqlx::test]
async fn close_waits_for_an_uncommitted_waiver_revocation(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let mut initial = pool.begin().await.unwrap();
    persist_assessment(&mut initial, &fixture, EnforcementOutcome::Fail).await;
    initial.commit().await.unwrap();
    let assessment_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM composite_policy_assessments WHERE system_id=$1 AND policy_version_id=$2",
    )
    .bind(fixture.system_id)
    .bind(fixture.version_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let finding_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM poam_findings WHERE system_id=$1 AND policy_lineage_id=$2",
    )
    .bind(fixture.system_id)
    .bind(fixture.resolved.policies[0].policy_lineage_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let actor = PoamActor {
        user_id: fixture.user_id,
        identifier: "poam-waiver-admin".into(),
        is_admin: true,
        can_mutate: true,
        environment_ids: Vec::new(),
        request_origin: Some("test".into()),
    };
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let created = poam_service::create(
        &pool,
        &actor,
        CreatePoamRequest {
            assessment_id: Some(assessment_id),
            finding_id: None,
            observation: None,
            title: "Waiver race remediation".into(),
            plan: "Validate waiver serialization".into(),
            owner: "Security".into(),
            target_date: None,
            risk: PoamRisk::Medium,
            default_milestones: false,
            assignment_version_ids: Vec::new(),
        },
        &clock,
    )
    .await
    .unwrap();
    let in_progress = poam_service::transition(
        &pool,
        &actor,
        created.poam.id,
        TransitionPoamRequest {
            revision: created.poam.revision,
            status: PoamStatus::InProgress,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        created.poam.id,
        TransitionPoamRequest {
            revision: in_progress.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let waiver = poam_service::create_waiver(
        &pool,
        &actor,
        CreateWaiverRequest {
            finding_id,
            assessment_id,
            justification: "Risk accepted for a bounded interval".into(),
        },
    )
    .await
    .unwrap();
    let waiver_id = Uuid::parse_str(waiver["waiver_id"].as_str().unwrap()).unwrap();
    poam_service::decide_waiver(
        &pool,
        &actor,
        waiver_id,
        WaiverDecisionRequest {
            status: WaiverDecision::Accepted,
            expires_at: Some(clock.now() + chrono::Duration::days(1)),
        },
        &clock,
    )
    .await
    .unwrap();

    let mut revocation = pool.begin().await.unwrap();
    sqlx::query("UPDATE finding_waivers SET status='revoked' WHERE id=$1")
        .bind(waiver_id)
        .execute(&mut *revocation)
        .await
        .unwrap();
    let close_pool = pool.clone();
    let close_actor = actor.clone();
    let close_clock = clock.clone();
    let mut close_task = tokio::spawn(async move {
        poam_service::close(
            &close_pool,
            &close_actor,
            awaiting.poam.id,
            awaiting.poam.revision,
            &close_clock,
        )
        .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut close_task)
            .await
            .is_err(),
        "closure must wait for the waiver writer lock"
    );
    revocation.commit().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), close_task)
        .await
        .expect("closure did not resume after waiver revocation")
        .unwrap();
    assert!(
        matches!(
            &result,
            Err(PoamError::Precondition("closure_not_ready", _, _))
        ),
        "unexpected closure result: {result:?}"
    );
    let state: (String, String) = sqlx::query_as(
        "SELECT p.status,w.status FROM poams p JOIN finding_waivers w ON w.id=$2 WHERE p.id=$1",
    )
    .bind(created.poam.id)
    .bind(waiver_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, ("awaiting_verification".into(), "revoked".into()));
}

#[sqlx::test]
async fn waiver_and_closure_evidence_matrix_is_exact_and_fail_closed(pool: PgPool) {
    let primary = assessment_fixture(&pool).await;
    let secondary = assessment_fixture_for_policy(&pool, &primary).await;
    let unrelated = assessment_fixture(&pool).await;
    let actor = admin_actor(primary.user_id);
    let dev: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='dev'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(primary.system_id)
        .bind(dev)
        .execute(&pool)
        .await
        .unwrap();
    let operator = PoamActor {
        is_admin: false,
        environment_ids: vec![dev],
        ..actor.clone()
    };
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let mut initial = pool.begin().await.unwrap();
    persist_assessment(&mut initial, &primary, EnforcementOutcome::Fail).await;
    persist_assessment(&mut initial, &secondary, EnforcementOutcome::Fail).await;
    persist_assessment(&mut initial, &unrelated, EnforcementOutcome::Fail).await;
    initial.commit().await.unwrap();
    let primary_finding = finding_id(&pool, &primary).await;
    let secondary_finding = finding_id(&pool, &secondary).await;
    let mut detail = create_service_poam(&pool, &primary, &actor, &clock, "Closure matrix").await;

    assert!(matches!(
        poam_service::verify(&pool, &actor, detail.poam.id, detail.poam.revision, &clock).await,
        Err(PoamError::Conflict("invalid_transition", _))
    ));
    assert!(matches!(
        poam_service::close(&pool, &actor, detail.poam.id, detail.poam.revision, &clock).await,
        Err(PoamError::Conflict("invalid_transition", _))
    ));
    assert!(matches!(
        poam_service::unlink_finding(
            &pool,
            &actor,
            detail.poam.id,
            primary_finding,
            detail.poam.revision,
            &clock,
        )
        .await,
        Err(PoamError::Validation("finding_required", _))
    ));
    detail = awaiting_verification(&pool, &actor, detail, &clock).await;
    assert!(matches!(
        poam_service::close(
            &pool,
            &actor,
            detail.poam.id,
            detail.poam.revision - 1,
            &clock,
        )
        .await,
        Err(PoamError::Conflict("stale_revision", _))
    ));

    for (outcome, expected) in [
        (EnforcementOutcome::Error, "error"),
        (EnforcementOutcome::NotChecked, "not_checked"),
    ] {
        let mut tx = pool.begin().await.unwrap();
        persist_assessment(&mut tx, &primary, outcome).await;
        tx.commit().await.unwrap();
        let result =
            poam_service::verify(&pool, &actor, detail.poam.id, detail.poam.revision, &clock)
                .await
                .unwrap();
        assert_eq!(result["outcome"], "rejected");
        assert_eq!(result["items"][0]["result"], expected);
        detail.poam.revision = result["revision"].as_i64().unwrap();
    }

    let unassessed_store_path = format!("{}-unassessed", primary.store_path);
    deploy_store_path(&pool, &primary, &unassessed_store_path).await;
    let missing = poam_service::verify(&pool, &actor, detail.poam.id, detail.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(missing["outcome"], "rejected");
    assert_eq!(missing["items"][0]["result"], "missing");
    detail.poam.revision = missing["revision"].as_i64().unwrap();
    deploy_store_path(&pool, &primary, &primary.store_path).await;
    let mut restored = pool.begin().await.unwrap();
    persist_assessment(&mut restored, &primary, EnforcementOutcome::Fail).await;
    restored.commit().await.unwrap();
    let primary_assessment = current_assessment_id(&pool, &primary).await;
    let waiver_evidence_systems = [primary.system_id, secondary.system_id, unrelated.system_id];
    let waiver_evidence = assessment_evidence_snapshot(&pool, &waiver_evidence_systems).await;
    let primary_fail_evidence = assessment_evidence_snapshot(&pool, &[primary.system_id]).await;

    let pending = poam_service::create_waiver(
        &pool,
        &operator,
        CreateWaiverRequest {
            finding_id: primary_finding,
            assessment_id: primary_assessment,
            justification: "Pending matrix waiver".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        assessment_evidence_snapshot(&pool, &waiver_evidence_systems).await,
        waiver_evidence
    );
    let pending_id = Uuid::parse_str(pending["waiver_id"].as_str().unwrap()).unwrap();
    assert!(matches!(
        poam_service::decide_waiver(
            &pool,
            &operator,
            pending_id,
            WaiverDecisionRequest {
                status: WaiverDecision::Accepted,
                expires_at: None,
            },
            &clock,
        )
        .await,
        Err(PoamError::Forbidden)
    ));
    assert_eq!(
        assessment_evidence_snapshot(&pool, &waiver_evidence_systems).await,
        waiver_evidence
    );
    let pending_verify =
        poam_service::verify(&pool, &actor, detail.poam.id, detail.poam.revision, &clock)
            .await
            .unwrap();
    assert_eq!(
        assessment_evidence_snapshot(&pool, &waiver_evidence_systems).await,
        waiver_evidence
    );
    assert_eq!(pending_verify["items"][0]["result"], "fail");
    detail.poam.revision = pending_verify["revision"].as_i64().unwrap();
    poam_service::decide_waiver(
        &pool,
        &actor,
        pending_id,
        WaiverDecisionRequest {
            status: WaiverDecision::Rejected,
            expires_at: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        assessment_evidence_snapshot(&pool, &waiver_evidence_systems).await,
        waiver_evidence
    );
    let rejected_verify =
        poam_service::verify(&pool, &actor, detail.poam.id, detail.poam.revision, &clock)
            .await
            .unwrap();
    assert_eq!(rejected_verify["items"][0]["result"], "fail");
    detail.poam.revision = rejected_verify["revision"].as_i64().unwrap();

    assert!(matches!(
        poam_service::create_waiver(
            &pool,
            &operator,
            CreateWaiverRequest {
                finding_id: secondary_finding,
                assessment_id: primary_assessment,
                justification: "Wrong finding".into(),
            },
        )
        .await,
        Err(PoamError::NotFound)
    ));

    let expiring = poam_service::create_waiver(
        &pool,
        &operator,
        CreateWaiverRequest {
            finding_id: primary_finding,
            assessment_id: primary_assessment,
            justification: "Time expiry".into(),
        },
    )
    .await
    .unwrap();
    let expiring_id = Uuid::parse_str(expiring["waiver_id"].as_str().unwrap()).unwrap();
    poam_service::decide_waiver(
        &pool,
        &actor,
        expiring_id,
        WaiverDecisionRequest {
            status: WaiverDecision::Accepted,
            expires_at: Some(clock.now() + chrono::Duration::minutes(1)),
        },
        &clock,
    )
    .await
    .unwrap();
    let later = FixedClock(clock.now() + chrono::Duration::minutes(2));
    let timed_out =
        poam_service::verify(&pool, &actor, detail.poam.id, detail.poam.revision, &later)
            .await
            .unwrap();
    assert_eq!(timed_out["items"][0]["result"], "fail");
    detail.poam.revision = timed_out["revision"].as_i64().unwrap();
    poam_service::decide_waiver(
        &pool,
        &actor,
        expiring_id,
        WaiverDecisionRequest {
            status: WaiverDecision::Expired,
            expires_at: None,
        },
        &later,
    )
    .await
    .unwrap();
    let explicitly_expired =
        poam_service::verify(&pool, &actor, detail.poam.id, detail.poam.revision, &later)
            .await
            .unwrap();
    assert_eq!(explicitly_expired["items"][0]["result"], "fail");
    detail.poam.revision = explicitly_expired["revision"].as_i64().unwrap();

    let revoked = poam_service::create_waiver(
        &pool,
        &operator,
        CreateWaiverRequest {
            finding_id: primary_finding,
            assessment_id: primary_assessment,
            justification: "Revocation".into(),
        },
    )
    .await
    .unwrap();
    let revoked_id = Uuid::parse_str(revoked["waiver_id"].as_str().unwrap()).unwrap();
    for status in [WaiverDecision::Accepted, WaiverDecision::Revoked] {
        poam_service::decide_waiver(
            &pool,
            &actor,
            revoked_id,
            WaiverDecisionRequest {
                status,
                expires_at: None,
            },
            &clock,
        )
        .await
        .unwrap();
    }
    let revoked_verify =
        poam_service::verify(&pool, &actor, detail.poam.id, detail.poam.revision, &clock)
            .await
            .unwrap();
    assert_eq!(revoked_verify["items"][0]["result"], "fail");
    detail.poam.revision = revoked_verify["revision"].as_i64().unwrap();

    let secondary_assessment = current_assessment_id(&pool, &secondary).await;
    for (label, finding, assessment, policy_version) in [
        (
            "wrong-finding",
            secondary_finding,
            primary_assessment,
            primary.version_id,
        ),
        (
            "wrong-assessment",
            primary_finding,
            secondary_assessment,
            primary.version_id,
        ),
        (
            "wrong-policy-version",
            primary_finding,
            primary_assessment,
            unrelated.version_id,
        ),
    ] {
        let waiver_id = Uuid::new_v4();
        let error = sqlx::query("INSERT INTO finding_waivers(id,finding_id,status,justification,policy_version_id,assessment_id,observation_token,observation_snapshot,accepted_by,accepted_at,created_by) VALUES($1,$2,'accepted',$3,$4,$5,'intentionally-wrong-observation','{}'::jsonb,$6,$7,$6)")
            .bind(waiver_id).bind(finding).bind(label).bind(policy_version).bind(assessment)
            .bind(actor.user_id).bind(clock.now()).execute(&pool).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().constraint(),
            Some("finding_waiver_initial_state")
        );
        let result =
            poam_service::verify(&pool, &actor, detail.poam.id, detail.poam.revision, &clock)
                .await
                .unwrap();
        assert_eq!(result["outcome"], "rejected", "{label}");
        assert_eq!(result["items"][0]["result"], "fail", "{label}");
        detail.poam.revision = result["revision"].as_i64().unwrap();
    }

    let exact = poam_service::create_waiver(
        &pool,
        &operator,
        CreateWaiverRequest {
            finding_id: primary_finding,
            assessment_id: primary_assessment,
            justification: "Exact accepted context".into(),
        },
    )
    .await
    .unwrap();
    let exact_id = Uuid::parse_str(exact["waiver_id"].as_str().unwrap()).unwrap();
    poam_service::decide_waiver(
        &pool,
        &actor,
        exact_id,
        WaiverDecisionRequest {
            status: WaiverDecision::Accepted,
            expires_at: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let linked = poam_service::link_finding(
        &pool,
        &actor,
        detail.poam.id,
        AddFindingRequest {
            revision: detail.poam.revision,
            assessment_id: Some(secondary_assessment),
            finding_id: None,
            observation: None,
        },
        &clock,
    )
    .await
    .unwrap();
    detail = linked;
    let mut secondary_pass = pool.begin().await.unwrap();
    persist_assessment(&mut secondary_pass, &secondary, EnforcementOutcome::Pass).await;
    secondary_pass.commit().await.unwrap();
    assert_eq!(
        assessment_evidence_snapshot(&pool, &[primary.system_id]).await,
        primary_fail_evidence
    );
    let accepted =
        poam_service::verify(&pool, &actor, detail.poam.id, detail.poam.revision, &clock)
            .await
            .unwrap();
    assert_eq!(accepted["outcome"], "accepted");
    let items = accepted["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|item| item["result"] == "pass"));
    assert!(
        items.iter().any(|item| {
            item["result"] == "waiver" && item["waiver_id"] == exact_id.to_string()
        })
    );
    assert_eq!(
        assessment_evidence_snapshot(&pool, &[primary.system_id]).await,
        primary_fail_evidence
    );
    detail.poam.revision = accepted["revision"].as_i64().unwrap();
    let failed_attempts_before_close: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM poam_verification_attempts WHERE poam_id=$1 AND outcome='rejected' ORDER BY id",
    )
    .bind(detail.poam.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(failed_attempts_before_close.len() >= 10);
    let closed = poam_service::close(&pool, &actor, detail.poam.id, detail.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(closed.poam.status, "completed");
    assert_eq!(
        assessment_evidence_snapshot(&pool, &[primary.system_id]).await,
        primary_fail_evidence
    );
    assert!(matches!(
        poam_service::reopen(
            &pool,
            &actor,
            closed.poam.id,
            closed.poam.revision - 1,
            &clock,
        )
        .await,
        Err(PoamError::Conflict("stale_revision", _))
    ));
    let reopened =
        poam_service::reopen(&pool, &actor, closed.poam.id, closed.poam.revision, &clock)
            .await
            .unwrap();
    assert_eq!(
        assessment_evidence_snapshot(&pool, &[primary.system_id]).await,
        primary_fail_evidence
    );
    let failed_after_reopen: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM poam_verification_attempts WHERE poam_id=$1 AND outcome='rejected' ORDER BY id",
    )
    .bind(reopened.poam.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(failed_after_reopen, failed_attempts_before_close);

    let awaiting = poam_service::transition(
        &pool,
        &actor,
        reopened.poam.id,
        TransitionPoamRequest {
            revision: reopened.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let closed_again = poam_service::close(
        &pool,
        &actor,
        awaiting.poam.id,
        awaiting.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        assessment_evidence_snapshot(&pool, &[primary.system_id]).await,
        primary_fail_evidence
    );
    let replacement = create_service_poam(&pool, &primary, &actor, &clock, "New remediation").await;
    assert_ne!(replacement.poam.id, closed_again.poam.id);
    assert!(matches!(
        poam_service::reopen(
            &pool,
            &actor,
            closed_again.poam.id,
            closed_again.poam.revision,
            &clock,
        )
        .await,
        Err(PoamError::Conflict("finding_already_managed", _))
    ));
    assert_eq!(
        assessment_evidence_snapshot(&pool, &[primary.system_id]).await,
        primary_fail_evidence
    );
}

#[sqlx::test]
async fn authenticated_http_routes_cover_authorization_and_lifecycle(pool: PgPool) {
    let primary = assessment_fixture(&pool).await;
    let compatible = assessment_fixture_for_policy(&pool, &primary).await;
    let hidden = assessment_fixture(&pool).await;
    for fixture in [&primary, &compatible, &hidden] {
        let mut tx = pool.begin().await.unwrap();
        persist_assessment(&mut tx, fixture, EnforcementOutcome::Fail).await;
        tx.commit().await.unwrap();
    }
    let primary_assessment = current_assessment_id(&pool, &primary).await;
    let compatible_assessment = current_assessment_id(&pool, &compatible).await;
    let hidden_assessment = current_assessment_id(&pool, &hidden).await;
    let evidence_systems = [primary.system_id, compatible.system_id, hidden.system_id];
    let mut expected_evidence = assessment_evidence_snapshot(&pool, &evidence_systems).await;
    macro_rules! assert_evidence_unchanged {
        () => {
            assert_eq!(
                assessment_evidence_snapshot(&pool, &evidence_systems).await,
                expected_evidence
            )
        };
    }
    let dev: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='dev'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let prod: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='prod'")
        .fetch_one(&pool)
        .await
        .unwrap();
    for (system_id, environment_id) in [
        (primary.system_id, dev),
        (compatible.system_id, prod),
        (hidden.system_id, prod),
    ] {
        sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
            .bind(system_id)
            .bind(environment_id)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(primary.user_id)
        .bind(dev)
        .execute(&pool)
        .await
        .unwrap();
    let operator = session(&pool, primary.user_id, AuthRole::Operator).await;
    let (viewer_id, viewer) = role_session(&pool, AuthRole::Viewer).await;
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(viewer_id)
        .bind(dev)
        .execute(&pool)
        .await
        .unwrap();
    let (admin_id, admin) = role_session(&pool, AuthRole::Admin).await;
    let base = poam_http_server(pool.clone()).await;
    let client = reqwest::Client::new();
    let csrf = "poam-http-csrf";
    let create_body = serde_json::json!({
        "assessment_id": primary_assessment,
        "title": "HTTP lifecycle",
        "plan": "Apply and validate the remediation",
        "owner": "platform",
        "risk": "high",
        "default_milestones": true
    });

    assert_eq!(
        client
            .get(format!("{base}/api/v1/poams"))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(format!("{base}/api/v1/poams"))
            .json(&create_body)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let no_csrf = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams"),
        &operator,
        None,
    )
    .json(&create_body)
    .send()
    .await
    .unwrap();
    assert_eq!(no_csrf.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        no_csrf.json::<serde_json::Value>().await.unwrap()["error"],
        "csrf_validation_failed"
    );
    let no_csrf_header = client
        .post(format!("{base}/api/v1/poams"))
        .header(
            "cookie",
            format!("{SESSION_COOKIE_NAME}={operator}; {CSRF_COOKIE_NAME}={csrf}"),
        )
        .json(&create_body)
        .send()
        .await
        .unwrap();
    assert_eq!(no_csrf_header.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        no_csrf_header.json::<serde_json::Value>().await.unwrap()["error"],
        "csrf_validation_failed"
    );
    let mismatch = client
        .post(format!("{base}/api/v1/poams"))
        .header(
            "cookie",
            format!("{SESSION_COOKIE_NAME}={operator}; {CSRF_COOKIE_NAME}={csrf}"),
        )
        .header(CSRF_HEADER_NAME.as_str(), "wrong")
        .json(&create_body)
        .send()
        .await
        .unwrap();
    assert_eq!(mismatch.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        mismatch.json::<serde_json::Value>().await.unwrap()["error"],
        "csrf_validation_failed"
    );
    let denied = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams"),
        &viewer,
        Some(csrf),
    )
    .json(&create_body)
    .send()
    .await
    .unwrap();
    assert_eq!(denied.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        denied.json::<serde_json::Value>().await.unwrap()["error"],
        "forbidden"
    );

    let hidden_created = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams"),
        &admin,
        Some(csrf),
    )
    .json(&serde_json::json!({
        "assessment_id": hidden_assessment,
        "title": "Admin-only context",
        "risk": "medium",
        "default_milestones": false
    }))
    .send()
    .await
    .unwrap();
    assert_eq!(hidden_created.status(), reqwest::StatusCode::CREATED);
    let hidden_id = hidden_created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let hidden_for_operator = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/{hidden_id}"),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(hidden_for_operator.status(), reqwest::StatusCode::NOT_FOUND);
    assert_eq!(
        hidden_for_operator
            .json::<serde_json::Value>()
            .await
            .unwrap()["error"],
        "not_found"
    );
    assert_eq!(
        http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/poams/{hidden_id}"),
            &admin,
            None,
        )
        .send()
        .await
        .unwrap()
        .status(),
        reqwest::StatusCode::OK
    );

    let created = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams"),
        &operator,
        Some(csrf),
    )
    .json(&create_body)
    .send()
    .await
    .unwrap();
    assert_eq!(created.status(), reqwest::StatusCode::CREATED);
    let mut detail = created.json::<serde_json::Value>().await.unwrap();
    let poam_id = detail["id"].as_str().unwrap().to_string();
    assert_eq!(detail["revision"], 1);
    assert_evidence_unchanged!();
    assert_eq!(detail["milestones"].as_array().unwrap().len(), 5);
    assert_eq!(detail["milestones"][0]["title"], "Update NixOS module");
    assert_eq!(
        detail["milestones"][4]["title"],
        "Verify compliance evaluation passes"
    );
    let (assignment_id, assignment_version_id, _bundle_id) =
        immutable_assignment_fixture(&pool, primary.system_id, primary.user_id).await;
    let assignment_before = assignment_snapshot(&pool, assignment_version_id).await;
    let assignment_linked = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/assignments"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({
        "revision": detail["revision"],
        "assignment_version_id": assignment_version_id
    }))
    .send()
    .await
    .unwrap();
    assert_eq!(assignment_linked.status(), reqwest::StatusCode::OK);
    detail = assignment_linked.json().await.unwrap();
    assert_eq!(
        detail["assignment_references"][0]["assignment_id"],
        assignment_id.to_string()
    );
    assert_eq!(
        detail["assignment_references"][0]["assignment_version_id"],
        assignment_version_id.to_string()
    );
    assert_eq!(
        assignment_snapshot(&pool, assignment_version_id).await,
        assignment_before
    );
    assert_evidence_unchanged!();
    let assignment_audit: (Uuid, String, String, serde_json::Value) = sqlx::query_as(
        "SELECT actor_user_id,action,target,metadata FROM admin_audit_events WHERE action='poam_assignment_linked' AND target=$1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(format!("poam:{poam_id}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(assignment_audit.0, primary.user_id);
    assert_eq!(assignment_audit.1, "poam_assignment_linked");
    assert_eq!(assignment_audit.2, format!("poam:{poam_id}"));
    assert_eq!(
        assignment_audit.3["assignment_version_id"],
        assignment_version_id.to_string()
    );

    let visible = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams?q=HTTP%20lifecycle&limit=1&offset=0"),
        &viewer,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(visible.status(), reqwest::StatusCode::OK);
    assert_eq!(
        visible.json::<serde_json::Value>().await.unwrap()["items"][0]["id"],
        poam_id
    );
    let operator_compatible = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/{poam_id}/compatible"),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap()
    .json::<serde_json::Value>()
    .await
    .unwrap();
    assert!(operator_compatible["items"].as_array().unwrap().is_empty());
    let admin_compatible = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/{poam_id}/compatible"),
        &admin,
        None,
    )
    .send()
    .await
    .unwrap()
    .json::<serde_json::Value>()
    .await
    .unwrap();
    assert_eq!(
        admin_compatible["items"][0]["assessment_id"],
        compatible_assessment.to_string()
    );

    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(primary.user_id)
        .bind(prod)
        .execute(&pool)
        .await
        .unwrap();
    let linked = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/findings"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({
        "revision": detail["revision"],
        "assessment_id": compatible_assessment
    }))
    .send()
    .await
    .unwrap();
    assert_eq!(linked.status(), reqwest::StatusCode::OK);
    detail = linked.json().await.unwrap();
    assert_eq!(detail["findings"].as_array().unwrap().len(), 2);
    assert_evidence_unchanged!();
    let compatible_finding_id = detail["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["system_id"] == compatible.system_id.to_string())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let assignment_unlinked_while_fail = http_request(
        &client,
        reqwest::Method::DELETE,
        format!(
            "{base}/api/v1/poams/{poam_id}/assignments/{assignment_version_id}?revision={}",
            detail["revision"]
        ),
        &operator,
        Some(csrf),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(
        assignment_unlinked_while_fail.status(),
        reqwest::StatusCode::OK
    );
    detail = assignment_unlinked_while_fail.json().await.unwrap();
    assert_eq!(
        assignment_snapshot(&pool, assignment_version_id).await,
        assignment_before
    );
    assert_evidence_unchanged!();
    let assignment_relinked_while_fail = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/assignments"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({
        "revision": detail["revision"],
        "assignment_version_id": assignment_version_id
    }))
    .send()
    .await
    .unwrap();
    assert_eq!(
        assignment_relinked_while_fail.status(),
        reqwest::StatusCode::OK
    );
    detail = assignment_relinked_while_fail.json().await.unwrap();
    assert_eq!(
        assignment_snapshot(&pool, assignment_version_id).await,
        assignment_before
    );
    assert_evidence_unchanged!();

    let finding_unlinked_while_fail = http_request(
        &client,
        reqwest::Method::DELETE,
        format!(
            "{base}/api/v1/poams/{poam_id}/findings/{compatible_finding_id}?revision={}",
            detail["revision"]
        ),
        &operator,
        Some(csrf),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(
        finding_unlinked_while_fail.status(),
        reqwest::StatusCode::OK
    );
    detail = finding_unlinked_while_fail.json().await.unwrap();
    assert_evidence_unchanged!();
    let finding_relinked_while_fail = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/findings"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({
        "revision": detail["revision"],
        "assessment_id": compatible_assessment
    }))
    .send()
    .await
    .unwrap();
    assert_eq!(
        finding_relinked_while_fail.status(),
        reqwest::StatusCode::OK
    );
    detail = finding_relinked_while_fail.json().await.unwrap();
    assert_evidence_unchanged!();

    let updated = http_request(
        &client,
        reqwest::Method::PATCH,
        format!("{base}/api/v1/poams/{poam_id}"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({"revision": detail["revision"], "owner": "security"}))
    .send()
    .await
    .unwrap();
    assert_eq!(updated.status(), reqwest::StatusCode::OK);
    detail = updated.json().await.unwrap();
    assert_evidence_unchanged!();
    let activity_before_stale: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM poam_activity WHERE poam_id=$1")
            .bind(Uuid::parse_str(&poam_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    let audit_before_stale: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit_events WHERE target=$1")
            .bind(format!("poam:{poam_id}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    let stale = http_request(
        &client,
        reqwest::Method::PATCH,
        format!("{base}/api/v1/poams/{poam_id}"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({"revision": 1, "owner": "stale"}))
    .send()
    .await
    .unwrap();
    assert_eq!(stale.status(), reqwest::StatusCode::CONFLICT);
    assert_eq!(
        stale.json::<serde_json::Value>().await.unwrap()["error"],
        "stale_revision"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM poam_activity WHERE poam_id=$1")
            .bind(Uuid::parse_str(&poam_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap(),
        activity_before_stale
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM admin_audit_events WHERE target=$1")
            .bind(format!("poam:{poam_id}"))
            .fetch_one(&pool)
            .await
            .unwrap(),
        audit_before_stale
    );
    assert_evidence_unchanged!();

    for status in [
        "in_progress",
        "blocked",
        "in_progress",
        "awaiting_verification",
    ] {
        let transitioned = http_request(
            &client,
            reqwest::Method::POST,
            format!("{base}/api/v1/poams/{poam_id}/transition"),
            &operator,
            Some(csrf),
        )
        .json(&serde_json::json!({
            "revision": detail["revision"],
            "status": status,
            "note": format!("transition to {status}")
        }))
        .send()
        .await
        .unwrap();
        assert_eq!(transitioned.status(), reqwest::StatusCode::OK);
        detail = transitioned.json().await.unwrap();
        assert_eq!(detail["status"], status);
        assert_evidence_unchanged!();
    }
    let noted = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/notes"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({"revision": detail["revision"], "text": "Ready"}))
    .send()
    .await
    .unwrap();
    assert_eq!(noted.status(), reqwest::StatusCode::OK);
    detail = noted.json().await.unwrap();
    assert!(
        detail["activity"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["kind"] == "note" && row["payload"]["text"] == "Ready")
    );
    assert_evidence_unchanged!();

    let added = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/milestones"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({
        "revision": detail["revision"],
        "title": "Document evidence",
        "target_date": "2026-12-31"
    }))
    .send()
    .await
    .unwrap();
    assert_eq!(added.status(), reqwest::StatusCode::CREATED);
    detail = added.json().await.unwrap();
    assert_evidence_unchanged!();
    let milestone_id = detail["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .find(|milestone| milestone["title"] == "Document evidence")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let changed = http_request(
        &client,
        reqwest::Method::PATCH,
        format!("{base}/api/v1/poams/{poam_id}/milestones/{milestone_id}"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({"revision": detail["revision"], "completed": true}))
    .send()
    .await
    .unwrap();
    assert_eq!(changed.status(), reqwest::StatusCode::OK);
    detail = changed.json().await.unwrap();
    assert!(
        detail["milestones"]
            .as_array()
            .unwrap()
            .iter()
            .find(|milestone| milestone["id"] == milestone_id)
            .unwrap()["completed_at"]
            .is_string()
    );
    assert_evidence_unchanged!();
    let removed = http_request(
        &client,
        reqwest::Method::DELETE,
        format!(
            "{base}/api/v1/poams/{poam_id}/milestones/{milestone_id}?revision={}",
            detail["revision"]
        ),
        &operator,
        Some(csrf),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(removed.status(), reqwest::StatusCode::OK);
    detail = removed.json().await.unwrap();
    assert_eq!(detail["milestones"].as_array().unwrap().len(), 5);
    assert_evidence_unchanged!();

    let failed = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/verify"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({"revision": detail["revision"]}))
    .send()
    .await
    .unwrap();
    assert_eq!(failed.status(), reqwest::StatusCode::OK);
    let verification = failed.json::<serde_json::Value>().await.unwrap();
    assert_eq!(verification["outcome"], "rejected");
    detail["revision"] = verification["revision"].clone();
    assert_evidence_unchanged!();
    let close_rejected = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/close"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({"revision": detail["revision"]}))
    .send()
    .await
    .unwrap();
    assert_eq!(
        close_rejected.status(),
        reqwest::StatusCode::PRECONDITION_FAILED
    );
    assert_eq!(
        close_rejected.json::<serde_json::Value>().await.unwrap()["error"],
        "closure_not_ready"
    );
    assert_evidence_unchanged!();
    detail = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/{poam_id}"),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();

    let primary_finding_id = detail["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["system_id"] == primary.system_id.to_string())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let waiver_created = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/finding-waivers"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({
        "finding_id": primary_finding_id,
        "assessment_id": primary_assessment,
        "justification": "HTTP decision boundary"
    }))
    .send()
    .await
    .unwrap();
    let waiver_status = waiver_created.status();
    let waiver_body = waiver_created.json::<serde_json::Value>().await.unwrap();
    assert_eq!(waiver_status, reqwest::StatusCode::CREATED, "{waiver_body}");
    let waiver_id = waiver_body["waiver_id"].as_str().unwrap().to_string();
    assert_evidence_unchanged!();
    let denied_decision = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/finding-waivers/{waiver_id}/status"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({"status":"accepted","expires_at":null}))
    .send()
    .await
    .unwrap();
    assert_eq!(denied_decision.status(), reqwest::StatusCode::FORBIDDEN);
    let waiver_decided = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/finding-waivers/{waiver_id}/status"),
        &admin,
        Some(csrf),
    )
    .json(&serde_json::json!({"status":"rejected","expires_at":null}))
    .send()
    .await
    .unwrap();
    assert_eq!(waiver_decided.status(), reqwest::StatusCode::OK);
    assert_evidence_unchanged!();

    let mut tx = pool.begin().await.unwrap();
    persist_assessment(&mut tx, &primary, EnforcementOutcome::Pass).await;
    persist_assessment(&mut tx, &compatible, EnforcementOutcome::Pass).await;
    tx.commit().await.unwrap();
    expected_evidence = assessment_evidence_snapshot(&pool, &evidence_systems).await;
    let verified = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/verify"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({"revision": detail["revision"]}))
    .send()
    .await
    .unwrap();
    assert_eq!(verified.status(), reqwest::StatusCode::OK);
    let verification = verified.json::<serde_json::Value>().await.unwrap();
    assert_eq!(verification["outcome"], "accepted");
    detail["revision"] = verification["revision"].clone();
    assert_evidence_unchanged!();
    for (token, csrf_header) in [(&viewer, Some(csrf)), (&operator, None)] {
        let denied = http_request(
            &client,
            reqwest::Method::POST,
            format!("{base}/api/v1/poams/{poam_id}/close"),
            token,
            csrf_header,
        )
        .json(&serde_json::json!({"revision": detail["revision"]}))
        .send()
        .await
        .unwrap();
        assert_eq!(denied.status(), reqwest::StatusCode::FORBIDDEN);
    }
    let closed = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/close"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({"revision": detail["revision"]}))
    .send()
    .await
    .unwrap();
    assert_eq!(closed.status(), reqwest::StatusCode::OK);
    detail = closed.json().await.unwrap();
    assert_eq!(detail["status"], "completed");
    assert_evidence_unchanged!();
    for (token, csrf_header) in [(&viewer, Some(csrf)), (&operator, None)] {
        let denied = http_request(
            &client,
            reqwest::Method::POST,
            format!("{base}/api/v1/poams/{poam_id}/reopen"),
            token,
            csrf_header,
        )
        .json(&serde_json::json!({"revision": detail["revision"]}))
        .send()
        .await
        .unwrap();
        assert_eq!(denied.status(), reqwest::StatusCode::FORBIDDEN);
    }
    let reopened = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/poams/{poam_id}/reopen"),
        &operator,
        Some(csrf),
    )
    .json(&serde_json::json!({"revision": detail["revision"]}))
    .send()
    .await
    .unwrap();
    assert_eq!(reopened.status(), reqwest::StatusCode::OK);
    detail = reopened.json().await.unwrap();
    assert_eq!(detail["status"], "in_progress");
    assert_evidence_unchanged!();
    assert_eq!(
        detail["assignment_references"][0]["assignment_id"],
        assignment_id.to_string()
    );
    assert_eq!(
        assignment_snapshot(&pool, assignment_version_id).await,
        assignment_before
    );
    assert_evidence_unchanged!();
    let assignment_unlinked = http_request(
        &client,
        reqwest::Method::DELETE,
        format!(
            "{base}/api/v1/poams/{poam_id}/assignments/{assignment_version_id}?revision={}",
            detail["revision"]
        ),
        &operator,
        Some(csrf),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(assignment_unlinked.status(), reqwest::StatusCode::OK);
    detail = assignment_unlinked.json().await.unwrap();
    assert!(
        detail["assignment_references"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        assignment_snapshot(&pool, assignment_version_id).await,
        assignment_before
    );
    let assignment_unlink_audit: (Uuid, String, String, serde_json::Value) = sqlx::query_as(
        "SELECT actor_user_id,action,target,metadata FROM admin_audit_events WHERE action='poam_assignment_unlinked' AND target=$1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(format!("poam:{poam_id}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(assignment_unlink_audit.0, primary.user_id);
    assert_eq!(assignment_unlink_audit.1, "poam_assignment_unlinked");
    assert_eq!(assignment_unlink_audit.2, format!("poam:{poam_id}"));
    assert_eq!(
        assignment_unlink_audit.3["assignment_version_id"],
        assignment_version_id.to_string()
    );
    let unlinked = http_request(
        &client,
        reqwest::Method::DELETE,
        format!(
            "{base}/api/v1/poams/{poam_id}/findings/{compatible_finding_id}?revision={}",
            detail["revision"]
        ),
        &operator,
        Some(csrf),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(unlinked.status(), reqwest::StatusCode::OK);
    detail = unlinked.json().await.unwrap();
    assert_eq!(
        detail["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|finding| finding["link_active"] == true)
            .count(),
        1
    );
    assert_evidence_unchanged!();

    let poam_uuid = Uuid::parse_str(&poam_id).unwrap();
    let actor_identifier: String = sqlx::query_scalar("SELECT email FROM users WHERE id=$1")
        .bind(primary.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let activities: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT kind,payload FROM poam_activity WHERE poam_id=$1 ORDER BY created_at,id",
    )
    .bind(poam_uuid)
    .fetch_all(&pool)
    .await
    .unwrap();
    let expected_kinds = [
        "created",
        "milestone_added",
        "milestone_added",
        "milestone_added",
        "milestone_added",
        "milestone_added",
        "assignment_linked",
        "finding_linked",
        "assignment_unlinked",
        "assignment_linked",
        "finding_unlinked",
        "finding_linked",
        "updated",
        "status_changed",
        "status_changed",
        "status_changed",
        "status_changed",
        "note",
        "milestone_added",
        "milestone_updated",
        "milestone_removed",
        "verification_attempted",
        "verification_attempted",
        "verification_attempted",
        "verification_attempted",
        "closed",
        "reopened",
        "assignment_unlinked",
        "finding_unlinked",
    ];
    assert_eq!(
        activities
            .iter()
            .map(|(kind, _)| kind.as_str())
            .collect::<Vec<_>>(),
        expected_kinds
    );
    for (kind, payload) in &activities {
        assert_eq!(payload["poam_id"], poam_id);
        assert!(payload["revision"].is_i64());
        let exact_audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_audit_events WHERE actor_user_id=$1 AND actor_identifier=$2 AND action=$3 AND target=$4 AND request_origin IS NULL AND metadata=$5",
        )
        .bind(primary.user_id)
        .bind(&actor_identifier)
        .bind(format!("poam_{kind}"))
        .bind(format!("poam:{poam_id}"))
        .bind(payload)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            exact_audits, 1,
            "missing exact audit counterpart for {kind}"
        );
    }
    let waiver_audits: Vec<(Uuid, String, String, Option<String>, serde_json::Value)> =
        sqlx::query_as(
            "SELECT actor_user_id,action,target,request_origin,metadata FROM admin_audit_events WHERE target=$1 AND action LIKE 'finding_waiver_%' ORDER BY created_at,id",
        )
        .bind(format!("finding:{primary_finding_id}"))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(waiver_audits.len(), 2);
    assert_eq!(waiver_audits[0].0, primary.user_id);
    assert_eq!(waiver_audits[0].1, "finding_waiver_created");
    assert_eq!(waiver_audits[0].2, format!("finding:{primary_finding_id}"));
    assert_eq!(waiver_audits[0].3, None);
    assert_eq!(waiver_audits[0].4["waiver_id"], waiver_id);
    assert_eq!(waiver_audits[0].4["finding_id"], primary_finding_id);
    assert_eq!(
        waiver_audits[0].4["assessment_id"],
        primary_assessment.to_string()
    );
    assert_eq!(waiver_audits[0].4["status"], "pending");
    assert_eq!(waiver_audits[1].0, admin_id);
    assert_eq!(waiver_audits[1].1, "finding_waiver_status_changed");
    assert_eq!(waiver_audits[1].2, format!("finding:{primary_finding_id}"));
    assert_eq!(waiver_audits[1].3, None);
    assert_eq!(waiver_audits[1].4["waiver_id"], waiver_id);
    assert_eq!(waiver_audits[1].4["finding_id"], primary_finding_id);
    assert_eq!(waiver_audits[1].4["from"], "pending");
    assert_eq!(waiver_audits[1].4["to"], "rejected");
    assert!(waiver_audits[1].4["expires_at"].is_null());

    for path in [
        "/api/v1/poams/dashboard".to_string(),
        "/api/v1/poams/dashboard/watchlist".to_string(),
        format!(
            "/api/v1/poams/rollups/systems?ids={},{}",
            primary.system_id, compatible.system_id
        ),
        format!("/api/v1/poams/rollups/bundles?ids={}", Uuid::new_v4()),
    ] {
        let response = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}{path}"),
            &operator,
            None,
        )
        .send()
        .await
        .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK, "{path}");
    }
}

#[sqlx::test]
async fn relationship_services_batch_active_history_and_immutable_assignments(pool: PgPool) {
    let visible = assessment_fixture(&pool).await;
    let hidden = assessment_fixture_for_policy(&pool, &visible).await;
    for fixture in [&visible, &hidden] {
        let mut tx = pool.begin().await.unwrap();
        persist_assessment(&mut tx, fixture, EnforcementOutcome::Fail).await;
        tx.commit().await.unwrap();
    }
    let hidden_assessment = current_assessment_id(&pool, &hidden).await;
    let dev: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='dev'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let prod: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='prod'")
        .fetch_one(&pool)
        .await
        .unwrap();
    for (system_id, environment_id) in [(visible.system_id, dev), (hidden.system_id, prod)] {
        sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
            .bind(system_id)
            .bind(environment_id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let actor = PoamActor {
        user_id: visible.user_id,
        identifier: "relationship-operator@example.invalid".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: vec![dev],
        request_origin: None,
    };
    let admin = admin_actor(visible.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());

    let historical = create_service_poam(&pool, &visible, &admin, &clock, "Historical POAM").await;
    let awaiting = awaiting_verification(&pool, &admin, historical, &clock).await;
    let mut passing = pool.begin().await.unwrap();
    persist_assessment(&mut passing, &visible, EnforcementOutcome::Pass).await;
    passing.commit().await.unwrap();
    let historical = poam_service::close(
        &pool,
        &admin,
        awaiting.poam.id,
        awaiting.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    let mut failing = pool.begin().await.unwrap();
    persist_assessment(&mut failing, &visible, EnforcementOutcome::Fail).await;
    failing.commit().await.unwrap();
    let current_assessment = current_assessment_id(&pool, &visible).await;
    let active = create_service_poam(&pool, &visible, &admin, &clock, "Active POAM").await;

    let relationships = poam_service::finding_relationships(
        &pool,
        &actor,
        &[current_assessment, hidden_assessment, Uuid::new_v4()],
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        relationships.len(),
        1,
        "hidden and unknown assessments are omitted"
    );
    assert_eq!(relationships[0].assessment_id, Some(current_assessment));
    assert_eq!(
        relationships[0].finding_id,
        finding_id(&pool, &visible).await
    );
    assert_eq!(
        relationships[0].active_poam.as_ref().unwrap().id,
        active.poam.id
    );
    assert_eq!(relationships[0].historical_poams.len(), 1);
    assert_eq!(relationships[0].historical_poams[0].id, historical.poam.id);
    assert_eq!(relationships[0].historical_poams[0].status, "completed");

    let (assignment_id, assignment_version_id, _) =
        immutable_assignment_fixture(&pool, visible.system_id, visible.user_id).await;
    let assignment_before = assignment_snapshot(&pool, assignment_version_id).await;
    sqlx::query("INSERT INTO poam_assignment_references(poam_id,assignment_id,assignment_version_id,added_by) VALUES($1,$2,$3,$4)")
        .bind(active.poam.id)
        .bind(assignment_id)
        .bind(assignment_version_id)
        .bind(visible.user_id)
        .execute(&pool)
        .await
        .unwrap();
    let assignments = poam_service::assignment_relationships(
        &pool,
        &actor,
        &[assignment_version_id, Uuid::new_v4()],
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].assignment_version_id, assignment_version_id);
    assert_eq!(assignments[0].poams.len(), 1);
    assert_eq!(assignments[0].poams[0].id, active.poam.id);
    assert_eq!(
        assignment_snapshot(&pool, assignment_version_id).await,
        assignment_before
    );
}

#[sqlx::test]
async fn assignment_compatibility_keeps_scope_and_lineage_from_the_same_finding(pool: PgPool) {
    let primary = assessment_fixture(&pool).await;
    let secondary = assessment_fixture(&pool).await;
    for fixture in [&primary, &secondary] {
        let mut tx = pool.begin().await.unwrap();
        persist_assessment(&mut tx, fixture, EnforcementOutcome::Fail).await;
        tx.commit().await.unwrap();
    }

    let actor = admin_actor(primary.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let poam = create_service_poam(&pool, &primary, &actor, &clock, "Paired context").await;
    let secondary_finding = finding_id(&pool, &secondary).await;
    sqlx::query("INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)")
        .bind(poam.poam.id)
        .bind(secondary_finding)
        .bind(actor.user_id)
        .execute(&pool)
        .await
        .unwrap();

    let (assignment_id, assignment_version_id, _) =
        immutable_assignment_fixture(&pool, secondary.system_id, actor.user_id).await;
    sqlx::query("UPDATE compliance_bundle_assignments SET system_id=$2 WHERE id=$1")
        .bind(assignment_id)
        .bind(primary.system_id)
        .execute(&pool)
        .await
        .unwrap();

    let result = poam_service::link_assignment(
        &pool,
        &actor,
        poam.poam.id,
        AssignmentReferenceRequest {
            revision: poam.poam.revision,
            assignment_version_id,
        },
        &clock,
    )
    .await;
    assert!(matches!(
        result,
        Err(PoamError::Validation(
            "incompatible_assignment_reference",
            _
        ))
    ));
}

#[sqlx::test]
async fn authenticated_relationship_http_contracts_enforce_bounds_visibility_and_compatibility(
    pool: PgPool,
) {
    let target = assessment_fixture(&pool).await;
    let candidate = assessment_fixture_for_policy(&pool, &target).await;
    let completed_candidate = assessment_fixture_for_policy(&pool, &target).await;
    let hidden_candidate = assessment_fixture_for_policy(&pool, &target).await;
    let incompatible = assessment_fixture(&pool).await;
    for fixture in [
        &target,
        &candidate,
        &completed_candidate,
        &hidden_candidate,
        &incompatible,
    ] {
        let mut tx = pool.begin().await.unwrap();
        persist_assessment(&mut tx, fixture, EnforcementOutcome::Fail).await;
        tx.commit().await.unwrap();
    }
    let dev: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='dev'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let prod: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='prod'")
        .fetch_one(&pool)
        .await
        .unwrap();
    for (system_id, environment_id) in [
        (target.system_id, dev),
        (candidate.system_id, dev),
        (completed_candidate.system_id, dev),
        (hidden_candidate.system_id, prod),
        (incompatible.system_id, dev),
    ] {
        sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
            .bind(system_id)
            .bind(environment_id)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(target.user_id)
        .bind(dev)
        .execute(&pool)
        .await
        .unwrap();
    let operator = session(&pool, target.user_id, AuthRole::Operator).await;
    let admin = admin_actor(target.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let candidate_poam =
        create_service_poam(&pool, &candidate, &admin, &clock, "Needle compatible").await;
    sqlx::query("UPDATE poams SET owner='Needle Owner' WHERE id=$1")
        .bind(candidate_poam.poam.id)
        .execute(&pool)
        .await
        .unwrap();
    let hidden_poam =
        create_service_poam(&pool, &hidden_candidate, &admin, &clock, "Needle hidden").await;
    let incompatible_poam =
        create_service_poam(&pool, &incompatible, &admin, &clock, "Needle incompatible").await;
    let completed = create_service_poam(
        &pool,
        &completed_candidate,
        &admin,
        &clock,
        "Needle completed",
    )
    .await;
    let awaiting = awaiting_verification(&pool, &admin, completed, &clock).await;
    let mut pass = pool.begin().await.unwrap();
    persist_assessment(&mut pass, &completed_candidate, EnforcementOutcome::Pass).await;
    pass.commit().await.unwrap();
    let completed = poam_service::close(
        &pool,
        &admin,
        awaiting.poam.id,
        awaiting.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    let target_assessment = current_assessment_id(&pool, &target).await;
    let hidden_assessment = current_assessment_id(&pool, &hidden_candidate).await;
    let (assignment_id, assignment_version_id, _) =
        immutable_assignment_fixture(&pool, candidate.system_id, candidate.user_id).await;
    let assignment_before = assignment_snapshot(&pool, assignment_version_id).await;
    sqlx::query("INSERT INTO poam_assignment_references(poam_id,assignment_id,assignment_version_id,added_by) VALUES($1,$2,$3,$4)")
        .bind(candidate_poam.poam.id)
        .bind(assignment_id)
        .bind(assignment_version_id)
        .bind(target.user_id)
        .execute(&pool)
        .await
        .unwrap();

    let base = poam_http_server(pool.clone()).await;
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .get(format!(
                "{base}/api/v1/poams/compatible?assessment_id={target_assessment}"
            ))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let compatible = http_request(
        &client,
        reqwest::Method::GET,
        format!(
            "{base}/api/v1/poams/compatible?assessment_id={target_assessment}&limit=10&offset=0"
        ),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(compatible.status(), reqwest::StatusCode::OK);
    let compatible: serde_json::Value = compatible.json().await.unwrap();
    assert_eq!(compatible["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        compatible["items"][0]["id"],
        candidate_poam.poam.id.to_string()
    );
    let returned_ids = compatible["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(!returned_ids.contains(&hidden_poam.poam.id.to_string().as_str()));
    assert!(!returned_ids.contains(&incompatible_poam.poam.id.to_string().as_str()));
    assert!(!returned_ids.contains(&completed.poam.id.to_string().as_str()));
    let searched = http_request(
        &client,
        reqwest::Method::GET,
        format!(
            "{base}/api/v1/poams/compatible?assessment_id={target_assessment}&q=Needle%20Owner"
        ),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap()
    .json::<serde_json::Value>()
    .await
    .unwrap();
    assert_eq!(searched["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        searched["items"][0]["id"],
        candidate_poam.poam.id.to_string()
    );

    let invalid_limit = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/compatible?assessment_id={target_assessment}&limit=101"),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(invalid_limit.status(), reqwest::StatusCode::BAD_REQUEST);
    let too_many = (0..101)
        .map(|_| Uuid::new_v4().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let bounded = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/relationships/findings?assessment_ids={too_many}"),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(bounded.status(), reqwest::StatusCode::BAD_REQUEST);
    let assignments_bounded = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/relationships/assignments?ids={too_many}"),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(
        assignments_bounded.status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    let findings = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/relationships/findings?assessment_ids={target_assessment},{hidden_assessment}"),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap()
    .json::<serde_json::Value>()
    .await
    .unwrap();
    assert_eq!(findings.as_array().unwrap().len(), 1);
    assert_eq!(findings[0]["assessment_id"], target_assessment.to_string());
    assert!(findings[0]["active_poam"].is_null());

    let assignments = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/relationships/assignments?ids={assignment_version_id}"),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(assignments.status(), reqwest::StatusCode::OK);
    let assignments: serde_json::Value = assignments.json().await.unwrap();
    assert_eq!(
        assignments[0]["assignment_version_id"],
        assignment_version_id.to_string()
    );
    assert_eq!(
        assignments[0]["poams"][0]["id"],
        candidate_poam.poam.id.to_string()
    );
    assert_eq!(
        assignment_snapshot(&pool, assignment_version_id).await,
        assignment_before
    );

    create_service_poam(&pool, &target, &admin, &clock, "Target active remediation").await;
    let conflicted = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/compatible?assessment_id={target_assessment}"),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap()
    .json::<serde_json::Value>()
    .await
    .unwrap();
    assert!(conflicted["items"].as_array().unwrap().is_empty());
}
