use axum::{Router, routing::get};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, NaiveDate, TimeDelta, TimeZone, Utc};
use crystal_forge::api::models::{
    CveEnvironmentDisposition, CveEnvironmentTriageAction, CveFilters, FleetCveInventorySection,
    FleetCveMutationDetailScope, FleetCvePoamRequest, FleetCveTriageRequest, FleetCveTriageRollup,
    SystemCveEffectiveDispositionSource, SystemCveInventoryParams, SystemCveTriageAction,
    SystemCveTriageRequest, SystemCveTriageScopeChoice, SystemCveTriageScopeKind,
};
use crystal_forge::auth::extractors::AuthenticatedUser;
use crystal_forge::auth::session::{
    CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
};
use crystal_forge::compliance::canonical::semantic_digest;
use crystal_forge::compliance::resolver::{
    EffectivePolicySet, ResolutionOutcome, resolve_system_effective_policies,
};
use crystal_forge::handlers::agent::state as agent_state_handler;
use crystal_forge::handlers::agent_request::CFState;
use crystal_forge::handlers::api::cves as cve_handlers;
use crystal_forge::handlers::api::poam as poam_handlers;
use crystal_forge::handlers::api::systems as system_handlers;
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::models::deployment_policies::{
    CompositePolicyConfig, CompositeRuleOutcome, CreateDeploymentPolicyRequest, EnforcementOutcome,
    EnforcementPhase, composite_config_digest,
};
use crystal_forge::models::poam::{
    AddCveFindingRequest, AddFindingRequest, AssignmentReferenceRequest, CreateCvePoamRequest,
    CreatePoamRequest, CreateWaiverRequest, CveObservationReference, CveRelationshipRowKey,
    FindingObservationReference, FindingObservationSource, PoamAssigneeRequest, PoamAssigneeView,
    PoamDetailQuery, PoamListQuery, PoamRisk, PoamStatus, TransitionPoamRequest, UpdatePoamRequest,
    WaiverDecision, WaiverDecisionRequest,
};
use crystal_forge::models::system_states::SystemState;
use crystal_forge::queries::compliance::nix_policy_observation_reference;
use crystal_forge::queries::cves::{
    CveExportError, CveReadScope, MAX_CVE_EXPORT_ROWS, SystemCveInventoryPageRequest,
    fetch_cve_affected_systems, fetch_cve_detail, fetch_cve_fleet_stats, fetch_cve_justifications,
    fetch_cve_list, fetch_cve_packages_grouped, fetch_cves_for_export,
    fetch_exact_system_vulnerabilities, fetch_package_names, fetch_system_cve_inventory_page,
};
use crystal_forge::queries::poam;
use crystal_forge::queries::users::insert_user;
use crystal_forge::queries::{
    auth_identity::{create_user_session, sync_user_role},
    commits::insert_commit,
    deployment_policies::create_deployment_policy,
    derivations::{SuccessfulEvalWrite, record_successful_eval_result},
    evaluation_snapshots::{persist_available_snapshot_tx, reconcile_external_current_generation},
    flakes::insert_flake,
    system_states::insert_system_state,
};
use crystal_forge::queue::QueueNotifier;
use crystal_forge::server::jobs::BackgroundJobRegistry;
use crystal_forge::services::composite_enforcement::persist_evaluation_assessments_in_tx;
use crystal_forge::services::poam::{self as poam_service, PoamActor, PoamClock, PoamError};
use ed25519_dalek::Signer;
use sqlx::{PgPool, Postgres, Transaction, migrate::Migrate};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

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

async fn apply_migration(pool: &PgPool, version: i64) {
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

#[derive(Clone)]
struct FixedClock(DateTime<Utc>);

impl PoamClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

#[sqlx::test(migrations = false)]
async fn migration_0258_preserves_populated_legacy_poams_and_adds_typed_shape(pool: PgPool) {
    apply_migrations_through(&pool, 257).await;
    let fixture = failing_assessment_fixture(&pool).await;
    let finding_id: Uuid = sqlx::query_scalar(
        r#"SELECT finding.id
           FROM composite_policy_assessments assessment
           JOIN poam_findings finding
             ON finding.system_id=assessment.system_id
            AND finding.policy_lineage_id=assessment.policy_lineage_id
           WHERE assessment.system_id=$1
           ORDER BY assessment.created_at DESC
           LIMIT 1"#,
    )
    .bind(fixture.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let poam_id: Uuid = sqlx::query_scalar(
        "INSERT INTO poams(title,owner,risk,created_by) VALUES('Legacy before 0258','Legacy Security Team','high',$1) RETURNING id",
    )
    .bind(fixture.user_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)")
        .bind(poam_id)
        .bind(finding_id)
        .bind(fixture.user_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    apply_migration(&pool, 258).await;

    let columns: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM information_schema.columns
           WHERE table_schema='public' AND table_name='poams'
             AND column_name=ANY($1)"#,
    )
    .bind(vec!["owner_kind", "owner_user_id", "owner_group_name"])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(columns, 3);
    let function_exists: bool =
        sqlx::query_scalar("SELECT to_regprocedure('poam_assignee_view(poams)') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(function_exists);
    let preserved: (String, Option<String>, Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT owner,owner_kind,owner_user_id,owner_group_name FROM poams WHERE id=$1",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(preserved, ("Legacy Security Team".into(), None, None, None));
    let view: serde_json::Value =
        sqlx::query_scalar("SELECT poam_assignee_view(poams) FROM poams WHERE id=$1")
            .bind(poam_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(view["kind"], "legacy");
    assert_eq!(view["display"], "Legacy Security Team");

    let invalid_shape = sqlx::query(
        "UPDATE poams SET owner_kind='user',owner_user_id=$2,owner_group_name='group' WHERE id=$1",
    )
    .bind(poam_id)
    .bind(fixture.user_id)
    .execute(&pool)
    .await;
    assert!(invalid_shape.is_err());
}

#[sqlx::test(migrations = false)]
async fn migration_0260_upgrades_a_database_with_0259_already_applied(pool: PgPool) {
    apply_migrations_through(&pool, 259).await;

    apply_migration(&pool, 260).await;

    let objects_exist: (bool, bool, bool, bool) = sqlx::query_as(
        r#"SELECT
             to_regprocedure(
               'cve_coherent_environment_disposition_state(text,text,uuid)'
             ) IS NOT NULL,
             to_regclass('view_cve_list_with_metadata') IS NOT NULL,
             to_regclass('view_cves_grouped_by_package') IS NOT NULL,
             to_regclass('view_cve_fleet_stats') IS NOT NULL"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(objects_exist, (true, true, true, true));
}

#[sqlx::test(migrations = false)]
async fn migration_0279_to_0284_preserves_populated_retained_cve_poam(pool: PgPool) {
    apply_migrations_through(&pool, 278).await;
    let scheduled = assessment_fixture(&pool).await;
    let accepted = assessment_fixture(&pool).await;
    let actor = admin_actor(scheduled.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve = "CVE-2026-32702";
    let package = "upgrade-continuity-package";
    let scheduled_environment = assign_environment(&pool, "upgrade-scheduled", &[&scheduled]).await;
    let accepted_environment = assign_environment(&pool, "upgrade-accepted", &[&accepted]).await;
    let scheduled_observation = seal_exact_cve_scan(
        &pool,
        &scheduled,
        clock.now() - TimeDelta::minutes(2),
        Some((cve, package, "1.0", false)),
    )
    .await
    .unwrap();
    seal_exact_cve_scan(
        &pool,
        &accepted,
        clock.now() - TimeDelta::minutes(2),
        Some((cve, package, "1.0", false)),
    )
    .await;
    let retained_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM evaluation_generation_snapshots WHERE system_id=$1 AND generation=1",
    )
    .bind(scheduled.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut legacy_tx = pool.begin().await.unwrap();
    let poam_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO poams(title,plan,owner,owner_kind,owner_user_id,
                              target_date,risk,created_by)
           VALUES('Upgrade CVE remediation','Deploy clean scan','Upgrade owner',
                  'user',$1,$2,'high',$1) RETURNING id"#,
    )
    .bind(actor.user_id)
    .bind(clock.today() + TimeDelta::days(30))
    .fetch_one(&mut *legacy_tx)
    .await
    .unwrap();
    let finding_id: Uuid = sqlx::query_scalar(
        "INSERT INTO poam_cve_findings(system_id,canonical_cve_id,canonical_package_name) VALUES($1,$2,$3) RETURNING id",
    )
    .bind(scheduled.system_id)
    .bind(cve)
    .bind(package)
    .fetch_one(&mut *legacy_tx)
    .await
    .unwrap();
    let link_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO poam_cve_finding_links(
             poam_id,cve_finding_id,system_id,canonical_cve_id,
             canonical_package_name,baseline_scan_id,baseline_scan_derivation_id,
             baseline_scan_completed_at,baseline_generation_snapshot_id,
             baseline_generation,baseline_target_store_path,
             baseline_occurrence_derivation_path,baseline_observed_package_version,linked_by)
           SELECT $1,$2,$3,$4,$5,scan.id,scan.derivation_id,scan.completed_at,
                  retained.id,retained.generation,retained.source_store_path,
                  observation.observed_derivation_path,
                  observation.observed_package_version,$6
           FROM cve_scans scan
           JOIN cve_scan_vulnerability_observations observation
             ON observation.scan_id=scan.id AND observation.canonical_cve_id=$4
              AND observation.canonical_package_name=$5
           JOIN evaluation_generation_snapshots retained
             ON retained.system_id=$3 AND retained.derivation_id=scan.derivation_id
           WHERE scan.id=$7 RETURNING id"#,
    )
    .bind(poam_id)
    .bind(finding_id)
    .bind(scheduled.system_id)
    .bind(cve)
    .bind(package)
    .bind(actor.user_id)
    .bind(scheduled_observation.scan_id)
    .fetch_one(&mut *legacy_tx)
    .await
    .unwrap();
    legacy_tx.commit().await.unwrap();
    let scheduled_disposition_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO cve_environment_dispositions(
             environment_id,canonical_cve_id,canonical_package_name,state,
             poam_id,scheduled_by,scheduled_at)
           VALUES($1,$2,$3,'scheduled',$4,$5,now()) RETURNING id"#,
    )
    .bind(scheduled_environment)
    .bind(cve)
    .bind(package)
    .bind(poam_id)
    .bind(actor.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let accepted_disposition_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO cve_environment_dispositions(
             environment_id,canonical_cve_id,canonical_package_name,state,
             justification,accepted_by,accepted_at)
           VALUES($1,$2,$3,'accepted','Isolated upgrade risk',$4,now()) RETURNING id"#,
    )
    .bind(accepted_environment)
    .bind(cve)
    .bind(package)
    .bind(actor.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let before: (Uuid, Uuid, i32, String, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT baseline_scan_id,baseline_generation_snapshot_id,baseline_generation,baseline_observed_package_version,retired_at FROM poam_cve_finding_links WHERE id=$1",
    )
    .bind(link_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before.0, scheduled_observation.scan_id);
    assert_eq!(before.1, retained_id);
    assert_eq!(before.2, 1);
    assert_eq!(before.3, "1.0");
    assert_eq!(before.4, None);
    let artifacts_before: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM evaluation_snapshots),(SELECT count(*) FROM evaluation_generation_snapshots),(SELECT count(*) FROM pending_system_deployments)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT cve_coherent_environment_disposition_state($1,$2,$3)",
        )
        .bind(cve)
        .bind(package)
        .bind(scheduled_environment)
        .fetch_one(&pool)
        .await
        .unwrap(),
        Some("scheduled".into())
    );

    for version in [279, 280, 281, 282, 283, 284] {
        apply_migration(&pool, version).await;
    }
    let artifacts_after: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM evaluation_snapshots),(SELECT count(*) FROM evaluation_generation_snapshots),(SELECT count(*) FROM pending_system_deployments)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(artifacts_after, artifacts_before);
    let after: (Uuid, Option<Uuid>, i32, String, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT baseline_scan_id,baseline_generation_snapshot_id,baseline_generation,baseline_observed_package_version,retired_at FROM poam_cve_finding_links WHERE id=$1",
    )
    .bind(link_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after,
        (before.0, Some(before.1), before.2, before.3, before.4)
    );
    let disposition_history: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM cve_environment_dispositions WHERE canonical_cve_id=$1 ORDER BY id",
    )
    .bind(cve)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        disposition_history,
        [scheduled_disposition_id, accepted_disposition_id]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    );
    let nullable_columns: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM information_schema.columns
           WHERE table_schema='public' AND is_nullable='YES'
             AND (table_name,column_name) IN (
               ('poam_cve_finding_links','baseline_generation_snapshot_id'),
               ('poam_cve_verification_items','baseline_generation_snapshot_id'))"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(nullable_columns, 2);
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT cve_coherent_environment_disposition_state($1,$2,$3)",
        )
        .bind(cve)
        .bind(package)
        .bind(scheduled_environment)
        .fetch_one(&pool)
        .await
        .unwrap(),
        Some("scheduled".into())
    );
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT cve_coherent_environment_disposition_state($1,$2,$3)",
        )
        .bind(cve)
        .bind(package)
        .bind(accepted_environment)
        .fetch_one(&pool)
        .await
        .unwrap(),
        Some("accepted".into())
    );
    let rows = fetch_cve_list(&pool, &CveReadScope::All, &CveFilters::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].triage_status, "outstanding");
    let detail = poam_service::fleet_cve_detail(&pool, &actor, cve, package)
        .await
        .unwrap();
    assert_eq!(detail.rollup, FleetCveTriageRollup::Partial);
    assert_eq!(detail.affected_system_count, 2);
    assert!(detail.environments.iter().any(|environment| {
        environment.environment_id == scheduled_environment
            && matches!(environment.disposition, Some(CveEnvironmentDisposition::Scheduled { poam_id: id, .. }) if id == poam_id)
    }));
    seal_exact_cve_scan(&pool, &scheduled, clock.now(), None).await;
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        poam_id,
        TransitionPoamRequest {
            revision: 1,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let verified = poam_service::verify(&pool, &actor, poam_id, awaiting.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(verified["outcome"], "accepted");
    assert_eq!(verified["cve_items"][0]["result"], "pass");
    assert_eq!(
        poam_service::detail(&pool, &actor, poam_id, &clock)
            .await
            .unwrap()
            .verification_attempts[0]
            .cve_items[0]
            .baseline_generation_snapshot_id,
        Some(retained_id)
    );

    let unretained = assessment_fixture(&pool).await;
    sqlx::query("DELETE FROM evaluation_generation_snapshots WHERE system_id=$1")
        .bind(unretained.system_id)
        .execute(&pool)
        .await
        .unwrap();
    let unretained_observation = seal_exact_cve_scan(
        &pool,
        &unretained,
        clock.now(),
        Some(("CVE-2026-32703", package, "2.0", false)),
    )
    .await
    .unwrap();
    let new_poam = poam_service::create_cve(
        &pool,
        &actor,
        CreateCvePoamRequest {
            observation: unretained_observation.clone(),
            title: "Unretained upgrade finding".into(),
            plan: "Deploy a fix".into(),
            owner: "Upgrade owner".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        new_poam.cve_findings[0].baseline_generation_snapshot_id,
        None
    );
    assert_eq!(
        new_poam.cve_findings[0].baseline_scan_id,
        unretained_observation.scan_id
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM evaluation_generation_snapshots WHERE system_id=$1"
        )
        .bind(unretained.system_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn typed_assignee_create_update_and_legacy_search_matrix(pool: PgPool) {
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap());
    let candidate_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users(username,first_name,last_name,email) VALUES('typed-owner','  Ada  ','  Lovelace  ','ada@example.invalid') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO oidc_group_mappings(group_name) VALUES('team:platform/admin')")
        .execute(&pool)
        .await
        .unwrap();

    let user_fixture = failing_assessment_fixture(&pool).await;
    let hidden_environment: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name,description) VALUES($1,'typed assignee visibility') RETURNING id",
    )
    .bind(format!("typed-hidden-{}", Uuid::new_v4().simple()))
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$1 WHERE id=$2")
        .bind(hidden_environment)
        .bind(user_fixture.system_id)
        .execute(&pool)
        .await
        .unwrap();
    let actor = admin_actor(user_fixture.user_id);
    let user = poam_service::create(
        &pool,
        &actor,
        assessment_create_request(
            &pool,
            &user_fixture,
            "Typed user owner",
            "",
            Some(PoamAssigneeRequest::User {
                user_id: candidate_id,
            }),
        )
        .await,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(user.poam.owner, "Ada Lovelace");
    assert_eq!(
        user.poam.assignee,
        PoamAssigneeView::User {
            user_id: candidate_id,
            display: "Ada Lovelace".into(),
            available: true,
        }
    );
    let delete_assigned_user = sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(candidate_id)
        .execute(&pool)
        .await;
    assert!(delete_assigned_user.is_err());
    let unscoped_assignee = PoamActor {
        user_id: candidate_id,
        identifier: "typed-owner".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: Vec::new(),
        request_origin: None,
    };
    assert!(matches!(
        poam_service::detail(&pool, &unscoped_assignee, user.poam.id, &clock).await,
        Err(PoamError::NotFound)
    ));

    let group_fixture = failing_assessment_fixture(&pool).await;
    sqlx::query("UPDATE systems SET environment_id=$1 WHERE id=$2")
        .bind(hidden_environment)
        .bind(group_fixture.system_id)
        .execute(&pool)
        .await
        .unwrap();
    let group_actor = admin_actor(group_fixture.user_id);
    let group = poam_service::create(
        &pool,
        &group_actor,
        assessment_create_request(
            &pool,
            &group_fixture,
            "Typed group owner",
            "",
            Some(PoamAssigneeRequest::OidcGroup {
                group_name: "  TEAM:Platform/Admin  ".into(),
            }),
        )
        .await,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(group.poam.owner, "team:platform/admin");
    assert_eq!(
        group.poam.assignee,
        PoamAssigneeView::OidcGroup {
            group_name: "team:platform/admin".into(),
            display: "team:platform/admin".into(),
            available: true,
        }
    );
    assert!(matches!(
        poam_service::detail(&pool, &unscoped_assignee, group.poam.id, &clock).await,
        Err(PoamError::NotFound)
    ));

    sqlx::query("DELETE FROM oidc_group_mappings WHERE group_name='team:platform/admin'")
        .execute(&pool)
        .await
        .unwrap();
    let preserved = poam_service::detail(&pool, &group_actor, group.poam.id, &clock)
        .await
        .unwrap();
    assert!(matches!(
        preserved.poam.assignee,
        PoamAssigneeView::OidcGroup {
            available: false,
            ..
        }
    ));

    let unassigned_fixture = failing_assessment_fixture(&pool).await;
    let unassigned_actor = admin_actor(unassigned_fixture.user_id);
    let unassigned = poam_service::create(
        &pool,
        &unassigned_actor,
        assessment_create_request(
            &pool,
            &unassigned_fixture,
            "Explicitly unassigned",
            "",
            Some(PoamAssigneeRequest::Unassigned),
        )
        .await,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(unassigned.poam.owner, "");
    assert_eq!(unassigned.poam.assignee, PoamAssigneeView::Unassigned);

    let legacy_fixture = failing_assessment_fixture(&pool).await;
    let legacy_actor = admin_actor(legacy_fixture.user_id);
    let legacy = poam_service::create(
        &pool,
        &legacy_actor,
        assessment_create_request(
            &pool,
            &legacy_fixture,
            "Legacy owner",
            "Legacy Security Team",
            None,
        )
        .await,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        legacy.poam.assignee,
        PoamAssigneeView::Legacy {
            display: "Legacy Security Team".into()
        }
    );
    for query in [
        PoamListQuery {
            owner: Some("Legacy Security".into()),
            ..Default::default()
        },
        PoamListQuery {
            q: Some("Legacy Security".into()),
            ..Default::default()
        },
    ] {
        let page = poam_service::list(&pool, &legacy_actor, &query, &clock)
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, legacy.poam.id);
    }

    sqlx::query("INSERT INTO oidc_group_mappings(group_name) VALUES('team:platform/admin')")
        .execute(&pool)
        .await
        .unwrap();
    let updated = poam_service::update(
        &pool,
        &group_actor,
        group.poam.id,
        UpdatePoamRequest {
            revision: group.poam.revision,
            assignee: Some(PoamAssigneeRequest::User {
                user_id: candidate_id,
            }),
            ..Default::default()
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(updated.poam.revision, group.poam.revision + 1);
    assert!(matches!(
        updated.poam.assignee,
        PoamAssigneeView::User { user_id, .. } if user_id == candidate_id
    ));
    let activity = updated
        .activity
        .iter()
        .find(|event| event.kind == "updated")
        .unwrap();
    assert_eq!(activity.payload["old"]["assignee"]["kind"], "oidc_group");
    assert_eq!(activity.payload["new"]["assignee"]["kind"], "user");

    let legacy_cleared = poam_service::update(
        &pool,
        &group_actor,
        updated.poam.id,
        UpdatePoamRequest {
            revision: updated.poam.revision,
            owner: Some("Compatibility Owner".into()),
            ..Default::default()
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(matches!(
        legacy_cleared.poam.assignee,
        PoamAssigneeView::Legacy { .. }
    ));
    let stored: (Option<String>, Option<Uuid>, Option<String>) =
        sqlx::query_as("SELECT owner_kind,owner_user_id,owner_group_name FROM poams WHERE id=$1")
            .bind(updated.poam.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, (None, None, None));
}

#[sqlx::test(migrations = "./migrations")]
async fn typed_assignee_rejects_invalid_identity_ambiguity_and_shape(pool: PgPool) {
    let fixture = failing_assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap());
    let disabled_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users(username,first_name,last_name,email,is_active) VALUES('disabled-owner','Disabled','Owner','disabled@example.invalid',false) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    for assignee in [
        PoamAssigneeRequest::User {
            user_id: Uuid::new_v4(),
        },
        PoamAssigneeRequest::User {
            user_id: disabled_id,
        },
    ] {
        let error = poam_service::create(
            &pool,
            &actor,
            assessment_create_request(&pool, &fixture, "Invalid user", "", Some(assignee)).await,
            &clock,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PoamError::Validation("invalid_assignee_user", _)
        ));
    }
    for group_name in [
        "missing-group".to_string(),
        "invalid group".to_string(),
        "x".repeat(129),
    ] {
        let error = poam_service::create(
            &pool,
            &actor,
            assessment_create_request(
                &pool,
                &fixture,
                "Invalid group",
                "",
                Some(PoamAssigneeRequest::OidcGroup { group_name }),
            )
            .await,
            &clock,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PoamError::Validation("invalid_assignee_group", _)
        ));
    }
    let ambiguous = poam_service::create(
        &pool,
        &actor,
        assessment_create_request(
            &pool,
            &fixture,
            "Ambiguous owner",
            "Client label",
            Some(PoamAssigneeRequest::Unassigned),
        )
        .await,
        &clock,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        ambiguous,
        PoamError::Validation("ambiguous_assignee", _)
    ));

    let invalid_shape = sqlx::query(
        r#"INSERT INTO poams(title,owner,owner_kind,owner_user_id,owner_group_name,risk,created_by)
           VALUES('Invalid shape','label','user',$1,'group','high',$2)"#,
    )
    .bind(disabled_id)
    .bind(fixture.user_id)
    .execute(&pool)
    .await;
    assert!(invalid_shape.is_err());
}

#[sqlx::test(migrations = "./migrations")]
async fn assignee_catalog_is_mutator_only_bounded_minimal_and_non_authorizing(pool: PgPool) {
    let hidden_environment: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name,description) VALUES($1,'hidden') RETURNING id",
    )
    .bind(format!("typed-hidden-{}", Uuid::new_v4()))
    .fetch_one(&pool)
    .await
    .unwrap();
    let assigned_user = insert_user(
        &pool,
        "catalog-person@example.invalid",
        Some("Catalog Person"),
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO oidc_group_mappings(group_name,role,environments) VALUES('catalog:group','operator',$1)",
    )
    .bind(vec!["sensitive-environment-name"])
    .execute(&pool)
    .await
    .unwrap();
    let (_, viewer) = role_session(&pool, AuthRole::Viewer).await;
    let (_, operator) = role_session(&pool, AuthRole::Operator).await;
    let (_, admin) = role_session(&pool, AuthRole::Admin).await;
    let base = poam_http_server(pool.clone()).await;
    let client = reqwest::Client::new();

    let viewer_response = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/poams/assignees"),
        &viewer,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(viewer_response.status(), reqwest::StatusCode::FORBIDDEN);
    for token in [&operator, &admin] {
        let response = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/poams/assignees"),
            token,
            None,
        )
        .send()
        .await
        .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body["people"].as_array().unwrap().windows(2).all(|rows| {
            let left = rows[0]["label"].as_str().unwrap().to_ascii_lowercase();
            let right = rows[1]["label"].as_str().unwrap().to_ascii_lowercase();
            left <= right
        }));
        assert!(body["groups"].as_array().unwrap().windows(2).all(|rows| {
            rows[0]["group_name"].as_str().unwrap() <= rows[1]["group_name"].as_str().unwrap()
        }));
        let person = body["people"]
            .as_array()
            .unwrap()
            .iter()
            .find(|person| person["user_id"] == assigned_user.id.to_string())
            .unwrap();
        assert_eq!(person.as_object().unwrap().len(), 2);
        let group = body["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|group| group["group_name"] == "catalog:group")
            .unwrap();
        assert_eq!(group.as_object().unwrap().len(), 1);
        assert!(!body.to_string().contains("sensitive-environment-name"));
        assert!(!body.to_string().contains("operator"));
    }

    // SECURITY: Assignment metadata must not create environment membership or
    // otherwise become an environment-visibility grant.
    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_environment_memberships WHERE user_id=$1 AND environment_id=$2",
    )
    .bind(assigned_user.id)
    .bind(hidden_environment)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(membership_count, 0);
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

async fn seal_exact_cve_scan(
    pool: &PgPool,
    fixture: &AssessmentFixture,
    completed_at: DateTime<Utc>,
    occurrence: Option<(&str, &str, &str, bool)>,
) -> Option<CveObservationReference> {
    seal_exact_cve_scan_for_derivation(
        pool,
        fixture.system_id,
        fixture.derivation_id,
        completed_at,
        occurrence,
    )
    .await
}

async fn seal_exact_cve_scan_for_derivation(
    pool: &PgPool,
    system_id: Uuid,
    derivation_id: i32,
    completed_at: DateTime<Utc>,
    occurrence: Option<(&str, &str, &str, bool)>,
) -> Option<CveObservationReference> {
    let occurrences = occurrence.into_iter().collect::<Vec<_>>();
    seal_exact_cve_scan_many_for_derivation(
        pool,
        system_id,
        derivation_id,
        completed_at,
        &occurrences,
    )
    .await
    .into_iter()
    .next()
}

async fn seal_exact_cve_scan_many(
    pool: &PgPool,
    fixture: &AssessmentFixture,
    completed_at: DateTime<Utc>,
    occurrences: &[(&str, &str, &str, bool)],
) -> Vec<CveObservationReference> {
    seal_exact_cve_scan_many_for_derivation(
        pool,
        fixture.system_id,
        fixture.derivation_id,
        completed_at,
        occurrences,
    )
    .await
}

async fn seal_exact_cve_scan_many_for_derivation(
    pool: &PgPool,
    system_id: Uuid,
    derivation_id: i32,
    completed_at: DateTime<Utc>,
    occurrences: &[(&str, &str, &str, bool)],
) -> Vec<CveObservationReference> {
    let scan_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO cve_scans(
             id,derivation_id,scanner_name,status,total_packages,total_vulnerabilities)
           VALUES($1,$2,'test-scanner','in_progress',$3,$4)"#,
    )
    .bind(scan_id)
    .bind(derivation_id)
    .bind(occurrences.len() as i32)
    .bind(occurrences.len() as i32)
    .execute(pool)
    .await
    .expect("insert active exact CVE scan");

    let mut references = Vec::with_capacity(occurrences.len());
    for &(cve_id, package_name, package_version, is_whitelisted) in occurrences {
        sqlx::query("INSERT INTO cves(id) VALUES($1) ON CONFLICT(id) DO NOTHING")
            .bind(cve_id)
            .execute(pool)
            .await
            .expect("insert exact CVE identity");
        let suffix = Uuid::new_v4().simple().to_string();
        let occurrence_derivation_path = format!("/nix/store/{suffix}-{package_name}.drv");
        let persisted_package_version = package_version.chars().take(100).collect::<String>();
        let package_derivation_id: i32 = sqlx::query_scalar(
            r#"INSERT INTO derivations(
                 commit_id,derivation_type,derivation_name,derivation_path,
                 pname,version,status_id,attempt_count)
               VALUES(NULL,'package',$1,$2,$3,$4,11,0) RETURNING id"#,
        )
        .bind(format!("{package_name}-{package_version}-{suffix}"))
        .bind(&occurrence_derivation_path)
        .bind(package_name)
        .bind(persisted_package_version)
        .fetch_one(pool)
        .await
        .expect("insert exact CVE package derivation");
        sqlx::query("INSERT INTO scan_packages(scan_id,derivation_id) VALUES($1,$2)")
            .bind(scan_id)
            .bind(package_derivation_id)
            .execute(pool)
            .await
            .expect("link exact CVE scan package");
        sqlx::query(
            r#"INSERT INTO cve_scan_vulnerability_observations(
                 scan_id,canonical_cve_id,
                 canonical_package_name,observed_package_name,
                 observed_package_version,observed_derivation_path,
                 is_whitelisted,whitelist_reason,detection_method)
               VALUES($1,$2,$3,$3,$4,$5,$6,$7,'test-scanner')"#,
        )
        .bind(scan_id)
        .bind(cve_id)
        .bind(package_name)
        .bind(package_version)
        .bind(&occurrence_derivation_path)
        .bind(is_whitelisted)
        .bind(is_whitelisted.then_some("accepted by scanner policy"))
        .execute(pool)
        .await
        .expect("insert exact CVE occurrence");
        references.push(CveObservationReference {
            system_id,
            scan_id,
            occurrence_derivation_path,
            canonical_cve_id: cve_id.into(),
            canonical_package_name: package_name.into(),
        });
    }

    sqlx::query(
        r#"UPDATE cve_scans
           SET status='completed',completed_at=$2,evidence_schema_version=1
           WHERE id=$1"#,
    )
    .bind(scan_id)
    .bind(completed_at)
    .execute(pool)
    .await
    .expect("seal exact CVE scan");
    references
}

async fn begin_exact_cve_scan(pool: &PgPool, fixture: &AssessmentFixture, total: i32) -> Uuid {
    let scan_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO cve_scans(
             id,derivation_id,scanner_name,status,total_packages,total_vulnerabilities)
           VALUES($1,$2,'test-scanner','in_progress',$3,$3)"#,
    )
    .bind(scan_id)
    .bind(fixture.derivation_id)
    .bind(total)
    .execute(pool)
    .await
    .expect("insert active exact CVE scan");
    scan_id
}

async fn complete_exact_cve_scan(pool: &PgPool, scan_id: Uuid) {
    sqlx::query(
        "UPDATE cve_scans SET status='completed',completed_at=clock_timestamp(),evidence_schema_version=1 WHERE id=$1",
    )
    .bind(scan_id)
    .execute(pool)
    .await
    .expect("seal generated exact CVE scan");
}

async fn assign_environment(pool: &PgPool, name: &str, fixtures: &[&AssessmentFixture]) -> Uuid {
    let environment_id: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap();
    let system_ids = fixtures
        .iter()
        .map(|fixture| fixture.system_id)
        .collect::<Vec<_>>();
    sqlx::query("UPDATE systems SET environment_id=$1 WHERE id=ANY($2)")
        .bind(environment_id)
        .bind(&system_ids)
        .execute(pool)
        .await
        .unwrap();
    environment_id
}

async fn add_scheduled_cve_target(
    pool: &PgPool,
    fixture: &AssessmentFixture,
    occurrence: Option<(&str, &str, &str, bool)>,
) -> Uuid {
    let (hostname, flake_id, repository): (String, i32, String) = sqlx::query_as(
        r#"SELECT system.hostname,flake.id,flake.repo_url
           FROM systems system JOIN flakes flake ON flake.id=system.flake_id
           WHERE system.id=$1"#,
    )
    .bind(fixture.system_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let commit_hash = Uuid::new_v4().simple().to_string();
    insert_commit(pool, &commit_hash, &repository, Utc::now())
        .await
        .unwrap();
    let commit_id: i32 =
        sqlx::query_scalar("SELECT id FROM commits WHERE flake_id=$1 AND git_commit_hash=$2")
            .bind(flake_id)
            .bind(&commit_hash)
            .fetch_one(pool)
            .await
            .unwrap();
    let store_path = format!("/nix/store/{}-scheduled-target", Uuid::new_v4().simple());
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
        | SuccessfulEvalWrite::PreservedBuildState { derivation_id, .. }
        | SuccessfulEvalWrite::LegacyPathConflict { derivation_id } => derivation_id,
    };
    sqlx::query("UPDATE derivations SET store_path=$1,status_id=10 WHERE id=$2")
        .bind(&store_path)
        .bind(derivation_id)
        .execute(pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let snapshot_id = persist_available_snapshot_tx(&mut tx, commit_id, &hostname, Vec::new())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    seal_exact_cve_scan_for_derivation(
        pool,
        fixture.system_id,
        derivation_id,
        Utc::now(),
        occurrence,
    )
    .await;
    sqlx::query_scalar(
        r#"INSERT INTO pending_system_deployments(
             system_id,target_store_path,status,expires_at,requested_commit_id,
             requested_derivation_id,evaluation_snapshot_id,
             evaluation_snapshot_binding_expected)
           VALUES($1,$2,'pending',now()+interval '1 hour',$3,$4,$5,true)
           RETURNING id"#,
    )
    .bind(fixture.system_id)
    .bind(&store_path)
    .bind(commit_id)
    .bind(derivation_id)
    .bind(snapshot_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn add_historical_cve_inventory(
    pool: &PgPool,
    fixture: &AssessmentFixture,
    cve_id: &str,
    package_name: &str,
) {
    sqlx::query("INSERT INTO cves(id) VALUES($1) ON CONFLICT(id) DO NOTHING")
        .bind(cve_id)
        .execute(pool)
        .await
        .unwrap();
    let scan_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO cve_scans(
             derivation_id,scanner_name,status,completed_at,evidence_schema_version)
           VALUES($1,'historical-test','completed',now(),0) RETURNING id"#,
    )
    .bind(fixture.derivation_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let package_derivation_id: i32 = sqlx::query_scalar(
        r#"INSERT INTO derivations(
             derivation_name,derivation_path,derivation_type,status_id,pname,version)
           VALUES($1,$2,'package',10,$3,'0.9') RETURNING id"#,
    )
    .bind(format!("{package_name}-historical"))
    .bind(format!(
        "/nix/store/{}-{package_name}.drv",
        Uuid::new_v4().simple()
    ))
    .bind(package_name)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO scan_packages(scan_id,derivation_id) VALUES($1,$2)")
        .bind(scan_id)
        .bind(package_derivation_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO package_vulnerabilities(
             derivation_id,cve_id,is_whitelisted,detection_method)
           VALUES($1,$2,false,'historical-test')"#,
    )
    .bind(package_derivation_id)
    .bind(cve_id)
    .execute(pool)
    .await
    .unwrap();
}

fn fleet_poam_request(actor_id: Uuid, clock: &FixedClock) -> FleetCvePoamRequest {
    FleetCvePoamRequest {
        title: "Remediate fleet OpenSSL CVE".into(),
        plan: "Deploy the fixed package through the normal environment promotion path".into(),
        assignee: PoamAssigneeRequest::User { user_id: actor_id },
        target_date: clock.today() + chrono::Duration::days(30),
        risk: PoamRisk::High,
        default_milestones: true,
    }
}

async fn assert_cve_authority_surfaces(
    pool: &PgPool,
    cve_id: &str,
    package_name: &str,
    expected_status: &str,
) {
    let scope = CveReadScope::All;
    let rows = fetch_cve_list(pool, &scope, &CveFilters::default())
        .await
        .unwrap();
    let row = rows
        .iter()
        .find(|row| row.cve_id == cve_id && row.package_name.as_deref() == Some(package_name))
        .unwrap();
    assert_eq!(row.triage_status, expected_status);

    let filtered = fetch_cve_list(
        pool,
        &scope,
        &CveFilters {
            triage_status: Some(expected_status.to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].cve_id, cve_id);

    let groups = fetch_cve_packages_grouped(pool, &scope, &CveFilters::default())
        .await
        .unwrap();
    let group = groups
        .iter()
        .find(|group| group.package_name == package_name)
        .unwrap();
    assert_eq!(
        group.outstanding_count,
        i64::from(expected_status == "outstanding")
    );

    let exported = fetch_cves_for_export(pool, &scope, &CveFilters::default())
        .await
        .unwrap();
    assert_eq!(exported.len(), 1);
    assert_eq!(exported[0].triage_status, expected_status);

    let stats = fetch_cve_fleet_stats(pool, &scope).await.unwrap();
    assert_eq!(stats.total_cves, 1);
    assert_eq!(stats.scheduled, i64::from(expected_status == "scheduled"));
    assert_eq!(
        stats.outstanding,
        i64::from(expected_status == "outstanding")
    );
}

async fn assert_cve_open_everywhere(
    pool: &PgPool,
    actor: &PoamActor,
    cve_id: &str,
    package_name: &str,
) {
    assert_cve_authority_surfaces(pool, cve_id, package_name, "outstanding").await;
    let detail = poam_service::fleet_cve_detail(pool, actor, cve_id, package_name)
        .await
        .unwrap();
    assert_eq!(detail.rollup, FleetCveTriageRollup::Outstanding);
    assert!(
        detail
            .environments
            .iter()
            .all(|environment| environment.disposition.is_none()),
        "incoherent scheduled persistence must fail closed as OPEN"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn environment_move_retains_mixed_history_and_separates_visible_poams(pool: PgPool) {
    let moved = assessment_fixture(&pool).await;
    let overridden = assessment_fixture(&pool).await;
    let actor = admin_actor(moved.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve = "CVE-2026-32704";
    let package = "move-history-package";
    let environment_a = assign_environment(&pool, "move-source-a", &[&moved, &overridden]).await;
    let environment_b = assign_environment(&pool, "move-destination-b", &[]).await;
    let (hostname, configuration, flake_name): (String, Option<String>, String) = sqlx::query_as(
        "SELECT system.hostname,system.system_configuration_name,flake.name FROM systems system JOIN flakes flake ON flake.id=system.flake_id WHERE system.id=$1",
    )
    .bind(moved.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    for subject in [&moved, &overridden] {
        seal_exact_cve_scan(
            &pool,
            subject,
            clock.now(),
            Some((cve, package, "1.0", false)),
        )
        .await;
    }
    let scheduled_a = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve,
        FleetCveTriageRequest {
            canonical_package_name: package.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch {
                environment_id: environment_a,
            }],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let a_poam = scheduled_a.poam_id.unwrap();
    let baseline: (Uuid, Uuid, Option<Uuid>, String) = sqlx::query_as(
        "SELECT id,baseline_scan_id,baseline_generation_snapshot_id,baseline_observed_package_version FROM poam_cve_finding_links WHERE poam_id=$1 AND system_id=$2 AND retired_at IS NULL",
    )
    .bind(a_poam)
    .bind(moved.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let host_override = poam_service::triage_system_cve(
        &pool,
        &actor,
        overridden.system_id,
        cve,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "Independent host controls survive the move".into(),
                review_date: None,
            },
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        host_override.detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
    let nonmove: (String, bool) = sqlx::query_as(
        "SELECT retirement_reason,retired_at IS NOT NULL FROM poam_cve_finding_links WHERE poam_id=$1 AND system_id=$2 ORDER BY linked_at LIMIT 1",
    )
    .bind(a_poam)
    .bind(overridden.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(nonmove.1);
    assert_ne!(nonmove.0, "environment_moved");

    let base = poam_http_server(pool.clone()).await;
    let token = session(&pool, actor.user_id, AuthRole::Admin).await;
    let client = reqwest::Client::new();
    let patch = http_request(
        &client,
        reqwest::Method::PATCH,
        format!("{base}/api/v1/systems/{}", moved.system_id),
        &token,
        Some("move-history-csrf"),
    )
    .json(&serde_json::json!({
        "hostname": hostname,
        "system_configuration_name": configuration,
        "environment": "move-destination-b",
        "flake_name": flake_name,
        "deployment_policy": "manual"
    }))
    .send()
    .await
    .unwrap();
    assert_eq!(
        patch.status(),
        reqwest::StatusCode::OK,
        "{}",
        patch.text().await.unwrap()
    );
    let moved_link: (Uuid, Option<DateTime<Utc>>, String, Uuid, Option<Uuid>, String) = sqlx::query_as(
        "SELECT id,retired_at,retirement_reason,baseline_scan_id,baseline_generation_snapshot_id,baseline_observed_package_version FROM poam_cve_finding_links WHERE id=$1",
    )
    .bind(baseline.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(moved_link.0, baseline.0);
    assert!(moved_link.1.is_some());
    assert_eq!(moved_link.2, "environment_moved");
    assert_eq!(
        (moved_link.3, moved_link.4, moved_link.5),
        (baseline.1, baseline.2, baseline.3)
    );
    let a_detail = poam_service::detail(&pool, &actor, a_poam, &clock)
        .await
        .unwrap();
    assert_ne!(a_detail.poam.status, "completed");
    assert_eq!(
        a_detail
            .cve_findings
            .iter()
            .filter(|finding| finding.link_active)
            .count(),
        0
    );
    assert_eq!(a_detail.cve_findings.len(), 2);
    let live_a: (Uuid, String) = sqlx::query_as(
        "SELECT poam_id,state FROM cve_current_environment_dispositions WHERE environment_id=$1 AND canonical_cve_id=$2 AND canonical_package_name=$3",
    )
    .bind(environment_a)
    .bind(cve)
    .bind(package)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(live_a, (a_poam, "scheduled".into()));
    assert_eq!(
        poam_service::system_cve_triage_detail(&pool, &actor, overridden.system_id, cve, package)
            .await
            .unwrap()
            .effective_source,
        SystemCveEffectiveDispositionSource::Host
    );

    let scheduled_b = poam_service::triage_system_cve(
        &pool,
        &actor,
        moved.system_id,
        cve,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let b_poam = scheduled_b.poam_id.unwrap();
    assert_ne!(b_poam, a_poam);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND system_id=$2 AND retired_at IS NULL"
        )
        .bind(b_poam)
        .bind(moved.system_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );

    let a_reader = assessment_fixture(&pool).await;
    sync_user_role(&pool, a_reader.user_id, AuthRole::Viewer)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(a_reader.user_id)
        .bind(environment_a)
        .execute(&pool)
        .await
        .unwrap();
    let scoped = PoamActor {
        user_id: a_reader.user_id,
        identifier: "a-only-reader".into(),
        is_admin: false,
        can_mutate: false,
        environment_ids: vec![environment_a],
        request_origin: None,
    };
    assert_eq!(
        poam_service::detail(&pool, &scoped, a_poam, &clock)
            .await
            .unwrap()
            .poam
            .id,
        a_poam
    );
    assert!(matches!(
        poam_service::detail(&pool, &scoped, b_poam, &clock).await,
        Err(PoamError::NotFound)
    ));
    let scoped_list = poam_service::list(&pool, &scoped, &PoamListQuery::default(), &clock)
        .await
        .unwrap();
    assert!(scoped_list.items.iter().any(|item| item.id == a_poam));
    assert!(!scoped_list.items.iter().any(|item| item.id == b_poam));
    let searched = poam_service::list(
        &pool,
        &scoped,
        &PoamListQuery {
            q: Some(hostname),
            ..Default::default()
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(
        searched.items.is_empty(),
        "A-only search cannot disclose B's current hostname"
    );
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT cve_coherent_environment_disposition_state($1,$2,$3)"
        )
        .bind(cve)
        .bind(package)
        .bind(environment_b)
        .fetch_one(&pool)
        .await
        .unwrap(),
        Some("scheduled".into())
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn scheduled_environment_reconciles_and_verifies_over_one_hundred_members(pool: PgPool) {
    let first = assessment_fixture(&pool).await;
    let actor = admin_actor(first.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve = "CVE-2026-32705";
    let package = "paged-continuity-package";
    let environment = assign_environment(&pool, "paged-continuity-environment", &[&first]).await;
    let initial = seal_exact_cve_scan(
        &pool,
        &first,
        clock.now() - TimeDelta::minutes(2),
        Some((cve, package, "1.0", false)),
    )
    .await
    .unwrap();
    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve,
        FleetCveTriageRequest {
            canonical_package_name: package.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch {
                environment_id: environment,
            }],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    let (hostname, flake_id): (String, i32) =
        sqlx::query_as("SELECT hostname,flake_id FROM systems WHERE id=$1")
            .bind(first.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let prefix = format!("page-{}-", Uuid::new_v4().simple());
    let additional_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"INSERT INTO systems(hostname,is_active,public_key,derivation,reachability,
                               flake_id,system_configuration_name,environment_id)
           SELECT $1 || series.n,true,'page-key-' || $1 || series.n,
                  'page-key-' || $1 || series.n,'direct',$2,$3,$4
           FROM generate_series(1,120) AS series(n) RETURNING id"#,
    )
    .bind(&prefix)
    .bind(flake_id)
    .bind(&hostname)
    .bind(environment)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(additional_ids.len(), 120);
    sqlx::query(
        r#"INSERT INTO system_states(hostname,change_reason,store_path,generation,
                                     generation_matches_current_store_path,timestamp)
           SELECT hostname,'startup',$2,1,true,clock_timestamp() + interval '1 minute'
           FROM systems WHERE id=ANY($1)"#,
    )
    .bind(&additional_ids)
    .bind(&first.store_path)
    .execute(&pool)
    .await
    .unwrap();
    let affected: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM view_current_exact_cve_occurrences WHERE cve_id=$1 AND package_name=$2 AND environment_id=$3",
    )
    .bind(cve)
    .bind(package)
    .bind(environment)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(affected, 121);

    assert_eq!(
        poam_service::reconcile_scheduled_environment_cve_page(&pool, None)
            .await
            .unwrap(),
        None
    );
    let first_page: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        first_page, 101,
        "one repair adds at most 100 missing subjects"
    );
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        poam_id,
        TransitionPoamRequest {
            revision: 1,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let rejected = poam_service::verify(&pool, &actor, poam_id, awaiting.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(rejected["outcome"], "rejected");
    assert_eq!(rejected["cve_items"].as_array().unwrap().len(), 101);
    assert!(matches!(
        poam_service::close(
            &pool,
            &actor,
            poam_id,
            rejected["revision"].as_i64().unwrap(),
            &clock
        )
        .await,
        Err(PoamError::Precondition("closure_not_ready", _, _))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL"
        )
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        101
    );

    poam_service::reconcile_scheduled_environment_cve_page(&pool, None)
        .await
        .unwrap();
    poam_service::reconcile_scheduled_environment_cve_page(&pool, None)
        .await
        .unwrap();
    let linked: (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(DISTINCT system_id) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(linked, (121, 121));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM poams WHERE id=$1")
            .bind(poam_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    seal_exact_cve_scan(&pool, &first, clock.now() + TimeDelta::minutes(2), None).await;
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM poams WHERE id=$1")
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let passed = poam_service::verify(&pool, &actor, poam_id, revision, &clock)
        .await
        .unwrap();
    assert_eq!(passed["outcome"], "accepted");
    assert_eq!(passed["cve_items"].as_array().unwrap().len(), 121);
    assert!(
        passed["cve_items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["result"] == "pass")
    );
    let passed_id = Uuid::parse_str(passed["attempt_id"].as_str().unwrap()).unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM poam_cve_verification_items WHERE attempt_id=$1 AND result='pass' AND baseline_scan_id=$2"
        ).bind(passed_id).bind(initial.scan_id).fetch_one(&pool).await.unwrap(),
        121
    );
    let closed = poam_service::close(
        &pool,
        &actor,
        poam_id,
        passed["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(closed.poam.status, "completed");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL"
        )
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

async fn replay_retired_cve_link(
    pool: &PgPool,
    link_id: Uuid,
    cve_id: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    // CONCURRENCY: Production writers take the canonical CVE key before
    // reading link rows. The direct-SQL test must follow the same order while
    // an asynchronous disposition reconciliation may be running.
    sqlx::query("SELECT lock_poam_cve_key($1)")
        .bind(cve_id)
        .execute(&mut *tx)
        .await?;
    let inserted = sqlx::query(
        r#"INSERT INTO poam_cve_finding_links(
             poam_id,cve_finding_id,system_id,canonical_cve_id,
             canonical_package_name,baseline_scan_id,baseline_scan_derivation_id,
             baseline_scan_completed_at,baseline_generation_snapshot_id,
             baseline_generation,baseline_target_store_path,
             baseline_occurrence_derivation_path,baseline_observed_package_version,linked_by)
           SELECT poam_id,cve_finding_id,system_id,canonical_cve_id,
                  canonical_package_name,baseline_scan_id,baseline_scan_derivation_id,
                  baseline_scan_completed_at,baseline_generation_snapshot_id,
                  baseline_generation,baseline_target_store_path,
                  baseline_occurrence_derivation_path,baseline_observed_package_version,linked_by
           FROM poam_cve_finding_links WHERE id=$1"#,
    )
    .bind(link_id)
    .execute(&mut *tx)
    .await;
    tx.rollback().await?;
    inserted.map(|_| ())
}

#[sqlx::test(migrations = "./migrations")]
async fn retired_nonclosure_cve_link_replay_requires_current_and_no_accepted_host(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve = "CVE-2026-32700";
    let package = "replay-guard-package";
    let environment =
        assign_environment(&pool, "replay-guard-environment", &[&fixture, &peer]).await;
    for subject in [&fixture, &peer] {
        seal_exact_cve_scan(
            &pool,
            subject,
            clock.now(),
            Some((cve, package, "1.0", false)),
        )
        .await;
    }
    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve,
        FleetCveTriageRequest {
            canonical_package_name: package.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch {
                environment_id: environment,
            }],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    let link: (Uuid, Uuid, Option<Uuid>) = sqlx::query_as(
        "SELECT id,baseline_scan_id,baseline_generation_snapshot_id FROM poam_cve_finding_links WHERE poam_id=$1 AND system_id=$2 AND retired_at IS NULL",
    )
    .bind(poam_id)
    .bind(fixture.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(link.2.is_some());
    sqlx::query(
        "UPDATE poam_cve_finding_links SET retired_at=now(),retired_by=$2,retirement_reason='operator_unlinked' WHERE id=$1",
    )
    .bind(link.0)
    .bind(actor.user_id)
    .execute(&pool)
    .await
    .unwrap();

    let accepted = poam_service::triage_system_cve(
        &pool,
        &actor,
        fixture.system_id,
        cve,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "Independently accepted host risk".into(),
                review_date: None,
            },
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        accepted.detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
    let before_replay: (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER (WHERE retired_at IS NULL) FROM poam_cve_finding_links WHERE poam_id=$1",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let rejected = replay_retired_cve_link(&pool, link.0, cve)
        .await
        .unwrap_err();
    assert_eq!(
        rejected
            .as_database_error()
            .and_then(|error| error.constraint()),
        Some("poam_cve_link_replay_accepted_host_risk"),
        "unexpected replay rejection: {rejected:?}"
    );
    sqlx::query("UPDATE poams SET target_date=NULL WHERE id=$1")
        .bind(poam_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE cve_system_dispositions SET retired_at=now(),retired_by=$4,retirement_reason='operator_cleared' WHERE system_id=$1 AND canonical_cve_id=$2 AND canonical_package_name=$3 AND retired_at IS NULL",
    )
    .bind(fixture.system_id)
    .bind(cve)
    .bind(package)
    .bind(actor.user_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        poam_service::system_cve_triage_detail(&pool, &actor, fixture.system_id, cve, package)
            .await
            .unwrap()
            .host_disposition
            .is_none()
    );
    seal_exact_cve_scan(&pool, &fixture, clock.now() + TimeDelta::minutes(1), None).await;
    let rejected = replay_retired_cve_link(&pool, link.0, cve)
        .await
        .unwrap_err();
    assert_eq!(
        rejected
            .as_database_error()
            .and_then(|error| error.constraint()),
        Some("poam_cve_link_replay_current_baseline")
    );
    let (_, newer) = external_cve_revision(
        &pool,
        &fixture,
        2,
        clock.now() + TimeDelta::minutes(2),
        Some((cve, package, "2.0", false)),
    )
    .await;
    assert!(newer.is_some());
    let rejected = replay_retired_cve_link(&pool, link.0, cve)
        .await
        .unwrap_err();
    assert_eq!(
        rejected
            .as_database_error()
            .and_then(|error| error.constraint()),
        Some("poam_cve_link_replay_current_baseline")
    );
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER (WHERE retired_at IS NULL) FROM poam_cve_finding_links WHERE poam_id=$1",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, before_replay);
    assert_eq!(counts.1, 1, "only the peer's link stays active");
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT baseline_scan_id FROM poam_cve_finding_links WHERE id=$1"
        )
        .bind(link.0)
        .fetch_one(&pool)
        .await
        .unwrap(),
        link.1
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn unavailable_group_blocks_new_scheduled_environment_membership(pool: PgPool) {
    let initial = assessment_fixture(&pool).await;
    let newcomer = assessment_fixture(&pool).await;
    let actor = admin_actor(initial.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve = "CVE-2026-32701";
    let package = "unavailable-owner-package";
    let group = format!("continuity:group-{}", Uuid::new_v4().simple());
    sqlx::query("INSERT INTO oidc_group_mappings(group_name) VALUES($1)")
        .bind(&group)
        .execute(&pool)
        .await
        .unwrap();
    let environment = assign_environment(
        &pool,
        "unavailable-owner-environment",
        &[&initial, &newcomer],
    )
    .await;
    seal_exact_cve_scan(
        &pool,
        &initial,
        clock.now(),
        Some((cve, package, "1.0", false)),
    )
    .await;
    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve,
        FleetCveTriageRequest {
            canonical_package_name: package.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch {
                environment_id: environment,
            }],
            poam: Some(FleetCvePoamRequest {
                assignee: PoamAssigneeRequest::OidcGroup {
                    group_name: group.clone(),
                },
                ..fleet_poam_request(actor.user_id, &clock)
            }),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    sqlx::query("DELETE FROM oidc_group_mappings WHERE group_name=$1")
        .bind(&group)
        .execute(&pool)
        .await
        .unwrap();
    seal_exact_cve_scan(
        &pool,
        &newcomer,
        clock.now() + TimeDelta::minutes(1),
        Some((cve, package, "1.1", false)),
    )
    .await;
    poam_service::reconcile_scheduled_environment_cves_for_system(&pool, newcomer.system_id)
        .await
        .unwrap();
    assert_eq!(
        poam_service::reconcile_scheduled_environment_cve_page(&pool, None)
            .await
            .unwrap(),
        None
    );
    let linked: Vec<Uuid> = sqlx::query_scalar(
        "SELECT system_id FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(poam_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(linked, vec![initial.system_id]);
    assert_eq!(
        poam_service::fleet_cve_detail(&pool, &actor, cve, package)
            .await
            .unwrap()
            .rollup,
        FleetCveTriageRollup::Outstanding
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn scheduled_environment_reconciliation_keeps_history_and_adds_only_current_subjects(
    pool: PgPool,
) {
    let sledge = assessment_fixture(&pool).await;
    let gray = assessment_fixture(&pool).await;
    let webb = assessment_fixture(&pool).await;
    let daly = assessment_fixture(&pool).await;
    let inconsistent = assessment_fixture(&pool).await;
    let actor = admin_actor(sledge.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve = "CVE-2026-32699";
    let package = "continuity-environment-package";
    let environment = assign_environment(
        &pool,
        "continuity-repair-environment",
        &[&sledge, &gray, &webb, &daly, &inconsistent],
    )
    .await;
    for fixture in [&sledge, &gray, &webb] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve, package, "1.0", false)),
        )
        .await;
    }
    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve,
        FleetCveTriageRequest {
            canonical_package_name: package.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch {
                environment_id: environment,
            }],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    assert_eq!(scheduled.detail.exact_mutation_target_count, 3);
    let original_links: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
        "SELECT system_id,cve_finding_id,baseline_scan_id FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL ORDER BY system_id",
    )
    .bind(poam_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(original_links.len(), 3);

    // A current evaluation artifact is not required for the newly affected
    // system. Its exact scan, not retained Config provenance, opens the link.
    sqlx::query("DELETE FROM evaluation_generation_snapshots WHERE system_id=$1")
        .bind(daly.system_id)
        .execute(&pool)
        .await
        .unwrap();
    let daly_scan = seal_exact_cve_scan(
        &pool,
        &daly,
        clock.now() + TimeDelta::minutes(1),
        Some((cve, package, "2.0", false)),
    )
    .await
    .unwrap();
    seal_exact_cve_scan(&pool, &webb, clock.now() + TimeDelta::minutes(2), None).await;
    seal_exact_cve_scan(
        &pool,
        &inconsistent,
        clock.now() + TimeDelta::minutes(1),
        Some((cve, package, "9.0", false)),
    )
    .await;
    report_external_generation(&pool, &inconsistent, 2, &inconsistent.store_path, false, 1).await;
    let current_request =
        SystemCveInventoryPageRequest::from_params(SystemCveInventoryParams::default()).unwrap();
    assert!(
        fetch_system_cve_inventory_page(&pool, inconsistent.system_id, &current_request)
            .await
            .unwrap()
            .read_only
    );

    let (repair_a, repair_b) = tokio::join!(
        poam_service::reconcile_scheduled_environment_cves_for_system(&pool, daly.system_id),
        poam_service::reconcile_scheduled_environment_cves_for_system(&pool, daly.system_id)
    );
    repair_a.expect("first concurrent repair must finish");
    repair_b.expect("second concurrent repair must finish without duplicate links");
    assert_eq!(
        poam_service::reconcile_scheduled_environment_cve_page(&pool, None)
            .await
            .unwrap(),
        None
    );
    let links: Vec<(Uuid, Uuid, Uuid, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT system_id,cve_finding_id,baseline_scan_id,baseline_generation_snapshot_id,baseline_observed_package_version FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL ORDER BY system_id",
    )
    .bind(poam_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(links.len(), 4);
    assert!(links.iter().all(|row| row.0 != inconsistent.system_id));
    for (system_id, finding_id, scan_id) in &original_links {
        let current = links.iter().find(|row| row.0 == *system_id).unwrap();
        assert_eq!(current.1, *finding_id);
        assert_eq!(
            current.2, *scan_id,
            "history must keep its original baseline"
        );
    }
    let added = links.iter().find(|row| row.0 == daly.system_id).unwrap();
    assert_eq!(added.2, daly_scan.scan_id);
    assert_eq!(added.3, None);
    assert_eq!(added.4, "2.0");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM poams WHERE id=$1")
            .bind(poam_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let detail = poam_service::detail(&pool, &actor, poam_id, &clock)
        .await
        .unwrap();
    let historic = detail
        .cve_findings
        .iter()
        .find(|finding| finding.system_id == webb.system_id)
        .unwrap();
    assert!(historic.link_active);
    assert!(historic.current_scan_id.is_some());
    assert!(historic.current_occurrence_derivation_path.is_none());
    let current = poam_service::fleet_cve_detail(&pool, &actor, cve, package)
        .await
        .unwrap();
    assert_eq!(current.affected_system_count, 3);
    let persisted_state: Option<String> =
        sqlx::query_scalar("SELECT cve_coherent_environment_disposition_state($1,$2,$3)")
            .bind(cve)
            .bind(package)
            .bind(environment)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(persisted_state.as_deref(), Some("scheduled"));

    let accepted = poam_service::triage_system_cve(
        &pool,
        &actor,
        gray.system_id,
        cve,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "Gray has independently approved controls".into(),
                review_date: None,
            },
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        accepted.detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
    let gray_links_before: (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER (WHERE retired_at IS NULL) FROM poam_cve_finding_links WHERE poam_id=$1 AND system_id=$2",
    )
    .bind(poam_id)
    .bind(gray.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    poam_service::reconcile_scheduled_environment_cves_for_system(&pool, gray.system_id)
        .await
        .unwrap();
    poam_service::reconcile_scheduled_environment_cve_page(&pool, None)
        .await
        .unwrap();
    let gray_links_after: (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER (WHERE retired_at IS NULL) FROM poam_cve_finding_links WHERE poam_id=$1 AND system_id=$2",
    )
    .bind(poam_id)
    .bind(gray.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(gray_links_after, gray_links_before);
    assert_eq!(
        poam_service::system_cve_triage_detail(&pool, &actor, gray.system_id, cve, package)
            .await
            .unwrap()
            .effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
    let all_links: (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(DISTINCT system_id) FROM poam_cve_finding_links WHERE poam_id=$1",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(all_links, (4, 4));
    assert!(
        matches!(
            current.environments[0].disposition,
            Some(CveEnvironmentDisposition::Scheduled { poam_id: id, .. }) if id == poam_id
        ),
        "scheduled environment after repair: {:?}",
        current.environments
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn unlinked_environment_host_state_publication_precedes_cve_poam_closure(pool: PgPool) {
    let a = assessment_fixture(&pool).await;
    let b = assessment_fixture(&pool).await;
    let actor = admin_actor(a.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve = "CVE-2026-32704";
    let package = "unlinked-state-race-package";
    let environment = assign_environment(&pool, "unlinked-state-race", &[&a, &b]).await;
    seal_exact_cve_scan(
        &pool,
        &a,
        clock.now() - TimeDelta::minutes(2),
        Some((cve, package, "1.0", false)),
    )
    .await;
    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve,
        FleetCveTriageRequest {
            canonical_package_name: package.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch {
                environment_id: environment,
            }],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    let b_scan = seal_exact_cve_scan(
        &pool,
        &b,
        clock.now() - TimeDelta::minutes(1),
        Some((cve, package, "1.0", false)),
    )
    .await
    .unwrap();
    report_external_generation(&pool, &b, 2, &b.store_path, false, 0).await;
    let b_findings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM poam_cve_findings WHERE system_id=$1")
            .bind(b.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        b_findings, 0,
        "B must have no finding key for the state publisher to discover"
    );
    seal_exact_cve_scan(&pool, &a, clock.now(), None).await;
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        poam_id,
        TransitionPoamRequest {
            revision: 1,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();

    let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
    sqlx::query("UPDATE systems SET public_key=$2 WHERE id=$1")
        .bind(b.system_id)
        .bind(STANDARD.encode(signing_key.verifying_key().to_bytes()))
        .execute(&pool)
        .await
        .unwrap();
    let mut state: SystemState = sqlx::query_as(
        "SELECT *,NULL::text AS boot_id FROM system_states WHERE hostname=(SELECT hostname FROM systems WHERE id=$1) ORDER BY id DESC LIMIT 1",
    )
    .bind(b.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    state.id = None;
    state.timestamp = None;
    state.generation = Some(3);
    state.generation_matches_current_store_path = Some(true);
    let hostname = state.hostname.clone();
    let body = serde_json::to_vec(&state).unwrap();
    let signature = STANDARD.encode(signing_key.sign(&body).to_bytes());
    let base = poam_http_server(pool.clone()).await;
    let client = reqwest::Client::new();

    // CONCURRENCY: Hold B's sentinel after the canonical key so ingestion
    // waits inside its real transaction while closure queues behind its key.
    let mut sentinel = pool.begin().await.unwrap();
    sqlx::query("SELECT lock_poam_finding_key($1,$2)")
        .bind(b.system_id)
        .bind(Uuid::nil())
        .execute(&mut *sentinel)
        .await
        .unwrap();
    let gate = Arc::new(tokio::sync::Barrier::new(2));
    let request_gate = Arc::clone(&gate);
    let mut publication = tokio::spawn(async move {
        request_gate.wait().await;
        client
            .post(format!("{base}/current-system"))
            .header("x-key-id", hostname)
            .header("x-signature", signature)
            .body(body)
            .send()
            .await
    });
    gate.wait().await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let acquired: bool = sqlx::query_scalar(
                "SELECT pg_try_advisory_xact_lock(hashtextextended($1::text,439))",
            )
            .bind(cve)
            .fetch_one(&pool)
            .await
            .unwrap();
            if !acquired {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("agent state publisher did not acquire the scheduled CVE key");
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut publication)
            .await
            .is_err(),
        "state publication must wait for B's sentinel"
    );

    let close_pool = pool.clone();
    let close_actor = actor.clone();
    let close_clock = clock.clone();
    let mut closure = tokio::spawn(async move {
        poam_service::close(
            &close_pool,
            &close_actor,
            poam_id,
            awaiting.poam.revision,
            &close_clock,
        )
        .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut closure)
            .await
            .is_err(),
        "closure must wait for the state publisher's scheduled CVE key"
    );
    sentinel.commit().await.unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), publication)
        .await
        .expect("state publication did not finish")
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let closed = tokio::time::timeout(Duration::from_secs(5), closure)
        .await
        .expect("closure did not finish after state publication")
        .unwrap();
    assert!(
        matches!(
            closed,
            Err(PoamError::Precondition("closure_not_ready", _, _))
                | Err(PoamError::Conflict("concurrent_finding_change", _))
        ),
        "closure must not accept stale unlinked B: {closed:?}"
    );
    let current: Option<Uuid> = sqlx::query_scalar(
        "SELECT scan_id FROM view_current_exact_cve_occurrences WHERE system_id=$1 AND cve_id=$2 AND package_name=$3",
    )
    .bind(b.system_id)
    .bind(cve)
    .bind(package)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(current, Some(b_scan.scan_id));
    let status: String = sqlx::query_scalar("SELECT status FROM poams WHERE id=$1")
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "awaiting_verification");
}

#[sqlx::test(migrations = "./migrations")]
async fn fleet_cve_triage_is_atomic_environment_scoped_and_semantically_idempotent(pool: PgPool) {
    let first = assessment_fixture(&pool).await;
    let second = assessment_fixture(&pool).await;
    let third = assessment_fixture(&pool).await;
    let actor = admin_actor(first.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    sync_user_role(&pool, second.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44010";
    let package_name = "openssl";
    let first_environment =
        assign_environment(&pool, "fleet-triage-production", &[&first, &second]).await;
    let second_environment = assign_environment(&pool, "fleet-triage-staging", &[&third]).await;
    for fixture in [&first, &second, &third] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "3.0.10", false)),
        )
        .await;
    }

    let open = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert_eq!(open.rollup, FleetCveTriageRollup::Outstanding);
    assert_eq!(open.affected_system_count, 3);
    assert_eq!(open.environments.len(), 2);
    assert_eq!(
        open.environments
            .iter()
            .find(|environment| environment.environment_id == first_environment)
            .unwrap()
            .affected_system_count,
        2
    );

    for actions in [
        vec![
            CveEnvironmentTriageAction::LeaveOpen {
                environment_id: first_environment,
            },
            CveEnvironmentTriageAction::LeaveOpen {
                environment_id: first_environment,
            },
            CveEnvironmentTriageAction::LeaveOpen {
                environment_id: second_environment,
            },
        ],
        vec![
            CveEnvironmentTriageAction::LeaveOpen {
                environment_id: first_environment,
            },
            CveEnvironmentTriageAction::LeaveOpen {
                environment_id: second_environment,
            },
            CveEnvironmentTriageAction::LeaveOpen {
                environment_id: Uuid::new_v4(),
            },
        ],
    ] {
        let rejected = poam_service::triage_fleet_cve(
            &pool,
            &actor,
            cve_id,
            FleetCveTriageRequest {
                canonical_package_name: package_name.into(),
                actions,
                poam: None,
            },
            &clock,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            rejected,
            PoamError::Conflict("cve_evidence_changed", _)
        ));
    }

    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: first_environment,
                },
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: second_environment,
                },
            ],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    assert!(!scheduled.poam_reused);
    assert_eq!(
        scheduled.detail_scope,
        FleetCveMutationDetailScope::ExactMutationSubjects
    );
    assert_eq!(scheduled.detail.exact_mutation_target_count, 3);
    assert_eq!(scheduled.detail.rollup, FleetCveTriageRollup::Scheduled);
    for environment in &scheduled.detail.environments {
        let Some(CveEnvironmentDisposition::Scheduled {
            poam_id: disposition_poam_id,
            poam,
            ..
        }) = environment.disposition.as_ref()
        else {
            panic!("scheduled response must include a scheduled disposition");
        };
        let poam = poam.as_ref().expect("new server returns complete metadata");
        assert_eq!(*disposition_poam_id, poam_id);
        assert_eq!(poam.id, poam_id);
        assert!(poam.human_id.starts_with("POAM-"));
        assert_eq!(poam.title, fleet_poam_request(actor.user_id, &clock).title);
        assert_eq!(poam.plan, fleet_poam_request(actor.user_id, &clock).plan);
        assert_eq!(
            poam.target_date,
            fleet_poam_request(actor.user_id, &clock).target_date
        );
        assert_eq!(poam.risk, PoamRisk::High);
        assert!(matches!(
            &poam.assignee,
            PoamAssigneeView::User {
                user_id,
                available: true,
                ..
            } if *user_id == actor.user_id
        ));
    }
    let active_links: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_links, 3);
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "scheduled").await;

    let repeated = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: first_environment,
                },
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: second_environment,
                },
            ],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(repeated.poam_id, Some(poam_id));
    assert!(repeated.poam_reused);

    sqlx::query("UPDATE poams SET target_date=NULL WHERE id=$1")
        .bind(poam_id)
        .execute(&pool)
        .await
        .unwrap();
    let incomplete = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert_eq!(incomplete.rollup, FleetCveTriageRollup::Outstanding);
    assert!(
        incomplete
            .environments
            .iter()
            .filter(|environment| environment.disposition.is_none())
            .count()
            >= 1,
        "incomplete active POA&M metadata must fail closed as OPEN"
    );
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "outstanding").await;
    sqlx::query("UPDATE poams SET target_date=$2 WHERE id=$1")
        .bind(poam_id)
        .bind(fleet_poam_request(actor.user_id, &clock).target_date)
        .execute(&pool)
        .await
        .unwrap();
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "scheduled").await;

    let subset = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch {
                environment_id: first_environment,
            }],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        subset,
        PoamError::Conflict("cve_evidence_changed", _)
    ));

    let foreign = assessment_fixture(&pool).await;
    let foreign_observation = seal_exact_cve_scan(
        &pool,
        &foreign,
        clock.now(),
        Some((cve_id, package_name, "3.0.10", false)),
    )
    .await
    .unwrap();
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM poams WHERE id=$1")
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let linked = poam_service::link_cve_finding(
        &pool,
        &actor,
        poam_id,
        AddCveFindingRequest {
            revision,
            observation: foreign_observation,
        },
        &clock,
    )
    .await
    .unwrap();
    let foreign_finding_id = linked
        .cve_findings
        .iter()
        .find(|finding| finding.system_id == foreign.system_id)
        .unwrap()
        .id;
    let historical_extra = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: first_environment,
                },
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: second_environment,
                },
            ],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .expect("extra historical membership must not force a new remediation episode");
    assert_eq!(historical_extra.poam_id, Some(poam_id));
    assert!(historical_extra.poam_reused);
    sqlx::query(
        "UPDATE poam_cve_finding_links SET retired_at=NOW(),retired_by=$2,retirement_reason='test_cleanup' WHERE cve_finding_id=$1 AND retired_at IS NULL",
    )
    .bind(foreign_finding_id)
    .bind(actor.user_id)
    .execute(&pool)
    .await
    .unwrap();

    let incompatible = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: first_environment,
                },
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: second_environment,
                },
            ],
            poam: Some(FleetCvePoamRequest {
                plan: "A different remediation plan must not silently reuse ownership".into(),
                ..fleet_poam_request(actor.user_id, &clock)
            }),
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        incompatible,
        PoamError::ConflictDetails("cve_subjects_already_managed", _, _)
    ));

    let accepted = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: first_environment,
                },
                CveEnvironmentTriageAction::AcceptRisk {
                    environment_id: second_environment,
                    justification: "Risk is accepted for the isolated staging environment".into(),
                    review_date: Some(clock.today() + chrono::Duration::days(14)),
                },
            ],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(accepted.detail.rollup, FleetCveTriageRollup::Partial);
    assert_eq!(accepted.poam_id, Some(poam_id));
    assert!(accepted.poam_reused);
    let preserved = accepted
        .detail
        .environments
        .iter()
        .find(|environment| environment.environment_id == first_environment)
        .and_then(|environment| environment.disposition.as_ref());
    assert!(matches!(
        preserved,
        Some(CveEnvironmentDisposition::Scheduled { poam: Some(poam), .. })
            if poam.id == poam_id
                && poam.plan == fleet_poam_request(actor.user_id, &clock).plan
                && matches!(&poam.assignee, PoamAssigneeView::User { user_id, .. } if *user_id == actor.user_id)
    ));
    assert!(matches!(
        accepted
            .detail
            .environments
            .iter()
            .find(|environment| environment.environment_id == second_environment)
            .and_then(|environment| environment.disposition.as_ref()),
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    let accepted_history: (i64, DateTime<Utc>) = sqlx::query_as(
        r#"SELECT COUNT(*),max(accepted_at)
           FROM cve_environment_dispositions
           WHERE canonical_cve_id=$1 AND canonical_package_name=$2
             AND environment_id=$3"#,
    )
    .bind(cve_id)
    .bind(package_name)
    .bind(second_environment)
    .fetch_one(&pool)
    .await
    .unwrap();
    let retry_actor = admin_actor(second.user_id);
    let accepted_retry = poam_service::triage_fleet_cve(
        &pool,
        &retry_actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: first_environment,
                },
                CveEnvironmentTriageAction::AcceptRisk {
                    environment_id: second_environment,
                    justification: "Risk is accepted for the isolated staging environment".into(),
                    review_date: Some(clock.today() + chrono::Duration::days(14)),
                },
            ],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(accepted_retry.detail.rollup, FleetCveTriageRollup::Partial);
    let accepted_history_after: (i64, DateTime<Utc>) = sqlx::query_as(
        r#"SELECT COUNT(*),max(accepted_at)
           FROM cve_environment_dispositions
           WHERE canonical_cve_id=$1 AND canonical_package_name=$2
             AND environment_id=$3"#,
    )
    .bind(cve_id)
    .bind(package_name)
    .bind(second_environment)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(accepted_history_after, accepted_history);

    let restored_schedule = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: first_environment,
                },
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: second_environment,
                },
            ],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .expect("restored exact environment ownership reuses the active episode");
    assert_eq!(restored_schedule.poam_id, Some(poam_id));
    assert!(restored_schedule.poam_reused);
    let remaining_links: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining_links, 3);
    let verification_items: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_cve_verification_items item JOIN poam_cve_findings finding ON finding.id=item.cve_finding_id WHERE finding.system_id=$1",
    )
    .bind(third.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        verification_items, 0,
        "accepted risk must not create PASS evidence"
    );

    let final_subject = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![
                CveEnvironmentTriageAction::LeaveOpen {
                    environment_id: first_environment,
                },
                CveEnvironmentTriageAction::AcceptRisk {
                    environment_id: second_environment,
                    justification: "Risk is accepted for the isolated staging environment".into(),
                    review_date: Some(clock.today() + chrono::Duration::days(14)),
                },
            ],
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        final_subject,
        PoamError::ConflictDetails("poam_final_subject", _, _)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL"
        )
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        3,
        "a rejected final-subject change must roll back the whole transaction"
    );

    let scoped_actor = PoamActor {
        user_id: first.user_id,
        identifier: "scoped-fleet-operator@example.invalid".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: vec![first_environment],
        request_origin: Some("fleet-scope-test".into()),
    };
    let scoped = poam_service::fleet_cve_detail(&pool, &scoped_actor, cve_id, package_name)
        .await
        .unwrap();
    assert_eq!(scoped.environments.len(), 1);
    assert_eq!(scoped.affected_system_count, 2);
    let hidden = poam_service::triage_fleet_cve(
        &pool,
        &scoped_actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![CveEnvironmentTriageAction::LeaveOpen {
                environment_id: second_environment,
            }],
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        hidden,
        PoamError::Conflict("cve_evidence_changed", _)
    ));

    let current = poam_service::detail(&pool, &actor, poam_id, &clock)
        .await
        .unwrap();
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        poam_id,
        TransitionPoamRequest {
            revision: current.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    for fixture in [&first, &second, &third] {
        seal_exact_cve_scan(&pool, fixture, clock.now() + TimeDelta::minutes(2), None).await;
    }
    let closed = poam_service::close(&pool, &actor, poam_id, awaiting.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(closed.poam.status, "completed");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM cve_environment_dispositions WHERE poam_id=$1 AND state='scheduled' AND retired_at IS NULL",
        )
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );

    for fixture in [&first, &second, &third] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now() + TimeDelta::minutes(3),
            Some((cve_id, package_name, "3.0.11", false)),
        )
        .await;
    }
    let recurrent = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert!(
        recurrent
            .environments
            .iter()
            .find(|environment| environment.environment_id == first_environment)
            .unwrap()
            .disposition
            .is_none(),
        "a completed POA&M must not claim recurrent CVE coverage"
    );
    let stale_disposition_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO cve_environment_dispositions(
             canonical_cve_id,canonical_package_name,environment_id,state,
             poam_id,scheduled_by,scheduled_at)
           VALUES($1,$2,$3,'scheduled',$4,$5,$6)
           RETURNING id"#,
    )
    .bind(cve_id)
    .bind(package_name)
    .bind(first_environment)
    .bind(poam_id)
    .bind(actor.user_id)
    .bind(clock.now())
    .fetch_one(&pool)
    .await
    .unwrap();
    let completed_reference = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert_eq!(
        completed_reference.rollup,
        FleetCveTriageRollup::Outstanding
    );
    assert!(
        completed_reference
            .environments
            .iter()
            .find(|environment| environment.environment_id == first_environment)
            .unwrap()
            .disposition
            .is_none(),
        "a stale completed POA&M reference must fail closed without a database error"
    );
    assert_eq!(
        fetch_cve_list(&pool, &CveReadScope::All, &CveFilters::default())
            .await
            .unwrap()
            .into_iter()
            .find(|row| {
                row.cve_id == cve_id && row.package_name.as_deref() == Some(package_name)
            })
            .unwrap()
            .triage_status,
        "outstanding"
    );
    sqlx::query(
        r#"UPDATE cve_environment_dispositions
           SET retired_at=$2,retired_by=$3,retirement_reason='test_cleanup'
           WHERE id=$1"#,
    )
    .bind(stale_disposition_id)
    .bind(clock.now())
    .bind(actor.user_id)
    .execute(&pool)
    .await
    .unwrap();
    let reopened = poam_service::reopen(&pool, &actor, poam_id, closed.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(reopened.poam.status, "in_progress");
    let restored = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert!(matches!(
        restored
            .environments
            .iter()
            .find(|environment| environment.environment_id == first_environment)
            .and_then(|environment| environment.disposition.as_ref()),
        Some(CveEnvironmentDisposition::Scheduled {
            poam_id: restored_id,
            ..
        }) if *restored_id == poam_id
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn system_cve_triage_derives_full_environment_and_preserves_shared_poam_subjects(
    pool: PgPool,
) {
    let first = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let foreign = assessment_fixture(&pool).await;
    let actor = admin_actor(first.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-32620";
    let package_name = "system-detail-package";
    let selected_environment =
        assign_environment(&pool, "system-triage-production", &[&first, &peer]).await;
    let foreign_environment = assign_environment(&pool, "system-triage-staging", &[&foreign]).await;
    for fixture in [&first, &peer, &foreign] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "1.0.0", false)),
        )
        .await;
    }

    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: selected_environment,
                },
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: foreign_environment,
                },
            ],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let shared_poam_id = scheduled.poam_id.unwrap();

    let detail = poam_service::system_cve_triage_detail(
        &pool,
        &actor,
        first.system_id,
        cve_id,
        package_name,
    )
    .await
    .unwrap();
    assert_eq!(
        detail.scope.kind,
        SystemCveTriageScopeKind::CurrentExactAffectedHostsInEnvironment
    );
    assert_eq!(detail.scope.environment_id, selected_environment);
    assert_eq!(detail.scope.exact_affected_system_count, 2);
    assert_eq!(
        detail
            .systems
            .iter()
            .map(|system| system.system_id)
            .collect::<std::collections::BTreeSet<_>>(),
        [first.system_id, peer.system_id].into_iter().collect()
    );

    let reused = poam_service::triage_system_cve(
        &pool,
        &actor,
        first.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(reused.poam_id, Some(shared_poam_id));
    assert!(reused.poam_reused);
    assert_eq!(reused.detail.scope.exact_affected_system_count, 2);

    let accepted = poam_service::triage_system_cve(
        &pool,
        &actor,
        first.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "The production environment has compensating controls".into(),
                review_date: Some(clock.today() + TimeDelta::days(14)),
            },
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(matches!(
        accepted.detail.disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    let active_systems: Vec<Uuid> = sqlx::query_scalar(
        "SELECT system_id FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL ORDER BY system_id",
    )
    .bind(shared_poam_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(active_systems, vec![foreign.system_id]);
    let fleet = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert!(matches!(
        fleet
            .environments
            .iter()
            .find(|environment| environment.environment_id == foreign_environment)
            .and_then(|environment| environment.disposition.as_ref()),
        Some(CveEnvironmentDisposition::Scheduled { poam_id, .. }) if *poam_id == shared_poam_id
    ));
    let verification_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM poam_cve_verification_items")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(verification_count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn external_current_reconciliation_uses_normal_triage_and_real_poam_baseline(pool: PgPool) {
    let first = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let actor = admin_actor(first.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .expect("admin should be authorized");
    let environment_id = assign_environment(&pool, "external-triage", &[&first, &peer]).await;
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-32691";
    let package = "external-current-package";
    let flake_id: i32 = sqlx::query_scalar("SELECT flake_id FROM systems WHERE id=$1")
        .bind(first.system_id)
        .fetch_one(&pool)
        .await
        .expect("selected system should have a registered flake");
    let head_id: i32 = sqlx::query_scalar(
        r#"INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp)
           VALUES($1,$2,now()+interval '1 minute') RETURNING id"#,
    )
    .bind(flake_id)
    .bind(Uuid::new_v4().simple().to_string())
    .fetch_one(&pool)
    .await
    .expect("a newer unactivated flake head should persist");
    sqlx::query(
        "INSERT INTO flake_branch_commit_snapshot(flake_id,commit_id,position) VALUES($1,$2,0)",
    )
    .bind(flake_id)
    .bind(head_id)
    .execute(&pool)
    .await
    .expect("newer head should be tracked independently of running Current");
    for fixture in [&first, &peer] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package, "1.0", false)),
        )
        .await;
    }

    // The new generation uses the same exact registered output but has no CF
    // deployment request. The peer remains normally retained generation 1.
    sqlx::query(
        r#"INSERT INTO system_states(
             hostname,change_reason,store_path,generation,
             generation_matches_current_store_path,timestamp)
           SELECT hostname,'startup',$2,2,true,clock_timestamp()+interval '1 minute'
           FROM systems WHERE id=$1"#,
    )
    .bind(first.system_id)
    .bind(&first.store_path)
    .execute(&pool)
    .await
    .expect("external generation report should persist");

    let before_reconciliation =
        poam_service::system_cve_triage_detail(&pool, &actor, first.system_id, cve_id, package)
            .await
            .expect("exact Current CVE triage must not wait for retained Config evidence");
    assert_eq!(before_reconciliation.scope.exact_affected_system_count, 2);
    assert!(
        reconcile_external_current_generation(&pool, first.system_id)
            .await
            .expect("one external generation should reconcile")
    );
    assert!(
        !reconcile_external_current_generation(&pool, first.system_id)
            .await
            .expect("repeated reconciliation should be idempotent")
    );

    let (retained_id, artifact_id, origin, generation): (Uuid, Uuid, String, i32) =
        sqlx::query_as(
            "SELECT id,snapshot_id,binding_origin,generation FROM evaluation_generation_snapshots WHERE system_id=$1 AND generation=2",
        )
        .bind(first.system_id)
        .fetch_one(&pool)
        .await
        .expect("external generation should retain an actual artifact");
    assert_eq!(generation, 2);
    assert_eq!(origin, "external_reconciled");
    let artifact_matches: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM evaluation_snapshots WHERE id=$1 AND lifecycle='available' AND schema_version=1 AND integrity_version=1)",
    )
    .bind(artifact_id)
    .fetch_one(&pool)
    .await
    .expect("artifact should remain readable");
    assert!(artifact_matches);
    let fabricated_deployments: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pending_system_deployments WHERE system_id=$1")
            .bind(first.system_id)
            .fetch_one(&pool)
            .await
            .expect("deployment provenance should remain separate");
    assert_eq!(fabricated_deployments, 0);

    let historical = SystemCveInventoryPageRequest::from_params(SystemCveInventoryParams {
        target: Some("exact_derivation".into()),
        target_id: Some(first.derivation_id.to_string()),
        ..Default::default()
    })
    .expect("explicit derivation should validate");
    let historical_read = fetch_system_cve_inventory_page(&pool, first.system_id, &historical)
        .await
        .expect("explicit target remains independently readable");
    assert!(
        historical_read.read_only,
        "explicit browsing never grants Current triage"
    );

    let detail =
        poam_service::system_cve_triage_detail(&pool, &actor, first.system_id, cve_id, package)
            .await
            .expect("reconciled external Current must use normal detail");
    assert_eq!(detail.scope.environment_id, environment_id);
    assert_eq!(detail.scope.exact_affected_system_count, 2);

    let accepted = poam_service::triage_system_cve(
        &pool,
        &actor,
        first.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "The external host has verified compensating controls".into(),
                review_date: None,
            },
            poam: None,
        },
        &clock,
    )
    .await
    .expect("host risk acceptance should persist");
    assert_eq!(
        accepted.detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
    let reopened =
        poam_service::system_cve_triage_detail(&pool, &actor, first.system_id, cve_id, package)
            .await
            .expect("host risk acceptance should reopen");
    assert!(matches!(
        reopened.host_disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));

    let environment = poam_service::triage_system_cve(
        &pool,
        &actor,
        first.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "The environment limits exposure to this finding".into(),
                review_date: None,
            },
            poam: None,
        },
        &clock,
    )
    .await
    .expect("environment acceptance should include the reconciled host");
    assert!(environment.detail.environment_disposition.is_some());
    assert_eq!(
        environment.detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );

    poam_service::triage_system_cve(
        &pool,
        &actor,
        first.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::LeaveOpen,
            poam: None,
        },
        &clock,
    )
    .await
    .expect("clearing host override should restore environment ownership");
    let scheduled = poam_service::triage_system_cve(
        &pool,
        &actor,
        first.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .expect("existing environment scheduler should create the POA&M");
    let poam_id = scheduled.poam_id.expect("schedule should create POA&M");
    let baseline: Uuid = sqlx::query_scalar(
        "SELECT baseline_generation_snapshot_id FROM poam_cve_finding_links WHERE poam_id=$1 AND system_id=$2 AND retired_at IS NULL",
    )
    .bind(poam_id)
    .bind(first.system_id)
    .fetch_one(&pool)
    .await
    .expect("external subject should have a real baseline link");
    assert_eq!(baseline, retained_id);
    let reused = poam_service::triage_system_cve(
        &pool,
        &actor,
        first.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .expect("compatible environment schedule should reuse POA&M");
    assert_eq!(reused.poam_id, Some(poam_id));
    assert!(reused.poam_reused);
}

#[sqlx::test(migrations = "./migrations")]
async fn external_unretained_current_supports_inventory_host_risk_and_null_poam_baseline(
    pool: PgPool,
) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve = "CVE-2026-32697";
    let package = "sledge-package";
    let environment = assign_environment(&pool, "sledge-unretained", &[&fixture]).await;
    let (commit_id, configuration): (i32, String) =
        sqlx::query_as("SELECT commit_id,derivation_name FROM derivations WHERE id=$1")
            .bind(fixture.derivation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    // Remove only the fixture's Config artifacts. The CVE scan and observed
    // output remain real; no retained record is substituted for them.
    sqlx::query("DELETE FROM evaluation_generation_snapshots WHERE system_id=$1")
        .bind(fixture.system_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM evaluation_snapshot_selections WHERE commit_id=$1 AND configuration_name=$2",
    )
    .bind(commit_id)
    .bind(&configuration)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM evaluation_snapshots WHERE commit_id=$1 AND configuration_name=$2")
        .bind(commit_id)
        .bind(&configuration)
        .execute(&pool)
        .await
        .unwrap();
    let unavailable: Uuid = sqlx::query_scalar(
        "INSERT INTO evaluation_snapshots(commit_id,configuration_name,lifecycle,integrity_version) VALUES($1,$2,'unavailable',0) RETURNING id",
    )
    .bind(commit_id)
    .bind(&configuration)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO evaluation_snapshot_selections(commit_id,configuration_name,current_snapshot_id) VALUES($1,$2,$3)")
        .bind(commit_id)
        .bind(&configuration)
        .bind(unavailable)
        .execute(&pool)
        .await
        .unwrap();
    report_external_generation(&pool, &fixture, 16, &fixture.store_path, true, 1).await;
    let observation = seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some((cve, package, "1.0", false)),
    )
    .await
    .unwrap();
    let current =
        SystemCveInventoryPageRequest::from_params(SystemCveInventoryParams::default()).unwrap();
    let inventory = fetch_system_cve_inventory_page(&pool, fixture.system_id, &current)
        .await
        .unwrap();
    assert!(!inventory.read_only);
    assert_eq!(inventory.source.unwrap().scan_id, observation.scan_id);
    assert_eq!(inventory.rows.len(), 1);
    assert_eq!(inventory.rows[0].cve_id, cve);
    let historical = SystemCveInventoryPageRequest::from_params(SystemCveInventoryParams {
        target: Some("exact_derivation".into()),
        target_id: Some(fixture.derivation_id.to_string()),
        ..Default::default()
    })
    .unwrap();
    assert!(
        fetch_system_cve_inventory_page(&pool, fixture.system_id, &historical)
            .await
            .unwrap()
            .read_only
    );
    assert_eq!(
        poam_service::system_cve_triage_detail(&pool, &actor, fixture.system_id, cve, package)
            .await
            .unwrap()
            .scope
            .environment_id,
        environment
    );
    let accepted = poam_service::triage_system_cve(
        &pool,
        &actor,
        fixture.system_id,
        cve,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "Scoped compensating controls for this host".into(),
                review_date: None,
            },
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        accepted.detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
    assert!(matches!(
        poam_service::system_cve_triage_detail(&pool, &actor, fixture.system_id, cve, package)
            .await
            .unwrap()
            .host_disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    let scheduled = poam_service::triage_system_cve(
        &pool,
        &actor,
        fixture.system_id,
        cve,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    let baseline: (Option<Uuid>, Uuid, i32, i32, String) = sqlx::query_as(
        "SELECT baseline_generation_snapshot_id,baseline_scan_id,baseline_generation,baseline_scan_derivation_id,baseline_observed_package_version FROM poam_cve_finding_links WHERE poam_id=$1 AND system_id=$2 AND retired_at IS NULL",
    )
    .bind(poam_id)
    .bind(fixture.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        baseline,
        (
            None,
            observation.scan_id,
            16,
            fixture.derivation_id,
            "1.0".into()
        )
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM evaluation_generation_snapshots WHERE system_id=$1"
        )
        .bind(fixture.system_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pending_system_deployments WHERE system_id=$1"
        )
        .bind(fixture.system_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    let clean_at = clock.now() + TimeDelta::minutes(1);
    seal_exact_cve_scan(&pool, &fixture, clean_at, None).await;
    let detail = poam_service::detail(&pool, &actor, poam_id, &clock)
        .await
        .unwrap();
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        poam_id,
        TransitionPoamRequest {
            revision: detail.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let verified = poam_service::verify(&pool, &actor, poam_id, awaiting.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(verified["outcome"], "accepted");
    let closed = poam_service::close(
        &pool,
        &actor,
        poam_id,
        verified["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(closed.poam.status, "completed");
    assert!(
        matches!(
            poam_service::reopen(&pool, &actor, poam_id, closed.poam.revision, &clock).await,
            Err(PoamError::Conflict("cve_disposition_conflict", _))
        ),
        "clean Current evidence must not reopen a host-only schedule"
    );
    seal_exact_cve_scan(
        &pool,
        &fixture,
        clean_at + TimeDelta::minutes(1),
        Some((cve, package, "2.0", false)),
    )
    .await;
    let reopened = poam_service::reopen(&pool, &actor, poam_id, closed.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(reopened.poam.status, "in_progress");
    assert_eq!(
        reopened.cve_findings[0].baseline_scan_id,
        observation.scan_id
    );
    assert_eq!(
        reopened.cve_findings[0].baseline_generation_snapshot_id,
        None
    );
    let copied: Vec<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT baseline_scan_id,baseline_generation_snapshot_id FROM poam_cve_finding_links WHERE poam_id=$1 ORDER BY linked_at,id",
    )
    .bind(poam_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(copied, vec![(observation.scan_id, None); 2]);
}

async fn external_cve_revision(
    pool: &PgPool,
    fixture: &AssessmentFixture,
    generation: i32,
    completed_at: DateTime<Utc>,
    occurrence: Option<(&str, &str, &str, bool)>,
) -> (i32, Option<CveObservationReference>) {
    let (repository, hostname): (String, String) = sqlx::query_as(
        "SELECT flake.repo_url,system.hostname FROM systems system JOIN flakes flake ON flake.id=system.flake_id WHERE system.id=$1",
    )
    .bind(fixture.system_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let hash = Uuid::new_v4().simple().to_string();
    insert_commit(pool, &hash, &repository, Utc::now())
        .await
        .unwrap();
    let commit_id: i32 = sqlx::query_scalar("SELECT id FROM commits WHERE git_commit_hash=$1")
        .bind(&hash)
        .fetch_one(pool)
        .await
        .unwrap();
    let store_path = format!("/nix/store/{hash}-continuity");
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
        | SuccessfulEvalWrite::PreservedBuildState { derivation_id, .. }
        | SuccessfulEvalWrite::LegacyPathConflict { derivation_id } => derivation_id,
    };
    sqlx::query("UPDATE derivations SET store_path=$1,status_id=10 WHERE id=$2")
        .bind(&store_path)
        .bind(derivation_id)
        .execute(pool)
        .await
        .unwrap();
    report_external_generation(pool, fixture, generation, &store_path, true, generation).await;
    let reference = seal_exact_cve_scan_for_derivation(
        pool,
        fixture.system_id,
        derivation_id,
        completed_at,
        occurrence,
    )
    .await;
    (derivation_id, reference)
}

#[sqlx::test(migrations = "./migrations")]
async fn external_cve_poam_preserves_baseline_across_revisions_clean_and_recurrence(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    assign_environment(&pool, "continuity-recurrence", &[&fixture]).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve = "CVE-2026-32698";
    let package = "continuity-package";
    let first = seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some((cve, package, "1.0", false)),
    )
    .await
    .unwrap();
    let created = poam_service::create_cve(
        &pool,
        &actor,
        CreateCvePoamRequest {
            observation: first.clone(),
            title: "Cross-revision remediation".into(),
            plan: "Deploy a clean revision".into(),
            owner: "Security".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = created.poam.id;
    let finding_id = created.cve_findings[0].id;
    let retained = created.cve_findings[0].baseline_generation_snapshot_id;
    assert!(retained.is_some(), "retained A baseline must stay non-null");

    let (b_derivation, b) = external_cve_revision(
        &pool,
        &fixture,
        2,
        clock.now() + TimeDelta::minutes(2),
        Some((cve, package, "2.0", false)),
    )
    .await;
    let b = b.unwrap();
    let b_detail = poam_service::detail(&pool, &actor, poam_id, &clock)
        .await
        .unwrap();
    let b_finding = &b_detail.cve_findings[0];
    assert_eq!(b_finding.id, finding_id);
    assert_eq!(b_finding.baseline_scan_id, first.scan_id);
    assert_eq!(b_finding.baseline_generation_snapshot_id, retained);
    assert_eq!(b_finding.baseline_observed_package_version, "1.0");
    assert_eq!(b_finding.current_scan_id, Some(b.scan_id));
    assert_eq!(b_finding.current_derivation_id, Some(b_derivation));
    assert_eq!(
        b_finding.current_observed_package_version.as_deref(),
        Some("2.0")
    );
    assert_eq!(b_finding.resolution_state, "fail");

    let awaiting = poam_service::transition(
        &pool,
        &actor,
        poam_id,
        TransitionPoamRequest {
            revision: b_detail.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let affected = poam_service::verify(&pool, &actor, poam_id, awaiting.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(affected["outcome"], "rejected");
    assert_ne!(affected["cve_items"][0]["result"], "pass");

    let (clean_derivation, _) = external_cve_revision(
        &pool,
        &fixture,
        3,
        clock.now() + TimeDelta::minutes(3),
        None,
    )
    .await;
    let clean_detail = poam_service::detail(&pool, &actor, poam_id, &clock)
        .await
        .unwrap();
    assert_eq!(clean_detail.poam.status, "awaiting_verification");
    assert_eq!(clean_detail.cve_findings[0].id, finding_id);
    assert_eq!(
        clean_detail.cve_findings[0].current_derivation_id,
        Some(clean_derivation)
    );
    assert_eq!(
        clean_detail.cve_findings[0].current_observed_package_version,
        None
    );
    assert_eq!(clean_detail.cve_findings[0].baseline_scan_id, first.scan_id);

    let (_, recurrence) = external_cve_revision(
        &pool,
        &fixture,
        4,
        clock.now() + TimeDelta::minutes(4),
        Some((cve, package, "4.0", false)),
    )
    .await;
    let recurrence = recurrence.unwrap();
    let recurring = poam_service::detail(&pool, &actor, poam_id, &clock)
        .await
        .unwrap();
    assert_eq!(recurring.cve_findings[0].id, finding_id);
    assert_eq!(
        recurring.cve_findings[0].current_scan_id,
        Some(recurrence.scan_id)
    );
    assert_eq!(recurring.cve_findings[0].baseline_scan_id, first.scan_id);
    let blocked = poam_service::close(&pool, &actor, poam_id, recurring.poam.revision, &clock)
        .await
        .unwrap_err();
    assert!(matches!(
        blocked,
        PoamError::Precondition("closure_not_ready", _, _)
    ));

    let (_, _) = external_cve_revision(
        &pool,
        &fixture,
        5,
        clock.now() + TimeDelta::minutes(5),
        None,
    )
    .await;
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM poams WHERE id=$1")
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let verified = poam_service::verify(&pool, &actor, poam_id, revision, &clock)
        .await
        .unwrap();
    assert_eq!(verified["outcome"], "accepted");
    assert_eq!(verified["cve_items"][0]["result"], "pass");
    let verified_detail = poam_service::detail(&pool, &actor, poam_id, &clock)
        .await
        .unwrap();
    assert_eq!(
        verified_detail.verification_attempts[0].cve_items[0].baseline_scan_id,
        first.scan_id
    );
    assert_ne!(
        verified_detail.verification_attempts[0].cve_items[0].scan_id,
        Some(first.scan_id)
    );
    let closed = poam_service::close(
        &pool,
        &actor,
        poam_id,
        verified["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(closed.poam.status, "completed");
    let (_, recurrent_scan) = external_cve_revision(
        &pool,
        &fixture,
        6,
        clock.now() + TimeDelta::minutes(6),
        Some((cve, package, "6.0", false)),
    )
    .await;
    let recurrent_scan = recurrent_scan.unwrap();
    let after = poam_service::detail(&pool, &actor, poam_id, &clock)
        .await
        .unwrap();
    assert_eq!(after.poam.status, "completed");
    assert_eq!(after.cve_findings[0].id, finding_id);
    assert_eq!(after.cve_findings[0].baseline_scan_id, first.scan_id);
    assert!(
        after.verification_attempts.iter().any(|attempt| {
            attempt.outcome == "accepted"
                && attempt.cve_items.iter().any(|item| item.result == "pass")
        }),
        "the accepted closure evidence must survive recurrence"
    );
    let next_episode = poam_service::triage_system_cve(
        &pool,
        &actor,
        fixture.system_id,
        cve,
        SystemCveTriageRequest {
            canonical_package_name: package.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .expect("a recurrent exact Current finding can begin a new episode");
    let new_poam_id = next_episode.poam_id.expect("recurrence requires a POA&M");
    assert_ne!(new_poam_id, poam_id);
    assert!(!next_episode.poam_reused);
    let (new_finding_id, new_scan_id): (Uuid, Uuid) = sqlx::query_as(
        r#"SELECT finding.id,link.baseline_scan_id FROM poam_cve_finding_links link
           JOIN poam_cve_findings finding ON finding.id=link.cve_finding_id
           WHERE link.poam_id=$1 AND link.retired_at IS NULL"#,
    )
    .bind(new_poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(new_finding_id, finding_id);
    assert_eq!(new_scan_id, recurrent_scan.scan_id);
    assert_eq!(
        poam_service::detail(&pool, &actor, poam_id, &clock)
            .await
            .unwrap()
            .poam
            .status,
        "completed"
    );
}

async fn report_external_generation(
    pool: &PgPool,
    fixture: &AssessmentFixture,
    generation: i32,
    store_path: &str,
    matches_current: bool,
    minutes: i32,
) {
    sqlx::query(
        r#"INSERT INTO system_states(
             hostname,change_reason,store_path,generation,
             generation_matches_current_store_path,timestamp)
           SELECT hostname,'startup',$2,$3,$4,
                  clock_timestamp()+($5::text || ' minutes')::interval
           FROM systems WHERE id=$1"#,
    )
    .bind(fixture.system_id)
    .bind(store_path)
    .bind(generation)
    .bind(matches_current)
    .bind(minutes)
    .execute(pool)
    .await
    .expect("external observation should persist");
}

#[sqlx::test(migrations = "./migrations")]
async fn external_reconciliation_rejects_missing_schema1_and_foreign_or_invalid_outputs(
    pool: PgPool,
) {
    let fixture = assessment_fixture(&pool).await;
    let foreign = assessment_fixture(&pool).await;
    report_external_generation(&pool, &fixture, 2, &fixture.store_path, true, 1).await;
    assert!(
        !reconcile_external_current_generation(&pool, fixture.system_id)
            .await
            .expect("missing scan must not authorize reconciliation")
    );

    seal_exact_cve_scan(
        &pool,
        &foreign,
        Utc::now(),
        Some(("CVE-2026-32692", "foreign-package", "1.0", false)),
    )
    .await;
    assert!(
        !reconcile_external_current_generation(&pool, fixture.system_id)
            .await
            .expect("another derivation's scan must not authorize this target")
    );

    let legacy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cve_scans(derivation_id,scanner_name,status) VALUES($1,'legacy','in_progress') RETURNING id",
    )
    .bind(fixture.derivation_id)
    .fetch_one(&pool)
    .await
    .expect("legacy scan should start unsealed");
    sqlx::query("UPDATE cve_scans SET status='completed',completed_at=now() WHERE id=$1")
        .bind(legacy_id)
        .execute(&pool)
        .await
        .expect("legacy schema-0 scan should complete");
    assert!(
        !reconcile_external_current_generation(&pool, fixture.system_id)
            .await
            .expect("schema-0 scan cannot authorize external retention")
    );

    seal_exact_cve_scan(
        &pool,
        &fixture,
        Utc::now(),
        Some(("CVE-2026-32693", "local-package", "1.0", false)),
    )
    .await;
    report_external_generation(&pool, &fixture, 3, "/nix/store/unmapped-external", true, 2).await;
    assert!(
        !reconcile_external_current_generation(&pool, fixture.system_id)
            .await
            .expect("unmapped output cannot borrow the scanned target")
    );
    report_external_generation(&pool, &fixture, 4, &fixture.store_path, false, 3).await;
    assert!(
        !reconcile_external_current_generation(&pool, fixture.system_id)
            .await
            .expect("contradictory generation/path report cannot reconcile")
    );
    report_external_generation(&pool, &fixture, 5, &foreign.store_path, true, 4).await;
    assert!(
        !reconcile_external_current_generation(&pool, fixture.system_id)
            .await
            .expect("foreign flake and configuration cannot reconcile")
    );
    let retained_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM evaluation_generation_snapshots WHERE system_id=$1 AND binding_origin='external_reconciled'",
    )
    .bind(fixture.system_id)
    .fetch_one(&pool)
    .await
    .expect("external bindings should be countable");
    assert_eq!(retained_count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn external_reconciliation_rejects_ambiguous_mapping_and_serializes_duplicate_reports(
    pool: PgPool,
) {
    let fixture = assessment_fixture(&pool).await;
    seal_exact_cve_scan(
        &pool,
        &fixture,
        Utc::now(),
        Some(("CVE-2026-32694", "ambiguous-package", "1.0", false)),
    )
    .await;
    report_external_generation(&pool, &fixture, 2, &fixture.store_path, true, 1).await;
    let (commit_id, flake_id, configuration): (i32, i32, String) = sqlx::query_as(
        r#"SELECT derivation.commit_id,commit.flake_id,derivation.derivation_name
           FROM derivations derivation JOIN commits commit ON commit.id=derivation.commit_id
           WHERE derivation.id=$1"#,
    )
    .bind(fixture.derivation_id)
    .fetch_one(&pool)
    .await
    .expect("original target lineage should exist");
    let second_commit: i32 = sqlx::query_scalar(
        r#"INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp)
           VALUES($1,$2,now()) RETURNING id"#,
    )
    .bind(flake_id)
    .bind(Uuid::new_v4().simple().to_string())
    .fetch_one(&pool)
    .await
    .expect("same-flake second commit should persist");
    assert_ne!(second_commit, commit_id);
    sqlx::query(
        r#"INSERT INTO derivations(
             commit_id,derivation_type,derivation_name,derivation_path,
             store_path,status_id)
           VALUES($1,'nixos',$2,$3,$4,10)"#,
    )
    .bind(second_commit)
    .bind(configuration)
    .bind(format!(
        "/nix/store/{}-duplicate-output.drv",
        Uuid::new_v4().simple()
    ))
    .bind(&fixture.store_path)
    .execute(&pool)
    .await
    .expect("ambiguous same-output target should persist");
    sqlx::query("UPDATE commits SET source_archived=true WHERE id=$1")
        .bind(second_commit)
        .execute(&pool)
        .await
        .expect("competing source can be archived without removing its derivation");
    assert!(
        !reconcile_external_current_generation(&pool, fixture.system_id)
            .await
            .expect("an archived competing mapping must still count toward ambiguity")
    );
    sqlx::query("UPDATE commits SET source_archived=false WHERE id=$1")
        .bind(second_commit)
        .execute(&pool)
        .await
        .expect("competing source should return to the active registry");
    assert!(
        !reconcile_external_current_generation(&pool, fixture.system_id)
            .await
            .expect("two scoped matching derivations must fail closed")
    );

    let independent = assessment_fixture(&pool).await;
    seal_exact_cve_scan(
        &pool,
        &independent,
        Utc::now(),
        Some(("CVE-2026-32695", "concurrent-package", "1.0", false)),
    )
    .await;
    report_external_generation(&pool, &independent, 2, &independent.store_path, true, 1).await;
    let (first, second) = tokio::join!(
        reconcile_external_current_generation(&pool, independent.system_id),
        reconcile_external_current_generation(&pool, independent.system_id)
    );
    assert_ne!(
        first.expect("first attempt should succeed"),
        second.expect("second attempt should succeed")
    );
    report_external_generation(&pool, &independent, 2, &independent.store_path, true, 2).await;
    assert!(
        !reconcile_external_current_generation(&pool, independent.system_id)
            .await
            .expect("repeat report must not insert duplicate")
    );
    let bindings: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM evaluation_generation_snapshots WHERE system_id=$1 AND generation=2",
    )
    .bind(independent.system_id)
    .fetch_one(&pool)
    .await
    .expect("retained-generation key should be unique");
    assert_eq!(bindings, 1);

    let archived = assessment_fixture(&pool).await;
    seal_exact_cve_scan(
        &pool,
        &archived,
        Utc::now(),
        Some(("CVE-2026-32696", "archived-package", "1.0", false)),
    )
    .await;
    report_external_generation(&pool, &archived, 2, &archived.store_path, true, 1).await;
    sqlx::query(
        "UPDATE commits SET source_archived=true WHERE id=(SELECT commit_id FROM derivations WHERE id=$1)",
    )
    .bind(archived.derivation_id)
    .execute(&pool)
    .await
    .expect("sole source should be archived");
    assert!(
        !reconcile_external_current_generation(&pool, archived.system_id)
            .await
            .expect("a sole archived source can be read provisionally, not retained")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn system_cve_triage_serializes_concurrent_environment_decisions_and_rejects_non_exact_rows(
    pool: PgPool,
) {
    let first = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let whitelisted = assessment_fixture(&pool).await;
    let no_scan = assessment_fixture(&pool).await;
    let stale = assessment_fixture(&pool).await;
    let actor = admin_actor(first.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-32621";
    let package_name = "concurrent-system-package";
    assign_environment(&pool, "system-triage-concurrent", &[&first, &peer]).await;
    assign_environment(&pool, "system-triage-whitelisted", &[&whitelisted]).await;
    assign_environment(&pool, "system-triage-no-scan", &[&no_scan]).await;
    assign_environment(&pool, "system-triage-stale", &[&stale]).await;
    for fixture in [&first, &peer] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "2.0.0", false)),
        )
        .await;
    }
    seal_exact_cve_scan(
        &pool,
        &whitelisted,
        clock.now(),
        Some((cve_id, package_name, "2.0.0", true)),
    )
    .await;
    seal_exact_cve_scan(
        &pool,
        &stale,
        clock.now(),
        Some((cve_id, package_name, "2.0.0", false)),
    )
    .await;
    deploy_store_path(&pool, &stale, "/nix/store/system-triage-new-generation").await;
    for ineligible_system_id in [whitelisted.system_id, no_scan.system_id, stale.system_id] {
        assert!(matches!(
            poam_service::system_cve_triage_detail(
                &pool,
                &actor,
                ineligible_system_id,
                cve_id,
                package_name
            )
            .await,
            Err(PoamError::NotFound)
        ));
    }

    let request = || SystemCveTriageRequest {
        canonical_package_name: package_name.into(),
        scope: SystemCveTriageScopeChoice::Environment,
        action: SystemCveTriageAction::AcceptRisk {
            justification: "Concurrent operators accept the documented environment risk".into(),
            review_date: None,
        },
        poam: None,
    };
    let (left, right) = tokio::join!(
        poam_service::triage_system_cve(&pool, &actor, first.system_id, cve_id, request(), &clock),
        poam_service::triage_system_cve(&pool, &actor, peer.system_id, cve_id, request(), &clock)
    );
    assert!(matches!(
        left.unwrap().detail.disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    assert!(matches!(
        right.unwrap().detail.disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    let active_dispositions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cve_environment_dispositions WHERE canonical_cve_id=$1 AND canonical_package_name=$2 AND retired_at IS NULL",
    )
    .bind(cve_id)
    .bind(package_name)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_dispositions, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn system_cve_host_precedence_and_scope_open_are_independent(pool: PgPool) {
    let selected = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let actor = admin_actor(selected.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-32623";
    let package_name = "host-precedence-package";
    let environment_id = assign_environment(&pool, "host-precedence", &[&selected, &peer]).await;
    for fixture in [&selected, &peer] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "4.0.0", false)),
        )
        .await;
    }

    let environment = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "The environment has compensating controls".into(),
                review_date: Some(clock.today() + TimeDelta::days(30)),
            },
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        environment.detail.scope.selected_system_id,
        selected.system_id
    );
    let selected_hostname: String = sqlx::query_scalar("SELECT hostname FROM systems WHERE id=$1")
        .bind(selected.system_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        environment.detail.scope.selected_system_hostname,
        selected_hostname
    );
    assert_eq!(environment.detail.scope.environment_id, environment_id);
    assert_eq!(environment.detail.scope.exact_affected_system_count, 2);
    assert!(environment.detail.host_disposition.is_none());
    assert!(matches!(
        environment.detail.environment_disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    assert_eq!(
        environment.detail.effective_source,
        SystemCveEffectiveDispositionSource::Environment
    );

    let host = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "This host has a stricter compensating control".into(),
                review_date: None,
            },
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(matches!(
        host.detail.host_disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    assert!(matches!(
        host.detail.environment_disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    assert_eq!(
        host.detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
    assert!(
        sqlx::query(
            r#"INSERT INTO cve_system_dispositions(
                 canonical_cve_id,canonical_package_name,system_id,state,
                 justification,accepted_by,accepted_at)
               VALUES($1,$2,$3,'accepted','Duplicate active override',$4,$5)"#,
        )
        .bind(cve_id)
        .bind(package_name)
        .bind(selected.system_id)
        .bind(actor.user_id)
        .bind(clock.now())
        .execute(&pool)
        .await
        .is_err(),
        "one host cannot have two active dispositions for the same CVE and package"
    );

    let environment_open = poam_service::triage_system_cve(
        &pool,
        &actor,
        peer.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::LeaveOpen,
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(environment_open.detail.environment_disposition.is_none());
    let selected_after_environment_open = poam_service::system_cve_triage_detail(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        package_name,
    )
    .await
    .unwrap();
    assert_eq!(
        selected_after_environment_open.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );

    let host_open = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::LeaveOpen,
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(host_open.detail.host_disposition.is_none());
    assert!(host_open.detail.environment_disposition.is_none());
    assert!(host_open.detail.effective_disposition.is_none());
    assert_eq!(
        host_open.detail.effective_source,
        SystemCveEffectiveDispositionSource::None
    );
    let history_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM cve_system_dispositions
           WHERE system_id=$1 AND canonical_cve_id=$2
             AND canonical_package_name=$3"#,
    )
    .bind(selected.system_id)
    .bind(cve_id)
    .bind(package_name)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(history_count, 1);
    assert!(
        sqlx::query(
            r#"UPDATE cve_system_dispositions SET justification='forged'
               WHERE system_id=$1 AND canonical_cve_id=$2
                 AND canonical_package_name=$3"#,
        )
        .bind(selected.system_id)
        .bind(cve_id)
        .bind(package_name)
        .execute(&pool)
        .await
        .is_err()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn system_cve_host_and_environment_mixed_states_preserve_precedence(pool: PgPool) {
    let selected = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let actor = admin_actor(selected.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-32626";
    let package_name = "mixed-scope-package";
    assign_environment(&pool, "mixed-scope", &[&selected, &peer]).await;
    for fixture in [&selected, &peer] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "7.0.0", false)),
        )
        .await;
    }

    let environment_poam = fleet_poam_request(actor.user_id, &clock);
    let environment_schedule = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(environment_poam.clone()),
        },
        &clock,
    )
    .await
    .unwrap();
    let environment_poam_id = environment_schedule.poam_id.unwrap();
    let host_accepted = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "The selected host has compensating controls".into(),
                review_date: None,
            },
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(matches!(
        host_accepted.detail.host_disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    assert!(matches!(
        host_accepted.detail.environment_disposition,
        Some(CveEnvironmentDisposition::Scheduled { poam_id, .. })
            if poam_id == environment_poam_id
    ));
    assert_eq!(
        host_accepted.detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
    let inherited_links: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT system_id FROM poam_cve_finding_links
           WHERE poam_id=$1 AND retired_at IS NULL ORDER BY system_id"#,
    )
    .bind(environment_poam_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(inherited_links, vec![peer.system_id]);

    let rescheduled_environment = poam_service::triage_system_cve(
        &pool,
        &actor,
        peer.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(environment_poam),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(rescheduled_environment.poam_id, Some(environment_poam_id));
    assert!(rescheduled_environment.poam_reused);
    let inherited_links: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT system_id FROM poam_cve_finding_links
           WHERE poam_id=$1 AND retired_at IS NULL ORDER BY system_id"#,
    )
    .bind(environment_poam_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(inherited_links, vec![peer.system_id]);

    seal_exact_cve_scan(&pool, &peer, clock.now() + TimeDelta::minutes(1), None).await;
    let detail = poam_service::detail(&pool, &actor, environment_poam_id, &clock)
        .await
        .unwrap();
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        environment_poam_id,
        TransitionPoamRequest {
            revision: detail.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let verified = poam_service::verify(
        &pool,
        &actor,
        environment_poam_id,
        awaiting.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(verified["outcome"], "accepted");
    let closed = poam_service::close(
        &pool,
        &actor,
        environment_poam_id,
        verified["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    seal_exact_cve_scan(
        &pool,
        &peer,
        clock.now() + TimeDelta::minutes(2),
        Some((cve_id, package_name, "7.0.1", false)),
    )
    .await;
    let reopened = poam_service::reopen(
        &pool,
        &actor,
        environment_poam_id,
        closed.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(reopened.poam.status, "in_progress");
    let reopened_links: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT system_id FROM poam_cve_finding_links
           WHERE poam_id=$1 AND retired_at IS NULL ORDER BY system_id"#,
    )
    .bind(environment_poam_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(reopened_links, vec![peer.system_id]);

    let host_open = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::LeaveOpen,
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(host_open.detail.host_disposition.is_none());
    assert!(matches!(
        host_open.detail.effective_disposition,
        Some(CveEnvironmentDisposition::Scheduled { poam_id, .. })
            if poam_id == environment_poam_id
    ));
    assert_eq!(
        host_open.detail.effective_source,
        SystemCveEffectiveDispositionSource::Environment
    );
    let restored_links: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT system_id FROM poam_cve_finding_links
           WHERE poam_id=$1 AND retired_at IS NULL ORDER BY system_id"#,
    )
    .bind(environment_poam_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        restored_links,
        [selected.system_id, peer.system_id]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    );

    let accepted_selected = assessment_fixture(&pool).await;
    let accepted_peer = assessment_fixture(&pool).await;
    let accepted_cve_id = "CVE-2026-32627";
    let accepted_package_name = "mixed-scope-accepted-package";
    assign_environment(
        &pool,
        "mixed-scope-accepted",
        &[&accepted_selected, &accepted_peer],
    )
    .await;
    for fixture in [&accepted_selected, &accepted_peer] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((accepted_cve_id, accepted_package_name, "8.0.0", false)),
        )
        .await;
    }
    let environment_accepted = poam_service::triage_system_cve(
        &pool,
        &actor,
        accepted_peer.system_id,
        accepted_cve_id,
        SystemCveTriageRequest {
            canonical_package_name: accepted_package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::AcceptRisk {
                justification: "The environment has compensating controls".into(),
                review_date: None,
            },
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(matches!(
        environment_accepted.detail.effective_disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    let mut host_poam = fleet_poam_request(actor.user_id, &clock);
    host_poam.title = "Mixed-scope host remediation".into();
    let host_scheduled = poam_service::triage_system_cve(
        &pool,
        &actor,
        accepted_selected.system_id,
        accepted_cve_id,
        SystemCveTriageRequest {
            canonical_package_name: accepted_package_name.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(host_poam),
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(matches!(
        host_scheduled.detail.host_disposition,
        Some(CveEnvironmentDisposition::Scheduled { .. })
    ));
    assert!(matches!(
        host_scheduled.detail.environment_disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    assert_eq!(
        host_scheduled.detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );

    let host_open = poam_service::triage_system_cve(
        &pool,
        &actor,
        accepted_selected.system_id,
        accepted_cve_id,
        SystemCveTriageRequest {
            canonical_package_name: accepted_package_name.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::LeaveOpen,
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(matches!(
        host_open.detail.effective_disposition,
        Some(CveEnvironmentDisposition::Accepted { .. })
    ));
    assert_eq!(
        host_open.detail.effective_source,
        SystemCveEffectiveDispositionSource::Environment
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn environment_schedule_rejects_scope_fully_owned_by_host_overrides(pool: PgPool) {
    let selected = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let actor = admin_actor(selected.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-32628";
    let package_name = "fully-overridden-package";
    assign_environment(&pool, "fully-overridden", &[&selected, &peer]).await;
    for fixture in [&selected, &peer] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "9.0.0", false)),
        )
        .await;
        poam_service::triage_system_cve(
            &pool,
            &actor,
            fixture.system_id,
            cve_id,
            SystemCveTriageRequest {
                canonical_package_name: package_name.into(),
                scope: SystemCveTriageScopeChoice::Host,
                action: SystemCveTriageAction::AcceptRisk {
                    justification: "The host has compensating controls".into(),
                    review_date: None,
                },
                poam: None,
            },
            &clock,
        )
        .await
        .unwrap();
    }

    let poam_count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM poams")
        .fetch_one(&pool)
        .await
        .unwrap();
    let error = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        PoamError::Conflict("cve_disposition_conflict", _)
    ));
    let poam_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM poams")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(poam_count_after, poam_count_before);
    let environment_dispositions: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM cve_environment_dispositions
           WHERE canonical_cve_id=$1 AND canonical_package_name=$2"#,
    )
    .bind(cve_id)
    .bind(package_name)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(environment_dispositions, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn system_cve_host_schedule_enforces_detachment_and_expansion_rules(pool: PgPool) {
    let selected = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let actor = admin_actor(selected.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-32624";
    let package_name = "host-schedule-package";
    assign_environment(&pool, "host-schedule", &[&selected, &peer]).await;
    for fixture in [&selected, &peer] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "5.0.0", false)),
        )
        .await;
    }
    let poam_request = fleet_poam_request(actor.user_id, &clock);
    let environment = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(poam_request.clone()),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = environment.poam_id.unwrap();
    let host = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(poam_request),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(host.poam_id, Some(poam_id));
    assert!(host.poam_reused);
    assert_eq!(
        host.detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );

    let environment_open = poam_service::triage_system_cve(
        &pool,
        &actor,
        peer.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::LeaveOpen,
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        environment_open.detail.effective_source,
        SystemCveEffectiveDispositionSource::None
    );
    let selected_after_environment_open = poam_service::system_cve_triage_detail(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        package_name,
    )
    .await
    .unwrap();
    assert_eq!(
        selected_after_environment_open.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
    let links_after_environment_open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(links_after_environment_open, 1);

    let final_subject = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::LeaveOpen,
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        final_subject,
        PoamError::ConflictDetails("poam_final_subject", _, _)
    ));

    let selected = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let cve_id = "CVE-2026-32625";
    let package_name = "host-only-expansion-package";
    assign_environment(&pool, "host-only-expansion", &[&selected, &peer]).await;
    for fixture in [&selected, &peer] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "6.0.0", false)),
        )
        .await;
    }

    let mut host_only_request = fleet_poam_request(actor.user_id, &clock);
    host_only_request.title = "Host-only remediation".into();
    let host_only = poam_service::triage_system_cve(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Host,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(host_only_request.clone()),
        },
        &clock,
    )
    .await
    .unwrap();
    let host_only_id = host_only.poam_id.unwrap();
    let expanded = poam_service::triage_system_cve(
        &pool,
        &actor,
        peer.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(host_only_request),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(expanded.poam_id, Some(host_only_id));
    assert!(expanded.poam_reused);
    let expanded_link_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(host_only_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(expanded_link_count, 2);

    let mut invariant_tx = pool.begin().await.unwrap();
    sqlx::query(
        r#"UPDATE cve_system_dispositions
           SET retired_at=$2,retired_by=$3,retirement_reason='host_triage_open'
           WHERE poam_id=$1 AND retired_at IS NULL"#,
    )
    .bind(host_only_id)
    .bind(clock.now())
    .bind(actor.user_id)
    .execute(&mut *invariant_tx)
    .await
    .unwrap();
    sqlx::query(
        r#"UPDATE cve_environment_dispositions
           SET retired_at=$2,retired_by=$3,retirement_reason='test_detach'
           WHERE poam_id=$1 AND retired_at IS NULL"#,
    )
    .bind(host_only_id)
    .bind(clock.now())
    .bind(actor.user_id)
    .execute(&mut *invariant_tx)
    .await
    .unwrap();
    sqlx::query(
        r#"UPDATE poam_cve_finding_links
           SET retired_at=$2,retired_by=$3,retirement_reason='test_detach'
           WHERE poam_id=$1 AND retired_at IS NULL"#,
    )
    .bind(host_only_id)
    .bind(clock.now())
    .bind(actor.user_id)
    .execute(&mut *invariant_tx)
    .await
    .unwrap();
    assert!(
        invariant_tx.commit().await.is_err(),
        "environment history must disqualify the detached host-only exception"
    );

    poam_service::triage_system_cve(
        &pool,
        &actor,
        peer.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::LeaveOpen,
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let mut incompatible_environment = fleet_poam_request(actor.user_id, &clock);
    incompatible_environment.title = "Incompatible environment remediation".into();
    let independent_environment = poam_service::triage_system_cve(
        &pool,
        &actor,
        peer.system_id,
        cve_id,
        SystemCveTriageRequest {
            canonical_package_name: package_name.into(),
            scope: SystemCveTriageScopeChoice::Environment,
            action: SystemCveTriageAction::SchedulePatch,
            poam: Some(incompatible_environment),
        },
        &clock,
    )
    .await
    .unwrap();
    let independent_environment_id = independent_environment.poam_id.unwrap();
    assert_ne!(independent_environment_id, host_only_id);
    let active_owners: Vec<(Uuid, Uuid)> = sqlx::query_as(
        r#"SELECT system_id,poam_id FROM poam_cve_finding_links
           WHERE canonical_cve_id=$1 AND canonical_package_name=$2
             AND retired_at IS NULL ORDER BY system_id"#,
    )
    .bind(cve_id)
    .bind(package_name)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        active_owners
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>(),
        [
            (selected.system_id, host_only_id),
            (peer.system_id, independent_environment_id),
        ]
        .into_iter()
        .collect()
    );

    seal_exact_cve_scan(&pool, &selected, clock.now() + TimeDelta::minutes(1), None).await;
    let detail = poam_service::detail(&pool, &actor, host_only_id, &clock)
        .await
        .unwrap();
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        host_only_id,
        TransitionPoamRequest {
            revision: detail.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let verified =
        poam_service::verify(&pool, &actor, host_only_id, awaiting.poam.revision, &clock)
            .await
            .unwrap();
    assert_eq!(verified["outcome"], "accepted");
    let closed = poam_service::close(
        &pool,
        &actor,
        host_only_id,
        verified["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    let active_host_after_close: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cve_system_dispositions WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(host_only_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_host_after_close, 0);

    seal_exact_cve_scan(
        &pool,
        &selected,
        clock.now() + TimeDelta::minutes(2),
        Some((cve_id, package_name, "5.0.1", false)),
    )
    .await;
    let reopened = poam_service::reopen(&pool, &actor, host_only_id, closed.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(reopened.poam.status, "in_progress");
    let restored = poam_service::system_cve_triage_detail(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        package_name,
    )
    .await
    .unwrap();
    assert!(matches!(
        restored.host_disposition,
        Some(CveEnvironmentDisposition::Scheduled {
            poam_id: restored_id,
            ..
        }) if restored_id == host_only_id
    ));
    assert!(matches!(
        restored.environment_disposition,
        Some(CveEnvironmentDisposition::Scheduled {
            poam_id: restored_id,
            ..
        }) if restored_id == independent_environment_id
    ));
    assert_eq!(
        restored.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_host_and_environment_triage_serialize_without_scope_theft(pool: PgPool) {
    let selected = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let actor = admin_actor(selected.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-32625";
    let package_name = "host-environment-race";
    assign_environment(&pool, "host-environment-race", &[&selected, &peer]).await;
    for fixture in [&selected, &peer] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "6.0.0", false)),
        )
        .await;
    }
    let host_request = SystemCveTriageRequest {
        canonical_package_name: package_name.into(),
        scope: SystemCveTriageScopeChoice::Host,
        action: SystemCveTriageAction::AcceptRisk {
            justification: "The selected host has compensating controls".into(),
            review_date: None,
        },
        poam: None,
    };
    let environment_request = SystemCveTriageRequest {
        canonical_package_name: package_name.into(),
        scope: SystemCveTriageScopeChoice::Environment,
        action: SystemCveTriageAction::AcceptRisk {
            justification: "The environment has compensating controls".into(),
            review_date: None,
        },
        poam: None,
    };
    let (host, environment) = tokio::join!(
        poam_service::triage_system_cve(
            &pool,
            &actor,
            selected.system_id,
            cve_id,
            host_request,
            &clock
        ),
        poam_service::triage_system_cve(
            &pool,
            &actor,
            peer.system_id,
            cve_id,
            environment_request,
            &clock
        )
    );
    host.unwrap();
    environment.unwrap();
    let detail = poam_service::system_cve_triage_detail(
        &pool,
        &actor,
        selected.system_id,
        cve_id,
        package_name,
    )
    .await
    .unwrap();
    assert!(detail.host_disposition.is_some());
    assert!(detail.environment_disposition.is_some());
    assert_eq!(
        detail.effective_source,
        SystemCveEffectiveDispositionSource::Host
    );
    let active_host_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cve_system_dispositions WHERE retired_at IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(active_host_count, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn system_cve_triage_http_contract_enforces_scope_roles_and_csrf(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let peer = assessment_fixture(&pool).await;
    let environment_id = assign_environment(&pool, "system-triage-http", &[&fixture, &peer]).await;
    let cve_id = "CVE-2026-32622";
    let package_name = "http-system-package";
    for exact in [&fixture, &peer] {
        seal_exact_cve_scan(
            &pool,
            exact,
            Utc::now(),
            Some((cve_id, package_name, "3.0.0", false)),
        )
        .await;
    }
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(fixture.user_id)
        .bind(environment_id)
        .execute(&pool)
        .await
        .unwrap();
    let token = session(&pool, fixture.user_id, AuthRole::Viewer).await;
    let hidden_token = session(&pool, peer.user_id, AuthRole::Viewer).await;
    let base = poam_http_server(pool.clone()).await;
    let url = format!(
        "{base}/api/v1/systems/{}/cves/{cve_id}/triage?package={package_name}",
        fixture.system_id
    );
    let client = reqwest::Client::new();

    let visible = http_request(&client, reqwest::Method::GET, url.clone(), &token, None)
        .send()
        .await
        .unwrap();
    assert_eq!(visible.status(), reqwest::StatusCode::OK);
    let visible: serde_json::Value = visible.json().await.unwrap();
    assert_eq!(
        visible["scope"]["environment_id"],
        environment_id.to_string()
    );
    assert_eq!(visible["scope"]["exact_affected_system_count"], 2);
    assert_eq!(
        visible["scope"]["kind"],
        "current_exact_affected_hosts_in_environment"
    );

    let hidden = http_request(
        &client,
        reqwest::Method::GET,
        url.clone(),
        &hidden_token,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(hidden.status(), reqwest::StatusCode::NOT_FOUND);

    let body = serde_json::json!({
        "canonical_package_name": package_name,
        "scope": "environment",
        "action": "accept_risk",
        "justification": "The environment has documented compensating controls",
        "review_date": null,
        "poam": null
    });
    let forged = http_request(
        &client,
        reqwest::Method::POST,
        url.clone(),
        &token,
        Some("system-triage-forged-scope"),
    )
    .json(&serde_json::json!({
        "canonical_package_name": package_name,
        "scope": "host",
        "action": "accept_risk",
        "justification": "A forged identity must never be accepted",
        "review_date": null,
        "poam": null,
        "system_id": peer.system_id,
        "environment_id": environment_id
    }))
    .send()
    .await
    .unwrap();
    assert_eq!(forged.status(), reqwest::StatusCode::BAD_REQUEST);
    let viewer = http_request(
        &client,
        reqwest::Method::POST,
        url.clone(),
        &token,
        Some("system-triage-viewer"),
    )
    .json(&body)
    .send()
    .await
    .unwrap();
    assert_eq!(viewer.status(), reqwest::StatusCode::FORBIDDEN);

    sync_user_role(&pool, fixture.user_id, AuthRole::Operator)
        .await
        .unwrap();
    let missing_csrf = http_request(&client, reqwest::Method::POST, url.clone(), &token, None)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(missing_csrf.status(), reqwest::StatusCode::FORBIDDEN);
    let accepted = http_request(
        &client,
        reqwest::Method::POST,
        url,
        &token,
        Some("system-triage-operator"),
    )
    .json(&body)
    .send()
    .await
    .unwrap();
    assert_eq!(accepted.status(), reqwest::StatusCode::OK);
    let accepted: serde_json::Value = accepted.json().await.unwrap();
    assert_eq!(
        accepted["detail"]["scope"]["exact_affected_system_count"],
        2
    );
    assert_eq!(accepted["detail"]["disposition"]["state"], "accepted");
}

#[sqlx::test(migrations = "./migrations")]
async fn fleet_cve_detail_returns_typed_group_metadata_for_scheduled_reuse(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let environment_id = assign_environment(&pool, "fleet-group-assignee", &[&fixture]).await;
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44011";
    let package_name = "group-package";
    seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some((cve_id, package_name, "1.0", false)),
    )
    .await;
    sqlx::query("INSERT INTO oidc_group_mappings(group_name) VALUES('fleet-maintainers')")
        .execute(&pool)
        .await
        .unwrap();
    let mut poam = fleet_poam_request(actor.user_id, &clock);
    poam.assignee = PoamAssigneeRequest::OidcGroup {
        group_name: "fleet-maintainers".into(),
    };

    let response = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch { environment_id }],
            poam: Some(poam),
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(matches!(
        response.detail.environments[0].disposition.as_ref(),
        Some(CveEnvironmentDisposition::Scheduled { poam: Some(poam), .. })
            if matches!(&poam.assignee, PoamAssigneeView::OidcGroup {
                group_name,
                available: true,
                ..
            } if group_name == "fleet-maintainers")
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn scheduled_list_authority_covers_current_subjects_with_historical_surplus(pool: PgPool) {
    let first = assessment_fixture(&pool).await;
    let second = assessment_fixture(&pool).await;
    let new_subject = assessment_fixture(&pool).await;
    let actor = admin_actor(first.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44016";
    let package_name = "set-authority";
    let environment_id =
        assign_environment(&pool, "set-authority-original", &[&first, &second]).await;
    for fixture in [&first, &second] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "1.0", false)),
        )
        .await;
    }
    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch { environment_id }],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "scheduled").await;
    assert!(matches!(
        scheduled.detail.environments[0].disposition,
        Some(CveEnvironmentDisposition::Scheduled { poam: Some(_), .. })
    ));

    sqlx::query("UPDATE users SET is_active=FALSE WHERE id=$1")
        .bind(actor.user_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_cve_open_everywhere(&pool, &actor, cve_id, package_name).await;
    sqlx::query("UPDATE users SET is_active=TRUE,user_type='service' WHERE id=$1")
        .bind(actor.user_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_cve_open_everywhere(&pool, &actor, cve_id, package_name).await;
    sqlx::query("UPDATE users SET user_type='human' WHERE id=$1")
        .bind(actor.user_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "scheduled").await;

    sqlx::query("INSERT INTO oidc_group_mappings(group_name) VALUES('coherence-operators')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"UPDATE poams
           SET owner='coherence-operators',owner_kind='oidc_group',
               owner_user_id=NULL,owner_group_name='coherence-operators'
           WHERE id=$1"#,
    )
    .bind(poam_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "scheduled").await;
    let group_detail = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert!(matches!(
        group_detail.environments[0].disposition.as_ref(),
        Some(CveEnvironmentDisposition::Scheduled {
            poam: Some(poam),
            ..
        }) if matches!(&poam.assignee, PoamAssigneeView::OidcGroup {
            available: true,
            ..
        })
    ));

    sqlx::query("DELETE FROM oidc_group_mappings WHERE group_name='coherence-operators'")
        .execute(&pool)
        .await
        .unwrap();
    assert_cve_open_everywhere(&pool, &actor, cve_id, package_name).await;

    sqlx::query(
        r#"UPDATE poams
           SET owner='Legacy Operations',owner_kind=NULL,
               owner_user_id=NULL,owner_group_name=NULL
           WHERE id=$1"#,
    )
    .bind(poam_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_cve_open_everywhere(&pool, &actor, cve_id, package_name).await;
    sqlx::query("UPDATE poams SET owner='' WHERE id=$1")
        .bind(poam_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_cve_open_everywhere(&pool, &actor, cve_id, package_name).await;

    sqlx::query(
        r#"UPDATE poams
           SET owner='restored actor',owner_kind='user',owner_user_id=$2,
               owner_group_name=NULL
           WHERE id=$1"#,
    )
    .bind(poam_id)
    .bind(actor.user_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "scheduled").await;

    sqlx::query("UPDATE systems SET environment_id=$1 WHERE id=$2")
        .bind(environment_id)
        .bind(new_subject.system_id)
        .execute(&pool)
        .await
        .unwrap();
    seal_exact_cve_scan(
        &pool,
        &new_subject,
        clock.now(),
        Some((cve_id, package_name, "1.0", false)),
    )
    .await;
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "outstanding").await;
    let added = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert_eq!(added.rollup, FleetCveTriageRollup::Outstanding);
    assert!(added.environments[0].disposition.is_none());

    sqlx::query("UPDATE systems SET is_active=FALSE WHERE id=$1")
        .bind(new_subject.system_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "scheduled").await;

    sqlx::query("UPDATE systems SET is_active=FALSE WHERE id=$1")
        .bind(second.system_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "scheduled").await;
    let stale = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert!(matches!(
        stale.environments[0].disposition,
        Some(CveEnvironmentDisposition::Scheduled { .. })
    ));

    sqlx::query("UPDATE systems SET is_active=TRUE WHERE id=$1")
        .bind(second.system_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "scheduled").await;

    let moved_environment = assign_environment(&pool, "set-authority-moved", &[&second]).await;
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "outstanding").await;
    let moved = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert_eq!(moved.rollup, FleetCveTriageRollup::Partial);
    assert!(
        moved
            .environments
            .iter()
            .find(|environment| environment.environment_id == moved_environment)
            .unwrap()
            .disposition
            .is_none(),
        "the moved subject's environment must be OPEN"
    );

    sqlx::query("UPDATE systems SET environment_id=$1 WHERE id=$2")
        .bind(environment_id)
        .bind(second.system_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "scheduled").await;

    sqlx::query(
        r#"UPDATE poam_cve_finding_links
           SET retired_at=$3,retired_by=$4,retirement_reason='test_retired_link'
           WHERE poam_id=$1 AND system_id=$2 AND retired_at IS NULL"#,
    )
    .bind(poam_id)
    .bind(second.system_id)
    .bind(clock.now())
    .bind(actor.user_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_cve_authority_surfaces(&pool, cve_id, package_name, "outstanding").await;
    let retired = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert!(retired.environments[0].disposition.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_cve_list_group_filters_and_stats_share_package_scoped_authority(pool: PgPool) {
    let mut fixtures = Vec::new();
    for _ in 0..6 {
        fixtures.push(assessment_fixture(&pool).await);
    }
    let actor = admin_actor(fixtures[0].user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let accepted_cve = "CVE-2026-44101";
    let scheduled_cve = "CVE-2026-44102";
    let mixed_cve = "CVE-2026-44103";
    let accepted_package = "authority-alpha";
    let open_package = "authority-beta";
    let scheduled_package = "authority-gamma";
    let mixed_package = "authority-delta";

    let accepted_environment =
        assign_environment(&pool, "authority-accepted", &[&fixtures[0], &fixtures[1]]).await;
    let open_environment = assign_environment(&pool, "authority-open", &[&fixtures[2]]).await;
    let scheduled_environment =
        assign_environment(&pool, "authority-scheduled", &[&fixtures[3]]).await;
    let mixed_accepted_environment =
        assign_environment(&pool, "authority-mixed-accepted", &[&fixtures[4]]).await;
    let mixed_open_environment =
        assign_environment(&pool, "authority-mixed-open", &[&fixtures[5]]).await;

    for (fixture, cve_id, package_name) in [
        (&fixtures[0], accepted_cve, accepted_package),
        (&fixtures[1], accepted_cve, accepted_package),
        (&fixtures[2], accepted_cve, open_package),
        (&fixtures[3], scheduled_cve, scheduled_package),
        (&fixtures[4], mixed_cve, mixed_package),
        (&fixtures[5], mixed_cve, mixed_package),
    ] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "1.0.0", false)),
        )
        .await;
    }
    sqlx::query(
        r#"UPDATE cves SET cvss_v3_score=CASE id
             WHEN $1 THEN 9.8 WHEN $2 THEN 8.1 WHEN $3 THEN 5.5 END,
             description='Exact authority ' || id
           WHERE id=ANY($4)"#,
    )
    .bind(accepted_cve)
    .bind(scheduled_cve)
    .bind(mixed_cve)
    .bind(vec![accepted_cve, scheduled_cve, mixed_cve])
    .execute(&pool)
    .await
    .unwrap();

    poam_service::triage_fleet_cve(
        &pool,
        &actor,
        accepted_cve,
        FleetCveTriageRequest {
            canonical_package_name: accepted_package.into(),
            actions: vec![CveEnvironmentTriageAction::AcceptRisk {
                environment_id: accepted_environment,
                justification: "Accepted exact authority test risk".into(),
                review_date: Some(clock.today() + TimeDelta::days(30)),
            }],
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();
    poam_service::triage_fleet_cve(
        &pool,
        &actor,
        scheduled_cve,
        FleetCveTriageRequest {
            canonical_package_name: scheduled_package.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch {
                environment_id: scheduled_environment,
            }],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    poam_service::triage_fleet_cve(
        &pool,
        &actor,
        mixed_cve,
        FleetCveTriageRequest {
            canonical_package_name: mixed_package.into(),
            actions: vec![
                CveEnvironmentTriageAction::AcceptRisk {
                    environment_id: mixed_accepted_environment,
                    justification: "Accepted one exact authority environment".into(),
                    review_date: None,
                },
                CveEnvironmentTriageAction::LeaveOpen {
                    environment_id: mixed_open_environment,
                },
            ],
            poam: None,
        },
        &clock,
    )
    .await
    .unwrap();

    let rows = fetch_cve_list(&pool, &CveReadScope::All, &CveFilters::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    let accepted = rows
        .iter()
        .find(|row| row.package_name.as_deref() == Some(accepted_package))
        .unwrap();
    assert_eq!(accepted.affected_count, 2);
    assert_eq!(accepted.triage_status, "accepted");
    assert_eq!(
        rows.iter()
            .find(|row| row.package_name.as_deref() == Some(open_package))
            .unwrap()
            .triage_status,
        "outstanding",
        "a disposition for one package must not affect another package for the same CVE"
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.package_name.as_deref() == Some(scheduled_package))
            .unwrap()
            .triage_status,
        "scheduled"
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.package_name.as_deref() == Some(mixed_package))
            .unwrap()
            .triage_status,
        "outstanding"
    );

    for (filters, expected_package) in [
        (
            CveFilters {
                triage_status: Some("accepted".into()),
                ..Default::default()
            },
            accepted_package,
        ),
        (
            CveFilters {
                triage_status: Some("scheduled".into()),
                ..Default::default()
            },
            scheduled_package,
        ),
        (
            CveFilters {
                package: Some("beta".into()),
                ..Default::default()
            },
            open_package,
        ),
        (
            CveFilters {
                search: Some("authority-delta".into()),
                ..Default::default()
            },
            mixed_package,
        ),
    ] {
        let filtered = fetch_cve_list(&pool, &CveReadScope::All, &filters)
            .await
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].package_name.as_deref(), Some(expected_package));
    }
    let critical = fetch_cve_list(
        &pool,
        &CveReadScope::All,
        &CveFilters {
            severity: Some("critical".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(critical.len(), 2);

    let groups = fetch_cve_packages_grouped(&pool, &CveReadScope::All, &CveFilters::default())
        .await
        .unwrap();
    assert_eq!(groups.len(), 4);
    let accepted_group = groups
        .iter()
        .find(|group| group.package_name == accepted_package)
        .unwrap();
    assert_eq!(accepted_group.cve_count, 1);
    assert_eq!(accepted_group.total_affected_systems, 2);
    assert_eq!(accepted_group.environments_count, 1);
    assert_eq!(accepted_group.outstanding_count, 0);

    let stats = fetch_cve_fleet_stats(&pool, &CveReadScope::All)
        .await
        .unwrap();
    assert_eq!(stats.total_cves, 4);
    assert_eq!(stats.critical, 2);
    assert_eq!(stats.high, 1);
    assert_eq!(stats.medium, 1);
    assert_eq!(stats.systems_affected, 6);
    assert_eq!(stats.environments_affected, 5);
    assert_eq!(stats.accepted, 1);
    assert_eq!(stats.scheduled, 1);
    assert_eq!(stats.outstanding, 2);

    assert_ne!(open_environment, accepted_environment);
}

#[sqlx::test(migrations = "./migrations")]
async fn fleet_cve_inventory_separates_current_scheduled_and_historical_scope(pool: PgPool) {
    let current = assessment_fixture(&pool).await;
    let scheduled = assessment_fixture(&pool).await;
    let overlap = assessment_fixture(&pool).await;
    let historical = assessment_fixture(&pool).await;
    let expired = assessment_fixture(&pool).await;
    let terminal = assessment_fixture(&pool).await;
    let superseded_exact = assessment_fixture(&pool).await;
    let hidden = assessment_fixture(&pool).await;
    let unassigned_historical = assessment_fixture(&pool).await;
    let cve_id = "CVE-2026-44990";
    let hidden_cve_id = "CVE-2026-44991";
    let package_name = "fleet-section-package";
    let visible_environment = assign_environment(
        &pool,
        "fleet-section-visible",
        &[
            &current,
            &scheduled,
            &overlap,
            &historical,
            &expired,
            &terminal,
            &superseded_exact,
        ],
    )
    .await;
    let hidden_environment = assign_environment(&pool, "fleet-section-hidden", &[&hidden]).await;

    seal_exact_cve_scan(
        &pool,
        &current,
        Utc::now(),
        Some((cve_id, package_name, "1.0-current", false)),
    )
    .await;
    seal_exact_cve_scan(
        &pool,
        &overlap,
        Utc::now(),
        Some((cve_id, package_name, "1.0-overlap", false)),
    )
    .await;
    add_scheduled_cve_target(
        &pool,
        &scheduled,
        Some((cve_id, package_name, "2.0-scheduled", false)),
    )
    .await;
    add_scheduled_cve_target(
        &pool,
        &overlap,
        Some((cve_id, package_name, "2.0-overlap", false)),
    )
    .await;
    add_historical_cve_inventory(&pool, &historical, cve_id, package_name).await;
    sqlx::query(
        r#"WITH scan AS (
             INSERT INTO cve_scans(
               derivation_id,scanner_name,status,completed_at,evidence_schema_version)
             VALUES($1,'historical-test','completed',now(),0) RETURNING id
           )
           INSERT INTO scan_packages(scan_id,derivation_id)
           SELECT scan.id,package.id FROM scan
           JOIN derivations package
             ON package.derivation_type='package' AND package.pname=$2"#,
    )
    .bind(unassigned_historical.derivation_id)
    .bind(package_name)
    .execute(&pool)
    .await
    .unwrap();

    let expired_id = add_scheduled_cve_target(
        &pool,
        &expired,
        Some((cve_id, package_name, "3.0-expired", false)),
    )
    .await;
    sqlx::query(
        "UPDATE pending_system_deployments SET expires_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(expired_id)
    .execute(&pool)
    .await
    .unwrap();
    let terminal_id = add_scheduled_cve_target(
        &pool,
        &terminal,
        Some((cve_id, package_name, "3.0-terminal", false)),
    )
    .await;
    sqlx::query("UPDATE pending_system_deployments SET status='failed' WHERE id=$1")
        .bind(terminal_id)
        .execute(&pool)
        .await
        .unwrap();
    add_scheduled_cve_target(
        &pool,
        &superseded_exact,
        Some((cve_id, package_name, "3.0-older-exact", false)),
    )
    .await;
    let superseded_store = format!("/nix/store/{}-inexact", Uuid::new_v4().simple());
    sqlx::query(
        r#"INSERT INTO pending_system_deployments(
             system_id,target_store_path,status,expires_at,issued_at)
           VALUES($1,$2,'pending',now()+interval '1 hour',now()+interval '1 minute')"#,
    )
    .bind(superseded_exact.system_id)
    .bind(superseded_store)
    .execute(&pool)
    .await
    .unwrap();
    add_scheduled_cve_target(
        &pool,
        &hidden,
        Some((hidden_cve_id, "hidden-section-package", "9.0", false)),
    )
    .await;

    let scope = CveReadScope::Environments(vec![visible_environment]);
    let rows = fetch_cve_list(&pool, &scope, &CveFilters::default())
        .await
        .unwrap();
    let row = rows
        .iter()
        .find(|row| row.cve_id == cve_id && row.package_name.as_deref() == Some(package_name))
        .unwrap();
    assert_eq!(row.current_affected_count, 2);
    assert_eq!(row.scheduled_deployment_target_count, 2);
    assert_eq!(row.historical_inventory_count, 1);
    assert_eq!(
        row.affected_count, 3,
        "current/scheduled overlap is distinct"
    );
    assert_eq!(row.exact_affected_count, 3);
    assert_eq!(row.legacy_affected_count, 1);
    assert!(!rows.iter().any(|row| row.cve_id == hidden_cve_id));

    let systems = crystal_forge::queries::cves::fetch_cve_inventory_systems(
        &pool,
        &scope,
        cve_id,
        Some(package_name),
    )
    .await
    .unwrap();
    assert_eq!(
        systems
            .iter()
            .filter(|system| system.inventory_section == FleetCveInventorySection::Current)
            .count(),
        2
    );
    assert_eq!(
        systems
            .iter()
            .filter(|system| {
                system.inventory_section == FleetCveInventorySection::ScheduledDeploymentTarget
            })
            .count(),
        2
    );
    assert_eq!(
        systems
            .iter()
            .filter(|system| system.inventory_section == FleetCveInventorySection::Historical)
            .count(),
        1
    );
    assert_eq!(
        systems
            .iter()
            .map(|system| system.system_id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        4,
        "five section rows represent four distinct systems because overlap is preserved"
    );
    assert!(!systems.iter().any(|system| {
        [
            expired.system_id,
            terminal.system_id,
            superseded_exact.system_id,
        ]
        .contains(&system.system_id)
    }));

    let stats = fetch_cve_fleet_stats(&pool, &scope).await.unwrap();
    assert_eq!(stats.systems_affected, 3);
    assert_eq!(stats.current_systems_affected, 2);
    assert_eq!(stats.scheduled_deployment_target_systems, 2);
    assert_eq!(stats.historical_inventory_systems, 1);
    assert_eq!(stats.environments_affected, 1);
    assert_eq!(
        stats.no_scan_systems, 0,
        "scheduled exact scans count as scans"
    );

    let actor = PoamActor {
        user_id: current.user_id,
        identifier: "fleet-section-operator@example.invalid".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: vec![visible_environment],
        request_origin: None,
    };
    let detail = poam_service::fleet_cve_inventory_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert_eq!(detail.current_affected_system_count, 2);
    assert_eq!(detail.scheduled_deployment_target_count, 2);
    assert_eq!(detail.historical_inventory_system_count, 1);
    assert_eq!(detail.affected_system_count, 3);
    assert_eq!(detail.exact_mutation_target_count, 2);
    assert_eq!(detail.environments.len(), 1);

    let admin_actor = PoamActor {
        user_id: current.user_id,
        identifier: "fleet-section-admin@example.invalid".into(),
        is_admin: true,
        can_mutate: true,
        environment_ids: Vec::new(),
        request_origin: None,
    };
    let admin_detail =
        poam_service::fleet_cve_inventory_detail(&pool, &admin_actor, cve_id, package_name)
            .await
            .unwrap();
    assert_eq!(admin_detail.unassigned_systems.len(), 1);
    assert_eq!(admin_detail.unassigned_affected_system_count, 0);
    assert_eq!(admin_detail.historical_inventory_system_count, 2);

    let scheduled_actor = PoamActor {
        user_id: scheduled.user_id,
        identifier: "scheduled-only@example.invalid".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: vec![visible_environment],
        request_origin: None,
    };
    let scheduled_only_cve = "CVE-2026-44992";
    let scheduled_only_deployment = add_scheduled_cve_target(
        &pool,
        &scheduled,
        Some((scheduled_only_cve, "scheduled-only-package", "1.0", false)),
    )
    .await;
    sqlx::query(
        "UPDATE pending_system_deployments SET issued_at=now()+interval '2 minutes' WHERE id=$1",
    )
    .bind(scheduled_only_deployment)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        poam_service::fleet_cve_detail(
            &pool,
            &scheduled_actor,
            scheduled_only_cve,
            "scheduled-only-package"
        )
        .await,
        Err(PoamError::NotFound)
    ));
    let mutation = poam_service::triage_fleet_cve(
        &pool,
        &scheduled_actor,
        scheduled_only_cve,
        FleetCveTriageRequest {
            canonical_package_name: "scheduled-only-package".into(),
            actions: vec![CveEnvironmentTriageAction::LeaveOpen {
                environment_id: visible_environment,
            }],
            poam: None,
        },
        &FixedClock(Utc::now()),
    )
    .await;
    assert!(
        mutation.is_err(),
        "scheduled-only inventory cannot mutate triage"
    );
    let mutation_writes: i64 = sqlx::query_scalar(
        r#"SELECT
             (SELECT count(*) FROM cve_environment_dispositions
              WHERE canonical_cve_id=$1 AND canonical_package_name=$2)
             +
             (SELECT count(*) FROM poam_cve_finding_links
              WHERE canonical_cve_id=$1 AND canonical_package_name=$2)"#,
    )
    .bind(scheduled_only_cve)
    .bind("scheduled-only-package")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(mutation_writes, 0);
    assert_ne!(visible_environment, hidden_environment);
}

#[sqlx::test(migrations = "./migrations")]
async fn cve_export_ignores_list_pagination_and_preserves_long_exact_identity(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    assign_environment(&pool, "export-complete", &[&fixture]).await;
    let scan_id = begin_exact_cve_scan(&pool, &fixture, 3).await;
    let long_package = "p".repeat(300);
    let long_version = "1.".to_string() + &"9".repeat(180);
    sqlx::query("INSERT INTO cves(id) VALUES('CVE-2097-10001'),('CVE-2097-10002')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO cve_scan_vulnerability_observations(
             scan_id,canonical_cve_id,canonical_package_name,
             observed_package_name,observed_package_version,
             observed_derivation_path,is_whitelisted,detection_method)
            VALUES($1,'CVE-2097-10001',$2,$2,$3,
                   '/nix/store/export-long-package.drv',FALSE,'test-scanner'),
                  ($1,'CVE-2097-10001','aaa-package','aaa-package','1.0',
                   '/nix/store/export-aaa-package.drv',FALSE,'test-scanner'),
                  ($1,'CVE-2097-10002','short-package','short-package','2.0',
                  '/nix/store/export-short-package.drv',FALSE,'test-scanner')"#,
    )
    .bind(scan_id)
    .bind(&long_package)
    .bind(&long_version)
    .execute(&pool)
    .await
    .unwrap();
    complete_exact_cve_scan(&pool, scan_id).await;

    let list = fetch_cve_list(&pool, &CveReadScope::All, &CveFilters::default())
        .await
        .unwrap();
    let long_row = list
        .iter()
        .find(|row| {
            row.cve_id == "CVE-2097-10001"
                && row.package_name.as_deref() == Some(long_package.as_str())
        })
        .unwrap();
    assert_eq!(
        long_row.package_name.as_deref(),
        Some(long_package.as_str())
    );
    assert_eq!(
        long_row.installed_version.as_deref(),
        Some(long_version.as_str())
    );
    let detail = fetch_cve_detail(&pool, &CveReadScope::All, "CVE-2097-10001")
        .await
        .unwrap();
    assert_eq!(detail.package_name.as_deref(), Some("aaa-package"));
    assert_eq!(detail.installed_version.as_deref(), Some("1.0"));

    for sort in ["severity", "cvss", "age", "affected"] {
        let tied = fetch_cve_list(
            &pool,
            &CveReadScope::All,
            &CveFilters {
                search: Some("CVE-2097-10001".into()),
                sort: Some(sort.into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            tied.iter()
                .filter_map(|row| row.package_name.as_deref())
                .collect::<Vec<_>>(),
            vec!["aaa-package", long_package.as_str()],
            "{sort} ordering must use canonical package name after CVE ID"
        );
    }

    let filters = CveFilters {
        limit: Some(1),
        ..Default::default()
    };
    let exported = fetch_cves_for_export(&pool, &CveReadScope::All, &filters)
        .await
        .unwrap();
    assert_eq!(exported.len(), 3);
    let tied_export = fetch_cves_for_export(
        &pool,
        &CveReadScope::All,
        &CveFilters {
            search: Some("CVE-2097-10001".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        tied_export
            .iter()
            .filter_map(|row| row.package_name.as_deref())
            .collect::<Vec<_>>(),
        vec!["aaa-package", long_package.as_str()]
    );
    let (_, token) = role_session(&pool, AuthRole::Admin).await;
    let base = poam_http_server(pool.clone()).await;
    let response = reqwest::Client::new()
        .get(format!("{base}/api/v1/cves/export?limit=1"))
        .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let csv = response.text().await.unwrap();
    assert_eq!(csv.lines().count(), 4, "header plus all complete rows");
    assert!(csv.contains(&long_package));
    assert!(csv.contains(&long_version));
}

#[sqlx::test(migrations = "./migrations")]
async fn cve_export_rejects_results_above_the_documented_bound(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    assign_environment(&pool, "export-overflow", &[&fixture]).await;
    let row_count = MAX_CVE_EXPORT_ROWS + 1;
    let scan_id = begin_exact_cve_scan(&pool, &fixture, row_count as i32).await;
    sqlx::query(
        r#"INSERT INTO cves(id)
           SELECT 'CVE-2098-' || lpad(value::text,6,'0')
           FROM generate_series(1,$1) value"#,
    )
    .bind(row_count)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO cve_scan_vulnerability_observations(
             scan_id,canonical_cve_id,canonical_package_name,
             observed_package_name,observed_package_version,
             observed_derivation_path,is_whitelisted,detection_method)
           SELECT $1,cve.id,'overflow-' || cve.id,'overflow-' || cve.id,'1.0',
                  '/nix/store/' || lower(cve.id) || '.drv',FALSE,'test-scanner'
           FROM cves cve WHERE cve.id LIKE 'CVE-2098-%'"#,
    )
    .bind(scan_id)
    .execute(&pool)
    .await
    .unwrap();
    complete_exact_cve_scan(&pool, scan_id).await;

    assert!(matches!(
        fetch_cves_for_export(&pool, &CveReadScope::All, &CveFilters::default()).await,
        Err(CveExportError::TooManyRows)
    ));
    let (_, token) = role_session(&pool, AuthRole::Admin).await;
    let base = poam_http_server(pool.clone()).await;
    let response = reqwest::Client::new()
        .get(format!("{base}/api/v1/cves/export"))
        .header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    let body = response.text().await.unwrap();
    assert!(body.contains("1000-row limit"));
    assert!(body.contains("narrow the filters"));
}

#[sqlx::test(migrations = "./migrations")]
async fn cve_dashboard_reads_scope_before_aggregation_and_count_distinct_hosts(pool: PgPool) {
    let first = assessment_fixture(&pool).await;
    let second = assessment_fixture(&pool).await;
    let hidden = assessment_fixture(&pool).await;
    let unassigned = assessment_fixture(&pool).await;
    let development = assign_environment(&pool, "Development", &[&first, &second]).await;
    let production = assign_environment(&pool, "Production", &[&hidden]).await;
    let cve = "CVE-2026-44901";
    let overlapping_cve = "CVE-2026-44902";
    let hidden_cve = "CVE-2026-44903";
    let package = "scope-alpha";
    let second_package = "scope-beta";
    let hidden_package = "scope-secret";
    let clock = Utc::now();

    let first_references = seal_exact_cve_scan_many(
        &pool,
        &first,
        clock,
        &[
            (cve, package, "1.0.0", false),
            (overlapping_cve, package, "1.0.0", false),
            (cve, second_package, "2.0.0", false),
        ],
    )
    .await;
    for (reference, fixed_version) in first_references.iter().zip(["1.1.0", "1.1.0", "2.1.0"]) {
        sqlx::query(
            r#"INSERT INTO package_vulnerabilities(
                 derivation_id,cve_id,fixed_version,is_whitelisted)
               SELECT id,$2,$3,FALSE FROM derivations WHERE derivation_path=$1"#,
        )
        .bind(&reference.occurrence_derivation_path)
        .bind(&reference.canonical_cve_id)
        .bind(fixed_version)
        .execute(&pool)
        .await
        .unwrap();
    }
    let hidden_references = seal_exact_cve_scan_many(
        &pool,
        &hidden,
        clock,
        &[
            (hidden_cve, hidden_package, "9.0.0", false),
            (cve, package, "9.0.0", false),
        ],
    )
    .await;
    let hidden_package_reference = hidden_references
        .iter()
        .find(|reference| reference.canonical_cve_id == cve)
        .unwrap();
    sqlx::query(
        r#"INSERT INTO package_vulnerabilities(
             derivation_id,cve_id,fixed_version,is_whitelisted)
           SELECT id,$2,'9.9.0',FALSE FROM derivations WHERE derivation_path=$1"#,
    )
    .bind(&hidden_package_reference.occurrence_derivation_path)
    .bind(cve)
    .execute(&pool)
    .await
    .unwrap();
    seal_exact_cve_scan(
        &pool,
        &unassigned,
        clock,
        Some((hidden_cve, "scope-unassigned", "9.0.0", false)),
    )
    .await;

    let development_scope = CveReadScope::Environments(vec![development]);
    let first_groups =
        fetch_cve_packages_grouped(&pool, &development_scope, &CveFilters::default())
            .await
            .unwrap();
    assert_eq!(
        first_groups
            .iter()
            .find(|group| group.package_name == package)
            .unwrap()
            .total_affected_systems,
        1,
        "two CVEs on one host must count as one affected system",
    );
    assert_eq!(
        fetch_cve_fleet_stats(&pool, &development_scope)
            .await
            .unwrap()
            .systems_affected,
        1,
    );

    seal_exact_cve_scan(
        &pool,
        &second,
        clock + TimeDelta::seconds(1),
        Some((cve, package, "1.0.0", false)),
    )
    .await;
    for (user_id, role) in [
        (first.user_id, AuthRole::Viewer),
        (second.user_id, AuthRole::Operator),
    ] {
        sync_user_role(&pool, user_id, role).await.unwrap();
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
        )
        .bind(user_id)
        .bind(development)
        .execute(&pool)
        .await
        .unwrap();
        let resolved = CveReadScope::for_user(
            &pool,
            &AuthenticatedUser {
                user_id,
                roles: vec![role],
            },
        )
        .await
        .unwrap();
        assert_eq!(resolved, development_scope);
    }
    sync_user_role(&pool, hidden.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let admin_scope = CveReadScope::for_user(
        &pool,
        &AuthenticatedUser {
            user_id: hidden.user_id,
            roles: vec![AuthRole::Admin],
        },
    )
    .await
    .unwrap();
    assert_eq!(admin_scope, CveReadScope::All);

    for scope in [
        CveReadScope::Environments(vec![development]),
        CveReadScope::Environments(vec![development]),
    ] {
        let rows = fetch_cve_list(&pool, &scope, &CveFilters::default())
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| {
            row.affected_environments.as_deref() == Some(&["Development".to_string()])
        }));
        assert!(!rows.iter().any(|row| row.cve_id == hidden_cve));

        let groups = fetch_cve_packages_grouped(&pool, &scope, &CveFilters::default())
            .await
            .unwrap();
        let package_group = groups
            .iter()
            .find(|group| group.package_name == package)
            .unwrap();
        assert_eq!(package_group.cve_count, 2);
        assert_eq!(package_group.total_affected_systems, 2);
        assert_eq!(package_group.environments_count, 1);
        let stats = fetch_cve_fleet_stats(&pool, &scope).await.unwrap();
        assert_eq!(stats.total_cves, 3);
        assert_eq!(stats.systems_affected, 2);
        assert_eq!(stats.environments_affected, 1);
        assert_eq!(
            fetch_package_names(&pool, &scope).await.unwrap(),
            vec![package.to_string(), second_package.to_string()],
        );
        assert_eq!(
            fetch_cve_detail(&pool, &scope, cve)
                .await
                .unwrap()
                .package_name
                .as_deref(),
            Some(package),
        );
        assert!(fetch_cve_detail(&pool, &scope, hidden_cve).await.is_err());
        assert_eq!(
            fetch_cve_affected_systems(&pool, &scope, cve)
                .await
                .unwrap()
                .len(),
            2,
        );
    }

    sqlx::query(
        r#"INSERT INTO system_cve_justifications(
             system_id,cve_id,category,reason,updated_by)
           VALUES($1,$3,'mitigated','visible development history',$4),
                 ($2,$3,'mitigated','hidden production history',$4),
                 (NULL,$3,'mitigated','fleet history',$4)"#,
    )
    .bind(first.system_id)
    .bind(hidden.system_id)
    .bind(cve)
    .bind(hidden.user_id)
    .execute(&pool)
    .await
    .unwrap();
    let scoped_history = fetch_cve_justifications(&pool, &development_scope, cve)
        .await
        .unwrap();
    assert_eq!(scoped_history.len(), 1);
    assert_eq!(scoped_history[0].system_id, Some(first.system_id));
    assert_eq!(
        fetch_cve_justifications(&pool, &admin_scope, cve)
            .await
            .unwrap()
            .len(),
        3,
    );

    let viewer_token = session(&pool, first.user_id, AuthRole::Viewer).await;
    let operator_token = session(&pool, second.user_id, AuthRole::Operator).await;
    let admin_token = session(&pool, hidden.user_id, AuthRole::Admin).await;
    let base = poam_http_server(pool.clone()).await;
    let client = reqwest::Client::new();
    for token in [&viewer_token, &operator_token] {
        let rows: serde_json::Value = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/cves"),
            token,
            None,
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 3);
        assert!(!rows.to_string().contains(hidden_package));

        let groups: serde_json::Value = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/cves/grouped"),
            token,
            None,
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(groups.as_array().unwrap().len(), 2);
        assert!(!groups.to_string().contains(hidden_package));

        let stats: serde_json::Value = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/cves/stats"),
            token,
            None,
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(stats["systems_affected"], 2);
        assert_eq!(stats["environments_affected"], 1);

        let packages: serde_json::Value = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/cves/packages"),
            token,
            None,
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(packages.as_array().unwrap().len(), 2);
        assert!(!packages.to_string().contains(hidden_package));

        let visible_detail = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/cves/{cve}"),
            token,
            None,
        )
        .send()
        .await
        .unwrap();
        assert_eq!(visible_detail.status(), reqwest::StatusCode::OK);
        let hidden_detail = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/cves/{hidden_cve}"),
            token,
            None,
        )
        .send()
        .await
        .unwrap();
        assert_eq!(hidden_detail.status(), reqwest::StatusCode::NOT_FOUND);

        let systems: serde_json::Value = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/cves/{cve}/systems"),
            token,
            None,
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(systems.as_array().unwrap().len(), 2);
        let hidden_systems: serde_json::Value = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/cves/{hidden_cve}/systems"),
            token,
            None,
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert!(hidden_systems.as_array().unwrap().is_empty());

        let history: serde_json::Value = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/cves/{cve}/justifications"),
            token,
            None,
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(history.as_array().unwrap().len(), 1);

        let export = http_request(
            &client,
            reqwest::Method::GET,
            format!("{base}/api/v1/cves/export"),
            token,
            None,
        )
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
        assert!(export.contains(package));
        assert!(!export.contains(hidden_package));
    }

    let admin_rows: serde_json::Value = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/cves"),
        &admin_token,
        None,
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(admin_rows.as_array().unwrap().len(), 5);
    assert!(admin_rows.to_string().contains(hidden_package));
    let admin_groups: serde_json::Value = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/cves/grouped"),
        &admin_token,
        None,
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(admin_groups.as_array().unwrap().len(), 4);
    assert!(admin_groups.to_string().contains(hidden_package));
    let admin_stats: serde_json::Value = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/cves/stats"),
        &admin_token,
        None,
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(admin_stats["systems_affected"], 4);
    let admin_packages: serde_json::Value = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/cves/packages"),
        &admin_token,
        None,
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(admin_packages.as_array().unwrap().len(), 4);
    assert!(admin_packages.to_string().contains(hidden_package));
    let admin_hidden_detail = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/cves/{hidden_cve}"),
        &admin_token,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(admin_hidden_detail.status(), reqwest::StatusCode::OK);
    let admin_hidden_systems: serde_json::Value = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/cves/{hidden_cve}/systems"),
        &admin_token,
        None,
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(admin_hidden_systems.as_array().unwrap().len(), 2);
    let admin_history: serde_json::Value = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/cves/{cve}/justifications"),
        &admin_token,
        None,
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(admin_history.as_array().unwrap().len(), 3);
    let admin_export = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/cves/export"),
        &admin_token,
        None,
    )
    .send()
    .await
    .unwrap()
    .text()
    .await
    .unwrap();
    assert!(admin_export.contains(hidden_package));

    let admin_rows = fetch_cve_list(&pool, &admin_scope, &CveFilters::default())
        .await
        .unwrap();
    assert!(admin_rows.iter().any(|row| row.cve_id == hidden_cve));
    assert!(
        admin_rows
            .iter()
            .any(|row| { row.package_name.as_deref() == Some("scope-unassigned") })
    );
    assert_eq!(
        fetch_cve_fleet_stats(&pool, &admin_scope)
            .await
            .unwrap()
            .systems_affected,
        4,
    );

    let actor = admin_actor(hidden.user_id);
    let package_detail = poam_service::fleet_cve_detail(&pool, &actor, cve, package)
        .await
        .unwrap();
    assert_eq!(package_detail.cve.package_name.as_deref(), Some(package));
    assert_eq!(
        package_detail.cve.installed_version.as_deref(),
        Some("9.0.0")
    );
    assert_eq!(package_detail.cve.fixed_version.as_deref(), Some("9.9.0"));
    assert_eq!(package_detail.cve.fix_status, "fix_available");
    let second_detail = poam_service::fleet_cve_detail(&pool, &actor, cve, second_package)
        .await
        .unwrap();
    assert_eq!(
        second_detail.cve.package_name.as_deref(),
        Some(second_package),
    );
    assert_eq!(
        second_detail.cve.installed_version.as_deref(),
        Some("2.0.0")
    );
    assert_eq!(second_detail.cve.fixed_version.as_deref(), Some("2.1.0"));
    assert_eq!(second_detail.cve.fix_status, "fix_available");
    let scoped_actor = PoamActor {
        user_id: second.user_id,
        identifier: "development-operator@example.invalid".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: vec![development],
        request_origin: None,
    };
    let scoped_detail = poam_service::fleet_cve_detail(&pool, &scoped_actor, cve, package)
        .await
        .unwrap();
    assert_eq!(scoped_detail.affected_system_count, 2);
    assert_eq!(
        scoped_detail.cve.installed_version.as_deref(),
        Some("1.0.0")
    );
    assert_eq!(scoped_detail.cve.fixed_version.as_deref(), Some("1.1.0"));
    assert_eq!(scoped_detail.cve.fix_status, "fix_available");
    assert_ne!(development, production);
}

#[sqlx::test(migrations = "./migrations")]
async fn unlinking_an_environments_final_exact_link_retires_its_schedule(pool: PgPool) {
    let first = assessment_fixture(&pool).await;
    let second = assessment_fixture(&pool).await;
    let actor = admin_actor(first.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44014";
    let package_name = "libarchive";
    let first_environment = assign_environment(&pool, "unlink-first", &[&first]).await;
    let second_environment = assign_environment(&pool, "unlink-second", &[&second]).await;
    for fixture in [&first, &second] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "3.7.4", false)),
        )
        .await;
    }
    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: first_environment,
                },
                CveEnvironmentTriageAction::SchedulePatch {
                    environment_id: second_environment,
                },
            ],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    let detail = poam_service::detail(&pool, &actor, poam_id, &clock)
        .await
        .unwrap();
    let first_finding_id = detail
        .cve_findings
        .iter()
        .find(|finding| finding.system_id == first.system_id)
        .unwrap()
        .id;
    poam_service::unlink_cve_finding(
        &pool,
        &actor,
        poam_id,
        first_finding_id,
        detail.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    let fleet = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert!(
        fleet
            .environments
            .iter()
            .find(|environment| environment.environment_id == first_environment)
            .unwrap()
            .disposition
            .is_none()
    );
    assert!(matches!(
        fleet
            .environments
            .iter()
            .find(|environment| environment.environment_id == second_environment)
            .and_then(|environment| environment.disposition.as_ref()),
        Some(CveEnvironmentDisposition::Scheduled { .. })
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_fleet_cve_schedules_converge_on_one_poam(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44012";
    let package_name = "zlib";
    let environment_id = assign_environment(&pool, "fleet-concurrent", &[&fixture]).await;
    seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some((cve_id, package_name, "1.3.1", false)),
    )
    .await;
    let request = || FleetCveTriageRequest {
        canonical_package_name: package_name.into(),
        actions: vec![CveEnvironmentTriageAction::SchedulePatch { environment_id }],
        poam: Some(fleet_poam_request(actor.user_id, &clock)),
    };

    let (first, second) = tokio::join!(
        poam_service::triage_fleet_cve(&pool, &actor, cve_id, request(), &clock),
        poam_service::triage_fleet_cve(&pool, &actor, cve_id, request(), &clock),
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.poam_id, second.poam_id);
    assert!(first.poam_reused || second.poam_reused);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM poams WHERE id=ANY($1)")
            .bind(&[first.poam_id.unwrap(), second.poam_id.unwrap()])
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn fleet_cve_triage_reloads_authority_after_a_concurrent_fleet_change(pool: PgPool) {
    let first = assessment_fixture(&pool).await;
    let second = assessment_fixture(&pool).await;
    let actor = admin_actor(first.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44104";
    let package_name = "authority-race";
    let first_environment = assign_environment(&pool, "authority-race-first", &[&first]).await;
    assign_environment(&pool, "authority-race-second", &[&second]).await;
    for fixture in [&first, &second] {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "1.0.0", false)),
        )
        .await;
    }
    sqlx::query("UPDATE systems SET is_active=FALSE WHERE id=$1")
        .bind(second.system_id)
        .execute(&pool)
        .await
        .unwrap();

    let mut publisher = pool.begin().await.unwrap();
    sqlx::query("SELECT lock_poam_cve_key($1)")
        .bind(cve_id)
        .execute(&mut *publisher)
        .await
        .unwrap();
    sqlx::query("UPDATE systems SET is_active=TRUE WHERE id=$1")
        .bind(second.system_id)
        .execute(&mut *publisher)
        .await
        .unwrap();

    let triage_pool = pool.clone();
    let triage_actor = actor.clone();
    let triage_clock = clock.clone();
    let mut triage = tokio::spawn(async move {
        poam_service::triage_fleet_cve(
            &triage_pool,
            &triage_actor,
            cve_id,
            FleetCveTriageRequest {
                canonical_package_name: package_name.into(),
                actions: vec![CveEnvironmentTriageAction::LeaveOpen {
                    environment_id: first_environment,
                }],
                poam: None,
            },
            &triage_clock,
        )
        .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut triage)
            .await
            .is_err(),
        "triage must wait for a concurrent exact-CVE fleet publication"
    );
    publisher.commit().await.unwrap();
    let rejected = tokio::time::timeout(Duration::from_secs(5), triage)
        .await
        .expect("triage should resume after publication commits")
        .expect("triage task should not panic")
        .unwrap_err();
    assert!(matches!(
        rejected,
        PoamError::Conflict("cve_evidence_changed", _)
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_cve_verify_waits_for_every_policy_key_on_the_system(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44013";
    let package_name = "libxml2";
    let environment_id = assign_environment(&pool, "exact-verify-policy-lock", &[&fixture]).await;
    seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some((cve_id, package_name, "2.12.7", false)),
    )
    .await;
    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch { environment_id }],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        poam_id,
        TransitionPoamRequest {
            revision: 1,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();

    let policy_lineage_id = fixture.resolved.policies[0].policy_lineage_id;
    sqlx::query(
        "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(fixture.system_id)
    .bind(policy_lineage_id)
    .execute(&pool)
    .await
    .unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT lock_poam_finding_key($1,$2)")
        .bind(fixture.system_id)
        .bind(policy_lineage_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let verify_pool = pool.clone();
    let verify_actor = actor.clone();
    let verify_clock = clock.clone();
    let mut verification = tokio::spawn(async move {
        poam_service::verify(
            &verify_pool,
            &verify_actor,
            poam_id,
            awaiting.poam.revision,
            &verify_clock,
        )
        .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut verification)
            .await
            .is_err(),
        "exact verification must wait for an unrelated policy finding key"
    );
    blocker.rollback().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), verification)
        .await
        .expect("verification should resume after the policy key is released")
        .expect("verification task should not panic")
        .expect("verification should complete");
    assert_eq!(result["outcome"], "rejected");
}

#[sqlx::test(migrations = "./migrations")]
async fn fleet_cve_poam_verification_requires_all_ten_exact_subjects_to_pass(pool: PgPool) {
    let mut fixtures = Vec::new();
    for _ in 0..10 {
        fixtures.push(assessment_fixture(&pool).await);
    }
    let actor = admin_actor(fixtures[0].user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44011";
    let package_name = "curl";
    let fixture_refs = fixtures.iter().collect::<Vec<_>>();
    let environment_id = assign_environment(&pool, "fleet-ten-systems", &fixture_refs).await;
    for fixture in &fixtures {
        seal_exact_cve_scan(
            &pool,
            fixture,
            clock.now(),
            Some((cve_id, package_name, "8.10.0", false)),
        )
        .await;
    }
    let scheduled = poam_service::triage_fleet_cve(
        &pool,
        &actor,
        cve_id,
        FleetCveTriageRequest {
            canonical_package_name: package_name.into(),
            actions: vec![CveEnvironmentTriageAction::SchedulePatch { environment_id }],
            poam: Some(fleet_poam_request(actor.user_id, &clock)),
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = scheduled.poam_id.unwrap();
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        poam_id,
        TransitionPoamRequest {
            revision: 1,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    for fixture in fixtures.iter().take(9) {
        seal_exact_cve_scan(&pool, fixture, clock.now() + TimeDelta::minutes(1), None).await;
    }
    let partial = poam_service::verify(&pool, &actor, poam_id, awaiting.poam.revision, &clock)
        .await
        .unwrap();
    assert_eq!(partial["outcome"], "rejected");
    let results = partial["cve_items"].as_array().unwrap();
    assert_eq!(
        results
            .iter()
            .filter(|item| item["result"] == "pass")
            .count(),
        9
    );
    assert_eq!(
        results
            .iter()
            .filter(|item| item["result"] == "missing")
            .count(),
        1
    );

    seal_exact_cve_scan(
        &pool,
        &fixtures[9],
        clock.now() + TimeDelta::minutes(1),
        None,
    )
    .await;
    let all_pass = poam_service::verify(
        &pool,
        &actor,
        poam_id,
        partial["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(all_pass["outcome"], "accepted");
    assert!(
        all_pass["cve_items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["result"] == "pass")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn mixed_whitelist_scan_cannot_hide_affected_cve_during_verification(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let cve = "CVE-2026-32704";
    let package = "mixed-whitelist-package";
    let baseline = seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now() - TimeDelta::minutes(2),
        Some((cve, package, "1.0", false)),
    )
    .await
    .unwrap();
    let created = poam_service::create_cve(
        &pool,
        &actor,
        CreateCvePoamRequest {
            observation: baseline.clone(),
            title: "Remediate mixed-whitelist CVE".into(),
            plan: "Deploy a clean package".into(),
            owner: "Security".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap();
    let scan_id = begin_exact_cve_scan(&pool, &fixture, 2).await;
    let whitelisted_path = format!("/nix/store/aaa-{}-{package}.drv", Uuid::new_v4().simple());
    let affected_path = format!("/nix/store/zzz-{}-{package}.drv", Uuid::new_v4().simple());
    assert!(whitelisted_path < affected_path);
    for (path, version, whitelisted) in [
        (&whitelisted_path, "2.0", true),
        (&affected_path, "2.1", false),
    ] {
        let package_derivation_id: i32 = sqlx::query_scalar(
            r#"INSERT INTO derivations(
                 derivation_type,derivation_name,derivation_path,pname,version,status_id,attempt_count)
               VALUES('package',$1,$2,$3,$4,11,0) RETURNING id"#,
        )
        .bind(format!("{package}-{version}"))
        .bind(path)
        .bind(package)
        .bind(version)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO scan_packages(scan_id,derivation_id) VALUES($1,$2)")
            .bind(scan_id)
            .bind(package_derivation_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO cve_scan_vulnerability_observations(
                 scan_id,canonical_cve_id,canonical_package_name,observed_package_name,
                 observed_package_version,observed_derivation_path,is_whitelisted,
                 whitelist_reason,detection_method)
               VALUES($1,$2,$3,$3,$4,$5,$6,$7,'test-scanner')"#,
        )
        .bind(scan_id)
        .bind(cve)
        .bind(package)
        .bind(version)
        .bind(path)
        .bind(whitelisted)
        .bind(whitelisted.then_some("accepted by scanner policy"))
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE cve_scans SET status='completed',completed_at=$2,evidence_schema_version=1 WHERE id=$1",
    )
    .bind(scan_id)
    .bind(clock.now())
    .execute(&pool)
    .await
    .unwrap();

    let awaiting = poam_service::transition(
        &pool,
        &actor,
        created.poam.id,
        TransitionPoamRequest {
            revision: created.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let verified = poam_service::verify(
        &pool,
        &actor,
        created.poam.id,
        awaiting.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(verified["outcome"], "rejected");
    assert_eq!(verified["cve_items"][0]["result"], "fail");
    assert_eq!(verified["cve_items"][0]["scan_id"], scan_id.to_string());
    let (persisted_baseline, persisted_path): (Uuid, String) = sqlx::query_as(
        "SELECT baseline_scan_id,occurrence_derivation_path FROM poam_cve_verification_items WHERE scan_id=$1",
    )
    .bind(scan_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted_baseline, baseline.scan_id);
    assert_eq!(persisted_path, affected_path);

    let mut forged = pool.begin().await.unwrap();
    let attempt_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO poam_verification_attempts(
             poam_id,attempted_by,outcome,poam_revision,attempted_at)
           VALUES($1,$2,'rejected',$3,$4) RETURNING id"#,
    )
    .bind(created.poam.id)
    .bind(fixture.user_id)
    .bind(verified["revision"].as_i64().unwrap())
    .bind(clock.now())
    .fetch_one(&mut *forged)
    .await
    .unwrap();
    // Copy legitimate Current and baseline evidence into an unsealed attempt.
    // Only the claimed result and cited occurrence differ from the real item.
    let rejected = sqlx::query(
        r#"INSERT INTO poam_cve_verification_items(
             attempt_id,cve_finding_id,system_id,canonical_cve_id,canonical_package_name,
             baseline_scan_id,baseline_scan_derivation_id,baseline_scan_completed_at,
             baseline_generation_snapshot_id,baseline_generation,baseline_target_store_path,
             baseline_occurrence_derivation_path,baseline_observed_package_version,result,
             scan_id,scan_derivation_id,scan_completed_at,generation_snapshot_id,generation,
             target_store_path,occurrence_present,occurrence_derivation_path,
             observed_package_version,detail)
           SELECT $1,cve_finding_id,system_id,canonical_cve_id,canonical_package_name,
                  baseline_scan_id,baseline_scan_derivation_id,baseline_scan_completed_at,
                  baseline_generation_snapshot_id,baseline_generation,baseline_target_store_path,
                  baseline_occurrence_derivation_path,baseline_observed_package_version,
                  'whitelisted',scan_id,scan_derivation_id,scan_completed_at,
                  generation_snapshot_id,generation,target_store_path,true,$2,'2.0',
                  'forged whitelisted result'
           FROM poam_cve_verification_items
           WHERE attempt_id=(SELECT id FROM poam_verification_attempts
                             WHERE poam_id=$3 AND id<>$1 ORDER BY attempted_at DESC LIMIT 1)"#,
    )
    .bind(attempt_id)
    .bind(&whitelisted_path)
    .bind(created.poam.id)
    .execute(&mut *forged)
    .await
    .unwrap_err();
    assert_eq!(
        rejected
            .as_database_error()
            .and_then(|error| error.constraint()),
        Some("poam_cve_verification_unwhitelisted_precedence"),
        "unexpected forged-result rejection: {rejected:?}"
    );
    forged.rollback().await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_cve_poam_rejects_stale_identity_and_closes_only_on_clean_evidence(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    sync_user_role(&pool, fixture.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44001";
    let package_name = "openssl";
    let old_reference = seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now() - TimeDelta::minutes(2),
        Some((cve_id, package_name, "3.0.1", false)),
    )
    .await
    .unwrap();
    let current_reference = seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now() - TimeDelta::minutes(1),
        Some((cve_id, package_name, "3.0.2", false)),
    )
    .await
    .unwrap();
    seal_exact_cve_scan(&pool, &fixture, clock.now() - TimeDelta::seconds(90), None).await;

    let stale = poam_service::create_cve(
        &pool,
        &actor,
        CreateCvePoamRequest {
            observation: old_reference,
            title: "Stale CVE remediation".into(),
            plan: String::new(),
            owner: "Security".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            stale,
            PoamError::Precondition("stale_cve_observation", _, _)
        ),
        "unexpected stale-observation error: {stale:?}"
    );

    let relationships =
        poam_service::cve_relationships(&pool, &actor, fixture.system_id, None, None, &clock)
            .await
            .unwrap();
    assert_eq!(relationships.len(), 1);
    assert_eq!(relationships[0].observation, current_reference);
    assert!(relationships[0].active_poam.is_none());

    let created = poam_service::create_cve(
        &pool,
        &actor,
        CreateCvePoamRequest {
            observation: current_reference.clone(),
            title: "Remediate exact OpenSSL CVE".into(),
            plan: "Deploy a fixed package derivation".into(),
            owner: "Security".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(created.findings.is_empty());
    assert_eq!(created.poam.finding_count, 0);
    assert_eq!(created.poam.cve_finding_count, 1);
    assert_eq!(created.cve_findings.len(), 1);
    assert_eq!(created.cve_findings[0].resolution_state, "fail");
    assert!(
        sqlx::query(
            "UPDATE poam_cve_finding_links SET baseline_observed_package_version='forged' WHERE poam_id=$1",
        )
        .bind(created.poam.id)
        .execute(&pool)
        .await
        .is_err(),
        "link-time baseline evidence must be immutable"
    );

    let duplicate = poam_service::create_cve(
        &pool,
        &actor,
        CreateCvePoamRequest {
            observation: current_reference.clone(),
            title: "Duplicate CVE remediation".into(),
            plan: String::new(),
            owner: "Security".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert!(matches!(duplicate, PoamError::Conflict(_, _)));

    let awaiting = poam_service::transition(
        &pool,
        &actor,
        created.poam.id,
        TransitionPoamRequest {
            revision: created.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let failed_close = poam_service::close(
        &pool,
        &actor,
        created.poam.id,
        awaiting.poam.revision,
        &clock,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        failed_close,
        PoamError::Precondition("closure_not_ready", _, _)
    ));
    let after_failure = poam_service::detail(&pool, &actor, created.poam.id, &clock)
        .await
        .unwrap();
    assert_eq!(after_failure.verification_attempts[0].outcome, "rejected");
    assert_eq!(
        after_failure.verification_attempts[0].cve_items[0].result,
        "missing"
    );
    assert_eq!(
        after_failure.verification_attempts[0].cve_items[0].baseline_observed_package_version,
        "3.0.2"
    );
    assert!(
        sqlx::query("DELETE FROM cve_scans WHERE id=$1")
            .bind(after_failure.verification_attempts[0].cve_items[0].baseline_scan_id)
            .execute(&pool)
            .await
            .is_err(),
        "verification evidence must retain its cited scan"
    );
    let forged_attempt: Uuid = sqlx::query_scalar(
        r#"INSERT INTO poam_verification_attempts(
             poam_id,attempted_by,outcome,poam_revision,attempted_at)
           VALUES($1,$2,'rejected',$3,$4) RETURNING id"#,
    )
    .bind(created.poam.id)
    .bind(fixture.user_id)
    .bind(after_failure.poam.revision)
    .bind(clock.now())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        sqlx::query(
            r#"INSERT INTO poam_cve_verification_items(
                 attempt_id,cve_finding_id,system_id,canonical_cve_id,
                 canonical_package_name,baseline_scan_id,
                 baseline_scan_derivation_id,baseline_scan_completed_at,
                 baseline_generation_snapshot_id,baseline_generation,
                 baseline_target_store_path,baseline_occurrence_derivation_path,
                 baseline_observed_package_version,result,scan_id,
                 scan_derivation_id,scan_completed_at,generation_snapshot_id,
                 generation,target_store_path,occurrence_present,
                 occurrence_derivation_path,observed_package_version,detail)
               SELECT $1,finding.id,finding.system_id,finding.canonical_cve_id,
                       finding.canonical_package_name,link.baseline_scan_id,
                       link.baseline_scan_derivation_id,
                       link.baseline_scan_completed_at,
                       link.baseline_generation_snapshot_id,
                       link.baseline_generation,link.baseline_target_store_path,
                       link.baseline_occurrence_derivation_path,
                       link.baseline_observed_package_version,'pass',$2,$3,$4,
                       link.baseline_generation_snapshot_id,
                       link.baseline_generation,$5,false,NULL,NULL,'forged pass'
               FROM poam_cve_findings finding
               JOIN poam_cve_finding_links link ON link.cve_finding_id=finding.id
               WHERE link.poam_id=$6 AND link.retired_at IS NULL"#,
        )
        .bind(forged_attempt)
        .bind(current_reference.scan_id)
        .bind(fixture.derivation_id)
        .bind(clock.now() - TimeDelta::minutes(1))
        .bind(&fixture.store_path)
        .bind(created.poam.id)
        .execute(&pool)
        .await
        .is_err(),
        "present exact evidence must reject a fabricated PASS"
    );

    seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some((cve_id, package_name, "3.0.3", true)),
    )
    .await;
    let whitelisted = poam_service::verify(
        &pool,
        &actor,
        created.poam.id,
        after_failure.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(whitelisted["outcome"], "rejected");
    assert_eq!(whitelisted["cve_items"][0]["result"], "whitelisted");

    seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now() + TimeDelta::minutes(1),
        Some((cve_id, package_name, "3.0.4", false)),
    )
    .await;
    sqlx::query(
        r#"INSERT INTO system_cve_justifications(system_id,cve_id,category,reason,updated_by)
           VALUES($1,$2,'accepted_risk','Accepted independently',$3)"#,
    )
    .bind(fixture.system_id)
    .bind(cve_id)
    .bind(fixture.user_id)
    .execute(&pool)
    .await
    .unwrap();
    let justified = poam_service::verify(
        &pool,
        &actor,
        created.poam.id,
        whitelisted["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(justified["outcome"], "rejected");
    assert_eq!(justified["cve_items"][0]["result"], "justified");

    seal_exact_cve_scan(&pool, &fixture, clock.now() + TimeDelta::minutes(2), None).await;
    let clean = poam_service::verify(
        &pool,
        &actor,
        created.poam.id,
        justified["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(clean["outcome"], "accepted");
    assert_eq!(clean["cve_items"][0]["result"], "pass");
    assert_eq!(
        clean["cve_items"][0]["baseline_generation_snapshot_id"],
        clean["cve_items"][0]["generation_snapshot_id"]
    );
    let clean_scan_id =
        Uuid::parse_str(clean["cve_items"][0]["scan_id"].as_str().unwrap()).unwrap();
    assert!(
        sqlx::query("DELETE FROM cve_scans WHERE id=$1")
            .bind(clean_scan_id)
            .execute(&pool)
            .await
            .is_err(),
        "current verification evidence must retain its cited scan"
    );
    let closed = poam_service::close(
        &pool,
        &actor,
        created.poam.id,
        clean["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(closed.poam.status, "completed");
    assert_eq!(closed.poam.cve_finding_count, 1);
    assert!(!closed.cve_findings[0].link_active);

    let reopened =
        poam_service::reopen(&pool, &actor, created.poam.id, closed.poam.revision, &clock)
            .await
            .unwrap();
    assert_eq!(reopened.poam.status, "in_progress");
    assert_eq!(reopened.poam.cve_finding_count, 1);
    assert_eq!(
        reopened.cve_findings[0].baseline_scan_id,
        current_reference.scan_id
    );
    let retained_baseline = created.cve_findings[0].baseline_generation_snapshot_id;
    assert!(retained_baseline.is_some());
    assert_eq!(
        reopened.cve_findings[0].baseline_generation_snapshot_id,
        retained_baseline
    );
    let copied: Vec<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT baseline_scan_id,baseline_generation_snapshot_id FROM poam_cve_finding_links WHERE poam_id=$1 ORDER BY linked_at,id",
    )
    .bind(created.poam.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        copied,
        vec![(current_reference.scan_id, retained_baseline); 2]
    );
    assert_eq!(
        reopened
            .cve_findings
            .iter()
            .filter(|finding| finding.link_active)
            .count(),
        1
    );

    let same_path = fixture.store_path.clone();
    deploy_store_path(&pool, &fixture, &same_path).await;
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        created.poam.id,
        TransitionPoamRequest {
            revision: reopened.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: None,
        },
        &clock,
    )
    .await
    .unwrap();
    let later_clean = poam_service::verify(
        &pool,
        &actor,
        created.poam.id,
        awaiting.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(later_clean["outcome"], "accepted");
    assert_eq!(later_clean["cve_items"][0]["result"], "pass");
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_cve_link_and_unlink_emit_distinct_activity(pool: PgPool) {
    let first = assessment_fixture(&pool).await;
    let second = assessment_fixture(&pool).await;
    sync_user_role(&pool, first.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let actor = admin_actor(first.user_id);
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44002";
    let package_name = "libxml2";
    let first_observation = seal_exact_cve_scan(
        &pool,
        &first,
        clock.now(),
        Some((cve_id, package_name, "2.12.1", false)),
    )
    .await
    .unwrap();
    let second_observation = seal_exact_cve_scan(
        &pool,
        &second,
        clock.now(),
        Some((cve_id, package_name, "2.12.2", false)),
    )
    .await
    .unwrap();
    let created = poam_service::create_cve(
        &pool,
        &actor,
        CreateCvePoamRequest {
            observation: first_observation,
            title: "Remediate exact libxml2 CVE".into(),
            plan: String::new(),
            owner: "Security".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap();
    let linked = poam_service::link_cve_finding(
        &pool,
        &actor,
        created.poam.id,
        AddCveFindingRequest {
            revision: created.poam.revision,
            observation: second_observation,
        },
        &clock,
    )
    .await
    .unwrap();
    let second_finding_id = linked
        .cve_findings
        .iter()
        .find(|finding| finding.system_id == second.system_id)
        .unwrap()
        .id;
    poam_service::unlink_cve_finding(
        &pool,
        &actor,
        created.poam.id,
        second_finding_id,
        linked.poam.revision,
        &clock,
    )
    .await
    .unwrap();

    let activity: Vec<(String, i64)> = sqlx::query_as(
        r#"SELECT kind,COUNT(*) FROM poam_activity
           WHERE poam_id=$1 AND kind IN ('cve_finding_linked','cve_finding_unlinked')
           GROUP BY kind ORDER BY kind"#,
    )
    .bind(created.poam.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        activity,
        vec![
            ("cve_finding_linked".into(), 1),
            ("cve_finding_unlinked".into(), 1),
        ]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_cve_create_rechecks_scope_after_concurrent_environment_move(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let dev: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='dev'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let prod: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='prod'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(fixture.system_id)
        .bind(dev)
        .execute(&pool)
        .await
        .unwrap();
    sync_user_role(&pool, fixture.user_id, AuthRole::Operator)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(fixture.user_id)
        .bind(dev)
        .execute(&pool)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let observation = seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some(("CVE-2026-44011", "openssl", "3.0.11", false)),
    )
    .await
    .unwrap();
    let actor = PoamActor {
        user_id: fixture.user_id,
        identifier: "moving-operator".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: vec![dev],
        request_origin: Some("test".into()),
    };

    let mut move_tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(fixture.system_id)
        .bind(prod)
        .execute(&mut *move_tx)
        .await
        .unwrap();
    let create_pool = pool.clone();
    let create_clock = clock.clone();
    let create = tokio::spawn(async move {
        poam_service::create_cve(
            &create_pool,
            &actor,
            CreateCvePoamRequest {
                observation,
                title: "Must not cross environment move".into(),
                plan: "No mutation after scope loss".into(),
                owner: "Security".into(),
                assignee: None,
                target_date: None,
                risk: PoamRisk::High,
                default_milestones: false,
                assignment_version_ids: vec![],
            },
            &create_clock,
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !create.is_finished(),
        "create must wait for the system move"
    );
    move_tx.commit().await.unwrap();
    assert!(matches!(create.await.unwrap(), Err(PoamError::NotFound)));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM poams WHERE title=$1")
        .bind("Must not cross environment move")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_cve_link_rechecks_scope_after_concurrent_environment_move(pool: PgPool) {
    let first = assessment_fixture(&pool).await;
    let second = assessment_fixture(&pool).await;
    let dev: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='dev'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let prod: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='prod'")
        .fetch_one(&pool)
        .await
        .unwrap();
    for system_id in [first.system_id, second.system_id] {
        sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
            .bind(system_id)
            .bind(dev)
            .execute(&pool)
            .await
            .unwrap();
    }
    sync_user_role(&pool, first.user_id, AuthRole::Operator)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(first.user_id)
        .bind(dev)
        .execute(&pool)
        .await
        .unwrap();
    let actor = PoamActor {
        user_id: first.user_id,
        identifier: "link-moving-operator".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: vec![dev],
        request_origin: Some("test".into()),
    };
    let clock = FixedClock(Utc::now());
    let first_observation = seal_exact_cve_scan(
        &pool,
        &first,
        clock.now(),
        Some(("CVE-2026-44012", "openssl", "3.0.12", false)),
    )
    .await
    .unwrap();
    let second_observation = seal_exact_cve_scan(
        &pool,
        &second,
        clock.now(),
        Some(("CVE-2026-44012", "openssl", "3.0.12", false)),
    )
    .await
    .unwrap();
    let created = poam_service::create_cve(
        &pool,
        &actor,
        CreateCvePoamRequest {
            observation: first_observation,
            title: "One authorized exact CVE".into(),
            plan: String::new(),
            owner: "Security".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap();

    let mut move_tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(second.system_id)
        .bind(prod)
        .execute(&mut *move_tx)
        .await
        .unwrap();
    let link_pool = pool.clone();
    let link_clock = clock.clone();
    let poam_id = created.poam.id;
    let revision = created.poam.revision;
    let link = tokio::spawn(async move {
        poam_service::link_cve_finding(
            &link_pool,
            &actor,
            poam_id,
            AddCveFindingRequest {
                revision,
                observation: second_observation,
            },
            &link_clock,
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!link.is_finished(), "link must wait for the system move");
    move_tx.commit().await.unwrap();
    assert!(matches!(link.await.unwrap(), Err(PoamError::NotFound)));
    let state: (i64, i64) = sqlx::query_as(
        r#"SELECT poam.revision,COUNT(link.id)
           FROM poams poam
           LEFT JOIN poam_cve_finding_links link
             ON link.poam_id=poam.id AND link.retired_at IS NULL
           WHERE poam.id=$1 GROUP BY poam.id"#,
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, (revision, 1));
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_cve_justification_rechecks_scope_after_concurrent_environment_move(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let dev: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='dev'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let prod: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='prod'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(fixture.system_id)
        .bind(dev)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(fixture.user_id)
        .bind(dev)
        .execute(&pool)
        .await
        .unwrap();
    let token = session(&pool, fixture.user_id, AuthRole::Operator).await;
    let clock = FixedClock(Utc::now());
    seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some(("CVE-2026-44013", "openssl", "3.0.13", false)),
    )
    .await
    .unwrap();
    let base = poam_http_server(pool.clone()).await;

    let mut move_tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(fixture.system_id)
        .bind(prod)
        .execute(&mut *move_tx)
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let request = http_request(
        &client,
        reqwest::Method::PUT,
        format!(
            "{base}/api/v1/systems/{}/cves/CVE-2026-44013/justification",
            fixture.system_id
        ),
        &token,
        Some("move-race"),
    )
    .json(&serde_json::json!({
        "category": "accepted_risk",
        "reason": "Must not survive an environment move"
    }));
    let response = tokio::spawn(async move { request.send().await.unwrap() });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !response.is_finished(),
        "justification must wait for the system move"
    );
    move_tx.commit().await.unwrap();
    assert_eq!(
        response.await.unwrap().status(),
        reqwest::StatusCode::NOT_FOUND
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM system_cve_justifications WHERE system_id=$1 AND cve_id=$2",
    )
    .bind(fixture.system_id)
    .bind("CVE-2026-44013")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn system_update_requires_matching_csrf_before_scope_changes(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let source_environment = assign_environment(&pool, "csrf-update-source", &[&fixture]).await;
    let destination_environment: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("csrf-update-dst-{}", Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .unwrap();
    seal_exact_cve_scan(
        &pool,
        &fixture,
        Utc::now(),
        Some(("CVE-2026-44099", "csrf-package", "1.0", false)),
    )
    .await
    .unwrap();
    let token = session(&pool, fixture.user_id, AuthRole::Admin).await;
    let base = poam_http_server(pool.clone()).await;
    let url = format!("{base}/api/v1/systems/{}", fixture.system_id);
    let original_hostname: String = sqlx::query_scalar("SELECT hostname FROM systems WHERE id=$1")
        .bind(fixture.system_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let updated_hostname = format!("csrf-updated-{}", Uuid::new_v4().simple());
    let flake_name: String = sqlx::query_scalar(
        "SELECT flake.name FROM systems system JOIN flakes flake ON flake.id=system.flake_id WHERE system.id=$1",
    )
    .bind(fixture.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let destination_name: String = sqlx::query_scalar("SELECT name FROM environments WHERE id=$1")
        .bind(destination_environment)
        .fetch_one(&pool)
        .await
        .unwrap();
    let payload = serde_json::json!({
        "hostname": updated_hostname,
        "system_configuration_name": original_hostname,
        "environment": destination_name,
        "flake_name": flake_name,
        "deployment_policy": "manual"
    });
    let client = reqwest::Client::new();

    let missing = http_request(&client, reqwest::Method::PATCH, url.clone(), &token, None)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        missing.json::<serde_json::Value>().await.unwrap()["error"],
        "csrf_validation_failed"
    );
    let state_after_missing: (String, Option<Uuid>) =
        sqlx::query_as("SELECT hostname,environment_id FROM systems WHERE id=$1")
            .bind(fixture.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        state_after_missing,
        (original_hostname.clone(), Some(source_environment))
    );

    let mismatch = client
        .patch(&url)
        .header(
            "cookie",
            format!("{SESSION_COOKIE_NAME}={token}; {CSRF_COOKIE_NAME}=csrf-cookie-token"),
        )
        .header(CSRF_HEADER_NAME.as_str(), "different-csrf-header-token")
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(mismatch.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        mismatch.json::<serde_json::Value>().await.unwrap()["error"],
        "csrf_validation_failed"
    );
    let rejected_state: (String, Option<Uuid>) =
        sqlx::query_as("SELECT hostname,environment_id FROM systems WHERE id=$1")
            .bind(fixture.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        rejected_state,
        (original_hostname, Some(source_environment))
    );

    let accepted = http_request(
        &client,
        reqwest::Method::PATCH,
        url,
        &token,
        Some("matching-system-update-csrf"),
    )
    .json(&payload)
    .send()
    .await
    .unwrap();
    assert_eq!(accepted.status(), reqwest::StatusCode::OK);
    let accepted_body: serde_json::Value = accepted.json().await.unwrap();
    assert_eq!(accepted_body["hostname"], updated_hostname);
    assert_eq!(accepted_body["environment"], destination_name);
    let committed_state: (String, Option<Uuid>) =
        sqlx::query_as("SELECT hostname,environment_id FROM systems WHERE id=$1")
            .bind(fixture.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        committed_state,
        (updated_hostname, Some(destination_environment))
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn system_move_rechecks_role_after_environment_lock_wait(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let source_environment = assign_environment(&pool, "system-move-source", &[&fixture]).await;
    let destination_environment: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("move-dst-{}", Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2),($1,$3)",
    )
    .bind(fixture.user_id)
    .bind(source_environment)
    .bind(destination_environment)
    .execute(&pool)
    .await
    .unwrap();
    let token = session(&pool, fixture.user_id, AuthRole::Operator).await;
    let base = poam_http_server(pool.clone()).await;
    let update_url = format!("{base}/api/v1/systems/{}", fixture.system_id);
    let hostname: String = sqlx::query_scalar("SELECT hostname FROM systems WHERE id=$1")
        .bind(fixture.system_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let flake_name: String = sqlx::query_scalar(
        "SELECT flake.name FROM systems system JOIN flakes flake ON flake.id=system.flake_id WHERE system.id=$1",
    )
    .bind(fixture.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let destination_name: String = sqlx::query_scalar("SELECT name FROM environments WHERE id=$1")
        .bind(destination_environment)
        .fetch_one(&pool)
        .await
        .unwrap();
    let payload = serde_json::json!({
        "hostname": hostname,
        "system_configuration_name": hostname,
        "environment": destination_name,
        "flake_name": flake_name,
        "deployment_policy": "manual"
    });

    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM environments WHERE id=$1 FOR UPDATE")
        .bind(destination_environment)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let mut request = tokio::spawn({
        let client = client.clone();
        let token = token.clone();
        let payload = payload.clone();
        let update_url = update_url.clone();
        async move {
            http_request(
                &client,
                reqwest::Method::PATCH,
                update_url,
                &token,
                Some("system-move-csrf"),
            )
            .json(&payload)
            .send()
            .await
            .unwrap()
        }
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut request)
            .await
            .is_err(),
        "system move must wait for the destination environment lock"
    );
    sync_user_role(&pool, fixture.user_id, AuthRole::Viewer)
        .await
        .unwrap();
    blocker.commit().await.unwrap();
    let denied = tokio::time::timeout(Duration::from_secs(5), request)
        .await
        .expect("system move did not resume")
        .unwrap();
    assert_eq!(denied.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT environment_id FROM systems WHERE id=$1")
            .bind(fixture.system_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        Some(source_environment)
    );

    sync_user_role(&pool, fixture.user_id, AuthRole::Operator)
        .await
        .unwrap();
    let moved = http_request(
        &client,
        reqwest::Method::PATCH,
        update_url,
        &token,
        Some("system-move-csrf"),
    )
    .json(&payload)
    .send()
    .await
    .unwrap();
    assert_eq!(moved.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = moved.json().await.unwrap();
    assert_eq!(body["environment"], destination_name);
    assert_eq!(
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT environment_id FROM systems WHERE id=$1")
            .bind(fixture.system_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        Some(destination_environment)
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_system_move_and_fleet_triage_finish_with_current_destination_scope(
    pool: PgPool,
) {
    let fixture = assessment_fixture(&pool).await;
    let source_environment = assign_environment(&pool, "move-triage-source", &[&fixture]).await;
    let destination_environment: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("triage-dst-{}", Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .unwrap();
    let destination_name: String = sqlx::query_scalar("SELECT name FROM environments WHERE id=$1")
        .bind(destination_environment)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2),($1,$3)",
    )
    .bind(fixture.user_id)
    .bind(source_environment)
    .bind(destination_environment)
    .execute(&pool)
    .await
    .unwrap();
    let token = session(&pool, fixture.user_id, AuthRole::Admin).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44016";
    let package_name = "move-triage-package";
    let observation = seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some((cve_id, package_name, "1.0.0", false)),
    )
    .await
    .unwrap();
    poam_service::create_cve(
        &pool,
        &actor,
        CreateCvePoamRequest {
            observation,
            title: "Preserve an exact key during system movement".into(),
            plan: "Exercise the complete lock hierarchy".into(),
            owner: "Security".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap();
    let hostname: String = sqlx::query_scalar("SELECT hostname FROM systems WHERE id=$1")
        .bind(fixture.system_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let flake_name: String = sqlx::query_scalar(
        "SELECT flake.name FROM systems system JOIN flakes flake ON flake.id=system.flake_id WHERE system.id=$1",
    )
    .bind(fixture.system_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let base = poam_http_server(pool.clone()).await;
    let client = reqwest::Client::new();
    let move_request = http_request(
        &client,
        reqwest::Method::PATCH,
        format!("{base}/api/v1/systems/{}", fixture.system_id),
        &token,
        Some("system-move-triage-csrf"),
    )
    .json(&serde_json::json!({
        "hostname": hostname,
        "system_configuration_name": hostname,
        "environment": destination_name,
        "flake_name": flake_name,
        "deployment_policy": "manual"
    }));
    let triage_pool = pool.clone();
    let triage_actor = actor.clone();
    let triage_clock = clock.clone();
    let operations = async move {
        tokio::join!(
            async move { move_request.send().await.unwrap() },
            poam_service::triage_fleet_cve(
                &triage_pool,
                &triage_actor,
                cve_id,
                FleetCveTriageRequest {
                    canonical_package_name: package_name.into(),
                    actions: vec![CveEnvironmentTriageAction::LeaveOpen {
                        environment_id: source_environment,
                    }],
                    poam: None,
                },
                &triage_clock,
            )
        )
    };
    let (moved, triaged) = tokio::time::timeout(Duration::from_secs(5), operations)
        .await
        .expect("system move and fleet triage must not deadlock");
    assert_eq!(moved.status(), reqwest::StatusCode::OK);
    assert!(
        triaged.is_ok()
            || matches!(
                triaged,
                Err(PoamError::Conflict("cve_evidence_changed", _))
                    | Err(PoamError::ConflictDetails("poam_final_subject", _, _))
            ),
        "unexpected fleet triage result: {triaged:?}"
    );
    let detail = poam_service::fleet_cve_detail(&pool, &actor, cve_id, package_name)
        .await
        .unwrap();
    assert_eq!(detail.environments.len(), 1);
    assert_eq!(
        detail.environments[0].environment_id,
        destination_environment
    );
    assert!(detail.environments[0].disposition.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_cve_justification_rechecks_active_user_after_waiting_for_system_lock(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let environment_id: Uuid = sqlx::query_scalar("SELECT id FROM environments WHERE name='dev'")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(fixture.system_id)
        .bind(environment_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(fixture.user_id)
    .bind(environment_id)
    .execute(&pool)
    .await
    .unwrap();
    let token = session(&pool, fixture.user_id, AuthRole::Operator).await;
    let clock = FixedClock(Utc::now());
    seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some(("CVE-2026-44014", "openssl", "3.0.14", false)),
    )
    .await
    .unwrap();
    let base = poam_http_server(pool.clone()).await;

    let mut system_lock = pool.begin().await.unwrap();
    sqlx::query("UPDATE systems SET hostname=hostname WHERE id=$1")
        .bind(fixture.system_id)
        .execute(&mut *system_lock)
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let request = http_request(
        &client,
        reqwest::Method::PUT,
        format!(
            "{base}/api/v1/systems/{}/cves/CVE-2026-44014/justification",
            fixture.system_id
        ),
        &token,
        Some("active-user-race"),
    )
    .json(&serde_json::json!({
        "category": "accepted_risk",
        "reason": "Must not survive user deactivation"
    }));
    let response = tokio::spawn(async move { request.send().await.unwrap() });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !response.is_finished(),
        "justification must wait for the system row lock"
    );
    sqlx::query("UPDATE users SET is_active=false WHERE id=$1")
        .bind(fixture.user_id)
        .execute(&pool)
        .await
        .unwrap();
    system_lock.commit().await.unwrap();
    assert_eq!(
        response.await.unwrap().status(),
        reqwest::StatusCode::FORBIDDEN
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM system_cve_justifications WHERE system_id=$1 AND cve_id=$2",
    )
    .bind(fixture.system_id)
    .bind("CVE-2026-44014")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn metadata_trigger_fails_fast_without_partial_write_then_justification_converges(
    pool: PgPool,
) {
    let fixture = assessment_fixture(&pool).await;
    let environment_id = assign_environment(&pool, "metadata-justification", &[&fixture]).await;
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(fixture.user_id)
        .bind(environment_id)
        .execute(&pool)
        .await
        .unwrap();
    let token = session(&pool, fixture.user_id, AuthRole::Admin).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2026-44017";
    let observation = seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some((cve_id, "metadata-package", "1.0.0", false)),
    )
    .await
    .unwrap();
    poam_service::create_cve(
        &pool,
        &actor,
        CreateCvePoamRequest {
            observation,
            title: "Keep metadata and justification serialization ordered".into(),
            plan: "Exercise trigger-only retry semantics".into(),
            owner: "Security".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap();
    let original_hostname: String = sqlx::query_scalar("SELECT hostname FROM systems WHERE id=$1")
        .bind(fixture.system_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let attempted_hostname = format!("{original_hostname}-blocked");
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT lock_poam_cve_key($1)")
        .bind(cve_id)
        .execute(&mut *blocker)
        .await
        .unwrap();

    let metadata_error = tokio::time::timeout(
        Duration::from_secs(1),
        sqlx::query("UPDATE systems SET hostname=$2 WHERE id=$1")
            .bind(fixture.system_id)
            .bind(&attempted_hostname)
            .execute(&pool),
    )
    .await
    .expect("metadata trigger must fail instead of waiting on an earlier CVE lock")
    .unwrap_err();
    assert_eq!(
        metadata_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("40001")
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT hostname FROM systems WHERE id=$1")
            .bind(fixture.system_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        original_hostname
    );

    let base = poam_http_server(pool.clone()).await;
    let client = reqwest::Client::new();
    let request = http_request(
        &client,
        reqwest::Method::PUT,
        format!(
            "{base}/api/v1/systems/{}/cves/{cve_id}/justification",
            fixture.system_id
        ),
        &token,
        Some("metadata-justification-csrf"),
    )
    .json(&serde_json::json!({
        "category": "accepted_risk",
        "reason": "The justification waits for the earlier canonical CVE lock"
    }));
    let mut justification = tokio::spawn(async move { request.send().await.unwrap() });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut justification)
            .await
            .is_err(),
        "justification must wait for the canonical CVE writer"
    );
    blocker.rollback().await.unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), justification)
        .await
        .expect("justification must resume without deadlock")
        .unwrap();
    let response_status = response.status();
    let response_body = response.text().await.unwrap();
    assert_eq!(
        response_status,
        reqwest::StatusCode::OK,
        "unexpected justification response: {response_body}"
    );

    sqlx::query("UPDATE systems SET hostname=$2 WHERE id=$1")
        .bind(fixture.system_id)
        .bind(&attempted_hostname)
        .execute(&pool)
        .await
        .expect("retry after the canonical lock is released must succeed");
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT hostname FROM systems WHERE id=$1")
            .bind(fixture.system_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        attempted_hostname
    );
    let justification_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM system_cve_justifications WHERE system_id=$1 AND cve_id=$2",
    )
    .bind(fixture.system_id)
    .bind(cve_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(justification_count, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_cve_justification_mutations_require_matching_csrf_before_writes(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let environment_id = assign_environment(&pool, "justification-csrf", &[&fixture]).await;
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(fixture.user_id)
        .bind(environment_id)
        .execute(&pool)
        .await
        .unwrap();
    let token = session(&pool, fixture.user_id, AuthRole::Admin).await;
    let cve_id = "CVE-2026-44015";
    seal_exact_cve_scan(
        &pool,
        &fixture,
        Utc::now(),
        Some((cve_id, "csrf-package", "1.0.0", false)),
    )
    .await
    .unwrap();
    let base = poam_http_server(pool.clone()).await;
    let client = reqwest::Client::new();
    let system_url = format!(
        "{base}/api/v1/systems/{}/cves/{cve_id}/justification",
        fixture.system_id
    );
    let system_body = serde_json::json!({
        "category": "accepted_risk",
        "reason": "Matching CSRF is required before this system write"
    });

    for request in [
        http_request(
            &client,
            reqwest::Method::PUT,
            system_url.clone(),
            &token,
            None,
        ),
        client
            .put(&system_url)
            .header(
                "cookie",
                format!("{SESSION_COOKIE_NAME}={token}; {CSRF_COOKIE_NAME}=expected-system"),
            )
            .header(CSRF_HEADER_NAME.as_str(), "mismatched-system"),
    ] {
        let response = request.json(&system_body).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap()["error"],
            "csrf_validation_failed"
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM system_cve_justifications WHERE system_id=$1 AND cve_id=$2",
        )
        .bind(fixture.system_id)
        .bind(cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
    }
    let saved = http_request(
        &client,
        reqwest::Method::PUT,
        system_url,
        &token,
        Some("matching-system"),
    )
    .json(&system_body)
    .send()
    .await
    .unwrap();
    assert_eq!(saved.status(), reqwest::StatusCode::OK);

    let fleet_url = format!("{base}/api/v1/cves/{cve_id}/justification");
    let fleet_body = serde_json::json!({
        "system_id": null,
        "category": "accepted_risk",
        "reason": "Matching CSRF is required before this fleet write"
    });
    for request in [
        http_request(
            &client,
            reqwest::Method::POST,
            fleet_url.clone(),
            &token,
            None,
        ),
        client
            .post(&fleet_url)
            .header(
                "cookie",
                format!("{SESSION_COOKIE_NAME}={token}; {CSRF_COOKIE_NAME}=expected-fleet"),
            )
            .header(CSRF_HEADER_NAME.as_str(), "mismatched-fleet"),
    ] {
        let response = request.json(&fleet_body).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap()["error"],
            "csrf_validation_failed"
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM system_cve_justifications WHERE system_id IS NULL AND cve_id=$1",
        )
        .bind(cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
    }
    let fleet_saved = http_request(
        &client,
        reqwest::Method::POST,
        fleet_url.clone(),
        &token,
        Some("matching-fleet"),
    )
    .json(&fleet_body)
    .send()
    .await
    .unwrap();
    assert_eq!(fleet_saved.status(), reqwest::StatusCode::CREATED);

    for request in [
        http_request(
            &client,
            reqwest::Method::DELETE,
            fleet_url.clone(),
            &token,
            None,
        ),
        client
            .delete(&fleet_url)
            .header(
                "cookie",
                format!("{SESSION_COOKIE_NAME}={token}; {CSRF_COOKIE_NAME}=expected-delete"),
            )
            .header(CSRF_HEADER_NAME.as_str(), "mismatched-delete"),
    ] {
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap()["error"],
            "csrf_validation_failed"
        );
        let remains: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM system_cve_justifications WHERE system_id IS NULL AND cve_id=$1)",
        )
        .bind(cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(remains);
    }
    let deleted = http_request(
        &client,
        reqwest::Method::DELETE,
        fleet_url,
        &token,
        Some("matching-delete"),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(deleted.status(), reqwest::StatusCode::NO_CONTENT);
    let remains: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM system_cve_justifications WHERE system_id IS NULL AND cve_id=$1)",
    )
    .bind(cve_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!remains);
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_cve_row_hydration_reaches_past_legacy_relationship_ceiling(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc::now());
    let scan_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO cve_scans(
             id,derivation_id,scanner_name,status,total_packages,total_vulnerabilities)
           VALUES($1,$2,'batch-test','in_progress',101,101)"#,
    )
    .bind(scan_id)
    .bind(fixture.derivation_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO cves(id)
           SELECT 'CVE-2099-'||LPAD(ordinal::text,4,'0')
           FROM generate_series(1,101) ordinal"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO cve_scan_vulnerability_observations(
             scan_id,canonical_cve_id,canonical_package_name,
             observed_package_name,observed_package_version,
             observed_derivation_path,is_whitelisted,detection_method)
           SELECT $1,'CVE-2099-'||LPAD(ordinal::text,4,'0'),
                  'package-'||LPAD(ordinal::text,3,'0'),
                  'package-'||LPAD(ordinal::text,3,'0'),'1.0.0',
                  '/nix/store/package-'||LPAD(ordinal::text,3,'0')||'.drv',
                  false,'batch-test'
           FROM generate_series(1,101) ordinal"#,
    )
    .bind(scan_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE cve_scans SET status='completed',completed_at=$2,evidence_schema_version=1 WHERE id=$1",
    )
    .bind(scan_id)
    .bind(clock.now())
    .execute(&pool)
    .await
    .unwrap();

    let legacy =
        poam_service::cve_relationships(&pool, &actor, fixture.system_id, None, None, &clock)
            .await
            .unwrap();
    assert_eq!(legacy.len(), 100);
    assert!(
        legacy
            .iter()
            .all(|relationship| relationship.observation.canonical_cve_id != "CVE-2099-0101")
    );

    let row_keys = [CveRelationshipRowKey {
        canonical_cve_id: "CVE-2099-0101".into(),
        canonical_package_name: "package-101".into(),
        scan_id,
        occurrence_derivation_path: "/nix/store/package-101.drv".into(),
    }];
    let requested = poam_service::cve_relationships_for_rows(
        &pool,
        &actor,
        fixture.system_id,
        &row_keys,
        None,
        None,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(requested.len(), 1);
    assert_eq!(requested[0].observation.canonical_cve_id, "CVE-2099-0101");
    assert_eq!(requested[0].observed_package_name, "package-101");

    let environment_id: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("row-hydration-{scan_id}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(fixture.system_id)
        .bind(environment_id)
        .execute(&pool)
        .await
        .unwrap();
    sync_user_role(&pool, actor.user_id, AuthRole::Viewer)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(actor.user_id)
        .bind(environment_id)
        .execute(&pool)
        .await
        .unwrap();
    let current_member = poam_service::cve_relationships_for_rows(
        &pool,
        &actor,
        fixture.system_id,
        &row_keys,
        None,
        None,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(current_member.len(), 1);
    let direct_current_member =
        poam_service::cve_relationships(&pool, &actor, fixture.system_id, None, None, &clock)
            .await
            .unwrap();
    assert_eq!(direct_current_member.len(), 100);

    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1 AND environment_id=$2")
        .bind(actor.user_id)
        .bind(environment_id)
        .execute(&pool)
        .await
        .unwrap();
    let revoked = poam_service::cve_relationships_for_rows(
        &pool,
        &actor,
        fixture.system_id,
        &row_keys,
        None,
        None,
        &clock,
    )
    .await
    .unwrap_err();
    assert!(matches!(revoked, PoamError::NotFound));
    let direct_revoked =
        poam_service::cve_relationships(&pool, &actor, fixture.system_id, None, None, &clock)
            .await
            .unwrap_err();
    assert!(matches!(direct_revoked, PoamError::NotFound));
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_system_vulnerability_rows_exclude_newer_undeployed_scan(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let clock = FixedClock(Utc::now());
    let deployed_cve = "CVE-2098-44001";
    let undeployed_cve = "CVE-2098-44002";
    assert!(
        fetch_exact_system_vulnerabilities(&pool, fixture.system_id)
            .await
            .unwrap()
            .is_empty(),
        "missing schema-1 evidence must not fall back to inferred rows"
    );
    seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now() - TimeDelta::minutes(1),
        Some((deployed_cve, "deployed-package", "1.0.0", false)),
    )
    .await;

    let (repository, hostname): (String, String) = sqlx::query_as(
        r#"SELECT flake.repo_url,derivation.derivation_name
           FROM derivations derivation
           JOIN commits commit ON commit.id=derivation.commit_id
           JOIN flakes flake ON flake.id=commit.flake_id
           WHERE derivation.id=$1"#,
    )
    .bind(fixture.derivation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let undeployed_commit = Uuid::new_v4().simple().to_string();
    insert_commit(&pool, &undeployed_commit, &repository, clock.now())
        .await
        .unwrap();
    let undeployed_commit_id: i32 =
        sqlx::query_scalar("SELECT id FROM commits WHERE git_commit_hash=$1")
            .bind(&undeployed_commit)
            .fetch_one(&pool)
            .await
            .unwrap();
    let undeployed_store_path = format!("/nix/store/{undeployed_commit}-undeployed");
    let undeployed_derivation = match record_successful_eval_result(
        &pool,
        Some(undeployed_commit_id),
        &hostname,
        "nixos",
        None,
        &format!("{undeployed_store_path}.drv"),
        Some(&undeployed_store_path),
        Some(true),
        true,
        &serde_json::json!({}),
    )
    .await
    .unwrap()
    {
        SuccessfulEvalWrite::Inserted { derivation_id }
        | SuccessfulEvalWrite::UpdatedEvaluationState { derivation_id }
        | SuccessfulEvalWrite::PreservedBuildState { derivation_id, .. }
        | SuccessfulEvalWrite::LegacyPathConflict { derivation_id } => derivation_id,
    };
    seal_exact_cve_scan_for_derivation(
        &pool,
        fixture.system_id,
        undeployed_derivation,
        clock.now(),
        Some((undeployed_cve, "undeployed-package", "2.0.0", false)),
    )
    .await;

    let rows = fetch_exact_system_vulnerabilities(&pool, fixture.system_id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cve_id, deployed_cve);
    assert_eq!(rows[0].package_name, "deployed-package");
}

#[sqlx::test(migrations = "./migrations")]
async fn exact_system_vulnerability_rows_hydrate_overlong_version_context(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    sync_user_role(&pool, fixture.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc::now());
    let cve_id = "CVE-2098-44003";
    let package_name = "rebuilt-package";
    let package_version = format!("1.0.0+{}", "rebuild".repeat(32));
    seal_exact_cve_scan(
        &pool,
        &fixture,
        clock.now(),
        Some((cve_id, package_name, &package_version, false)),
    )
    .await;

    let rows = fetch_exact_system_vulnerabilities(&pool, fixture.system_id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cve_id, cve_id);
    assert_eq!(rows[0].installed_version, package_version);

    let conflicting = poam_service::cve_relationships_for_rows(
        &pool,
        &actor,
        fixture.system_id,
        &[CveRelationshipRowKey {
            canonical_cve_id: rows[0].cve_id.clone(),
            canonical_package_name: rows[0].canonical_package_name.clone(),
            scan_id: rows[0].scan_id,
            occurrence_derivation_path: format!("{}-conflict", rows[0].occurrence_derivation_path),
        }],
        None,
        None,
        &clock,
    )
    .await
    .unwrap();
    assert!(
        conflicting.is_empty(),
        "stable identity must not substitute a conflicting occurrence"
    );

    let relationships = poam_service::cve_relationships_for_rows(
        &pool,
        &actor,
        fixture.system_id,
        &[CveRelationshipRowKey {
            canonical_cve_id: rows[0].cve_id.clone(),
            canonical_package_name: rows[0].canonical_package_name.clone(),
            scan_id: rows[0].scan_id,
            occurrence_derivation_path: rows[0].occurrence_derivation_path.clone(),
        }],
        None,
        None,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(relationships.len(), 1);
    assert_eq!(relationships[0].observed_package_version, package_version);
    assert_eq!(relationships[0].observed_package_name, package_name);
}

#[sqlx::test(migrations = "./migrations")]
async fn linked_finding_requirement_metadata_is_authoritative_and_batched(pool: PgPool) {
    let framework_id = Uuid::new_v4();
    let framework_version_id = Uuid::new_v4();
    let requirement_id = Uuid::new_v4();
    let requirement_version_id = Uuid::new_v4();
    sqlx::query("INSERT INTO compliance_frameworks(id,name,canonical_source_key) VALUES($1,$2,$3)")
        .bind(framework_id)
        .bind("NIST SP 800-53")
        .bind(format!("poam-metadata-{framework_id}"))
        .execute(&pool)
        .await
        .expect("insert framework");
    sqlx::query(
        "INSERT INTO compliance_framework_versions(id,framework_id,version,canonical_release_key,title) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(framework_version_id)
    .bind(framework_id)
    .bind("Rev. 5")
    .bind(format!("release-{framework_version_id}"))
    .bind("Security and Privacy Controls")
    .execute(&pool)
    .await
    .expect("insert framework version");
    sqlx::query(
        "INSERT INTO compliance_requirements(id,framework_id,canonical_requirement_key) VALUES($1,$2,$3)",
    )
    .bind(requirement_id)
    .bind(framework_id)
    .bind("AC-2")
    .execute(&pool)
    .await
    .expect("insert requirement");
    sqlx::query(
        "INSERT INTO compliance_requirement_versions(id,requirement_id,framework_version_id,external_id,title,kind) VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(requirement_version_id)
    .bind(requirement_id)
    .bind(framework_version_id)
    .bind("AC-2")
    .bind("Account Management")
    .bind("control")
    .execute(&pool)
    .await
    .expect("insert requirement version");

    let mut tx = pool.begin().await.expect("begin metadata query");
    let rows =
        poam::finding_requirement_metadata(&mut tx, &[requirement_version_id, Uuid::new_v4()])
            .await
            .expect("load requirement metadata");
    assert_eq!(rows.len(), 1, "unknown UUIDs must not fabricate metadata");
    assert_eq!(rows[0].external_id, "AC-2");
    assert_eq!(rows[0].title.as_deref(), Some("Account Management"));
    assert_eq!(rows[0].framework_name, "NIST SP 800-53");
    assert_eq!(rows[0].framework_version, "Rev. 5");
}

async fn add_requirement_mapping(pool: &PgPool, policy_version_id: Uuid) -> Uuid {
    let framework_id = Uuid::new_v4();
    let framework_version_id = Uuid::new_v4();
    let requirement_id = Uuid::new_v4();
    let requirement_version_id = Uuid::new_v4();
    sqlx::query("INSERT INTO compliance_frameworks(id,name,canonical_source_key) VALUES($1,'Verification Framework',$2)")
        .bind(framework_id)
        .bind(format!("verification-{framework_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO compliance_framework_versions(id,framework_id,version,canonical_release_key) VALUES($1,$2,'Version 3',$3)")
        .bind(framework_version_id)
        .bind(framework_id)
        .bind(format!("verification-release-{framework_version_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO compliance_requirements(id,framework_id,canonical_requirement_key) VALUES($1,$2,'VR-3')")
        .bind(requirement_id)
        .bind(framework_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO compliance_requirement_versions(id,requirement_id,framework_version_id,external_id,title,kind) VALUES($1,$2,$3,'VR-3','Durable verification identity','control')")
        .bind(requirement_version_id)
        .bind(requirement_id)
        .bind(framework_version_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO policy_requirement_mappings(policy_version_id,requirement_version_id,relationship,coverage,provenance,trust_state) VALUES($1,$2,'implements','full','manual','trusted')")
        .bind(policy_version_id)
        .bind(requirement_version_id)
        .execute(pool)
        .await
        .unwrap();
    requirement_version_id
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

fn policy_results_for_versions(
    version_ids: &[Uuid],
    config: &CompositePolicyConfig,
    outcomes: &[EnforcementOutcome],
) -> serde_json::Value {
    let mut assigned = serde_json::Map::new();
    for (version_id, outcome) in version_ids.iter().zip(outcomes) {
        assigned.insert(
            version_id.to_string(),
            policy_results(*version_id, config, *outcome)["assigned"][version_id.to_string()]
                .clone(),
        );
    }
    serde_json::json!({"assigned": assigned})
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

async fn persist_legacy_assessment_group(
    pool: &PgPool,
    fixture: &mut AssessmentFixture,
    version_ids: &[Uuid],
    outcomes: &[EnforcementOutcome],
) {
    fixture.resolved = match resolve_system_effective_policies(pool, fixture.system_id)
        .await
        .unwrap()
    {
        ResolutionOutcome::Resolved(resolved) => resolved,
        ResolutionOutcome::Conflict(conflicts) => panic!("unexpected conflict: {conflicts:?}"),
    };
    let mut tx = pool.begin().await.unwrap();
    persist_evaluation_assessments_in_tx(
        &mut tx,
        fixture.system_id,
        fixture.derivation_id,
        &fixture.store_path,
        &policy_results_for_versions(version_ids, &fixture.config, outcomes),
        &fixture.resolved,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    sqlx::query(
        "UPDATE composite_policy_assessments SET effective_set_digest=$1 WHERE system_id=$2",
    )
    .bind(&fixture.resolved.effective_set_digest)
    .bind(fixture.system_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn add_composite_policy(pool: &PgPool, system_id: Uuid) -> (Uuid, Uuid) {
    let policy = create_deployment_policy(
        pool,
        &CreateDeploymentPolicyRequest {
            name: format!("poam-legacy-extra-policy-{}", Uuid::new_v4()),
            policy_type: "composite".into(),
            config: serde_json::to_value(assessment_config()).unwrap(),
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let version_id: Uuid =
        sqlx::query_scalar("SELECT current_draft_version_id FROM deployment_policies WHERE id=$1")
            .bind(policy.id)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET trust_state='trusted' WHERE id=$1")
        .bind(version_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO system_policies(system_id,policy_id) VALUES($1,$2)")
        .bind(system_id)
        .bind(policy.id)
        .execute(pool)
        .await
        .unwrap();
    (policy.id, version_id)
}

async fn assign_policy_through_bundle(
    pool: &PgPool,
    system_id: Uuid,
    policy_id: Uuid,
    policy_version_id: Uuid,
) -> Uuid {
    let mut policy_tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id=NULL WHERE id=$1")
        .bind(policy_id)
        .execute(&mut *policy_tx)
        .await
        .unwrap();
    sqlx::query("UPDATE deployment_policy_versions SET publication_state='accepted',trust_state='trusted' WHERE id=$1")
        .bind(policy_version_id)
        .execute(&mut *policy_tx)
        .await
        .unwrap();
    sqlx::query("UPDATE deployment_policies SET current_published_version_id=$1 WHERE id=$2")
        .bind(policy_version_id)
        .bind(policy_id)
        .execute(&mut *policy_tx)
        .await
        .unwrap();
    policy_tx.commit().await.unwrap();

    let bundle_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundles(name,framework,version,layer) VALUES($1,'test','1.0','fleet') RETURNING id",
    )
    .bind(format!("poam-mode-transition-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .unwrap();
    let bundle_version_id: Uuid =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id=$1")
            .bind(bundle_id)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO compliance_bundle_version_policies(bundle_version_id,policy_version_id,policy_order) VALUES($1,$2,0)")
        .bind(bundle_version_id)
        .bind(policy_version_id)
        .execute(pool)
        .await
        .unwrap();
    let mut bundle_tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id=NULL WHERE id=$1")
        .bind(bundle_id)
        .execute(&mut *bundle_tx)
        .await
        .unwrap();
    sqlx::query("UPDATE compliance_bundle_versions SET publication_state='accepted',trust_state='trusted',semantic_digest=$1 WHERE id=$2")
        .bind(format!("poam-mode-transition-{bundle_version_id}"))
        .bind(bundle_version_id)
        .execute(&mut *bundle_tx)
        .await
        .unwrap();
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id=$1 WHERE id=$2")
        .bind(bundle_version_id)
        .bind(bundle_id)
        .execute(&mut *bundle_tx)
        .await
        .unwrap();
    bundle_tx.commit().await.unwrap();

    let assignment_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_bundle_assignments
             (bundle_id,bundle_version_id,system_id,scope_type,active,enforcement_mode,assignment_overlay_digest)
           VALUES($1,$2,$3,'system',true,'enforce','poam-mode-transition') RETURNING id"#,
    )
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(system_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let assignment_version_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_bundle_assignment_versions
             (assignment_id,version_number,bundle_version_id,enforcement_mode,assignment_overlay_digest)
           VALUES($1,1,$2,'enforce','poam-mode-transition') RETURNING id"#,
    )
    .bind(assignment_id)
    .bind(bundle_version_id)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id=$1 WHERE id=$2")
        .bind(assignment_version_id)
        .bind(assignment_id)
        .execute(pool)
        .await
        .unwrap();
    assignment_id
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
    assert!(
        actor.is_admin,
        "service POA&M fixture requires an Admin actor"
    );
    let has_role: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_role_assignments WHERE user_id=$1)")
            .bind(actor.user_id)
            .fetch_one(pool)
            .await
            .unwrap();
    if !has_role {
        sync_user_role(pool, actor.user_id, AuthRole::Admin)
            .await
            .unwrap();
    }
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
            assignee: None,
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

async fn record_later_legacy_result(
    pool: &PgPool,
    fixture: &AssessmentFixture,
    passed: bool,
    label: &str,
) -> (i32, String, serde_json::Value) {
    let (flake_id, derivation_name, derivation_type): (i32, String, String) = sqlx::query_as(
        "SELECT commit.flake_id,derivation.derivation_name,derivation.derivation_type \
         FROM derivations derivation JOIN commits commit ON commit.id=derivation.commit_id \
         WHERE derivation.id=$1",
    )
    .bind(fixture.derivation_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let commit_id: i32 = sqlx::query_scalar(
        "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) \
         VALUES($1,$2,CURRENT_TIMESTAMP) RETURNING id",
    )
    .bind(flake_id)
    .bind(format!("poam-legacy-{label}-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .unwrap();
    let store_path = format!("/nix/store/{}-poam-legacy-{label}", Uuid::new_v4().simple());
    let result = serde_json::json!({
        "assigned": {
            fixture.version_id.to_string(): {
                "passed": passed,
                "details": format!("later legacy check {}", if passed { "passes" } else { "fails" })
            }
        }
    });
    let write = record_successful_eval_result(
        pool,
        Some(commit_id),
        &derivation_name,
        &derivation_type,
        None,
        &format!("{store_path}.drv"),
        Some(&store_path),
        Some(true),
        true,
        &result,
    )
    .await
    .unwrap();
    let derivation_id = match write {
        SuccessfulEvalWrite::Inserted { derivation_id }
        | SuccessfulEvalWrite::UpdatedEvaluationState { derivation_id }
        | SuccessfulEvalWrite::PreservedBuildState { derivation_id, .. }
        | SuccessfulEvalWrite::LegacyPathConflict { derivation_id } => derivation_id,
    };
    sqlx::query("UPDATE derivations SET store_path=$1,status_id=10 WHERE id=$2")
        .bind(&store_path)
        .bind(derivation_id)
        .execute(pool)
        .await
        .unwrap();
    deploy_store_path(pool, fixture, &store_path).await;
    (derivation_id, store_path, result)
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
        assignee: None,
        target_date: None,
        risk: PoamRisk::High,
        default_milestones: false,
        assignment_version_ids: Vec::new(),
    }
}

async fn assessment_create_request(
    pool: &PgPool,
    fixture: &AssessmentFixture,
    title: &str,
    owner: &str,
    assignee: Option<PoamAssigneeRequest>,
) -> CreatePoamRequest {
    CreatePoamRequest {
        assessment_id: Some(current_assessment_id(pool, fixture).await),
        finding_id: None,
        observation: None,
        title: title.into(),
        plan: "Resolve the finding".into(),
        owner: owner.into(),
        assignee,
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
async fn legacy_fail_can_close_with_observation_bound_accepted_waiver(pool: PgPool) {
    let (fixture, finding_id, observation, persisted_result) = legacy_fail_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 30, 12, 0, 0).unwrap());
    let created = poam_service::create(
        &pool,
        &actor,
        legacy_create_request(finding_id, observation.clone()),
        &clock,
    )
    .await
    .unwrap();
    let waiver = poam_service::create_waiver(
        &pool,
        &actor,
        CreateWaiverRequest {
            finding_id,
            assessment_id: None,
            observation: Some(observation),
            justification: "Legacy remediation is accepted until replacement".into(),
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
            expires_at: None,
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
            revision: created.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: Some("Legacy risk acceptance is ready for verification".into()),
        },
        &clock,
    )
    .await
    .unwrap();
    let verified = poam_service::verify(
        &pool,
        &actor,
        created.poam.id,
        awaiting.poam.revision,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(verified["outcome"], "accepted");
    assert_eq!(verified["items"][0]["result"], "waiver");
    assert!(verified["items"][0]["assessment_id"].is_null());
    let closed = poam_service::close(
        &pool,
        &actor,
        created.poam.id,
        verified["revision"].as_i64().unwrap(),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(closed.poam.status, "completed");
    let stored_assessment: Option<Uuid> =
        sqlx::query_scalar("SELECT assessment_id FROM finding_waivers WHERE id=$1")
            .bind(waiver_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(stored_assessment.is_none());
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
    let (fixture, finding_id, observation, initial_failing_result) =
        legacy_fail_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
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

    let (passing_derivation_id, passing_store_path, passing_result) =
        record_later_legacy_result(&pool, &fixture, true, "pass").await;
    assert_ne!(passing_derivation_id, fixture.derivation_id);
    assert_ne!(passing_store_path, fixture.store_path);

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

    // INVARIANT: A copied source-neutral Pass item is not accepted without an
    // attempt-bound effective-context attestation. The item constraint rejects
    // forged evidence before the deferred closure constraint must inspect it.
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
        .bind(passing_derivation_id)
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
    let forged_error = sqlx::query(
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
    .unwrap_err();
    assert_eq!(
        forged_error.as_database_error().unwrap().constraint(),
        Some("poam_verification_items_accepted_evidence")
    );
    forged.rollback().await.unwrap();

    sqlx::query("UPDATE derivations SET policy_results=$1 WHERE id=$2")
        .bind(&passing_result)
        .bind(passing_derivation_id)
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
            && attempt.items[0].derivation_id == Some(passing_derivation_id)
            && attempt.items[0].target_store_path.as_deref() == Some(passing_store_path.as_str())
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
    let initial_result_after: serde_json::Value =
        sqlx::query_scalar("SELECT policy_results FROM derivations WHERE id=$1")
            .bind(fixture.derivation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(initial_result_after, initial_failing_result);
    let result_after: serde_json::Value =
        sqlx::query_scalar("SELECT policy_results FROM derivations WHERE id=$1")
            .bind(passing_derivation_id)
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
async fn legacy_close_waits_for_and_rejects_newer_failed_evidence(pool: PgPool) {
    let (fixture, finding_id, observation, _) = legacy_fail_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap());
    let created = poam_service::create(
        &pool,
        &actor,
        legacy_create_request(finding_id, observation),
        &clock,
    )
    .await
    .unwrap();
    let awaiting = poam_service::transition(
        &pool,
        &actor,
        created.poam.id,
        TransitionPoamRequest {
            revision: created.poam.revision,
            status: PoamStatus::AwaitingVerification,
            note: Some("Later legacy deployment is ready".into()),
        },
        &clock,
    )
    .await
    .unwrap();
    let (later_derivation_id, _, _) =
        record_later_legacy_result(&pool, &fixture, true, "race-pass").await;

    let mut newer_fail = pool.begin().await.unwrap();
    sqlx::query("SELECT lock_poam_finding_key($1,$2)")
        .bind(fixture.system_id)
        .bind(fixture.resolved.policies[0].policy_lineage_id)
        .execute(&mut *newer_fail)
        .await
        .unwrap();
    let failing_result = serde_json::json!({
        "assigned": {
            fixture.version_id.to_string(): {
                "passed": false,
                "details": "newer legacy evaluation fails before closure"
            }
        }
    });
    sqlx::query("UPDATE derivations SET policy_results=$1 WHERE id=$2")
        .bind(&failing_result)
        .bind(later_derivation_id)
        .execute(&mut *newer_fail)
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
        "legacy closure must wait for the finding evidence writer"
    );
    newer_fail.commit().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), close_task)
        .await
        .expect("legacy closure remained blocked")
        .unwrap();
    assert!(matches!(
        result,
        Err(PoamError::Precondition("closure_not_ready", _, _))
    ));
    let state: String = sqlx::query_scalar("SELECT status FROM poams WHERE id=$1")
        .bind(created.poam.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "awaiting_verification");
    let rejected: (Option<i32>, String) = sqlx::query_as(
        "SELECT item.derivation_id,item.result FROM poam_verification_items item \
         JOIN poam_verification_attempts attempt ON attempt.id=item.attempt_id \
         WHERE attempt.poam_id=$1 ORDER BY attempt.attempted_at DESC,attempt.id DESC LIMIT 1",
    )
    .bind(created.poam.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rejected, (Some(later_derivation_id), "fail".into()));
    let composite_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM composite_policy_assessments WHERE system_id=$1")
            .bind(fixture.system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(composite_count, 0);
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
async fn legacy_complete_digest_remains_valid_for_poam_creation_and_verification(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 29, 14, 0, 0).unwrap());
    let mut failed = pool.begin().await.unwrap();
    persist_assessment(&mut failed, &fixture, EnforcementOutcome::Fail).await;
    failed.commit().await.unwrap();
    sqlx::query(
        "UPDATE composite_policy_assessments SET effective_set_digest=$1 WHERE system_id=$2",
    )
    .bind(&fixture.resolved.effective_set_digest)
    .bind(fixture.system_id)
    .execute(&pool)
    .await
    .unwrap();

    let created = create_service_poam(
        &pool,
        &fixture,
        &actor,
        &clock,
        "Legacy composite evidence compatibility",
    )
    .await;
    let awaiting = awaiting_verification(&pool, &actor, created, &clock).await;

    let mut passing = pool.begin().await.unwrap();
    persist_assessment(&mut passing, &fixture, EnforcementOutcome::Pass).await;
    passing.commit().await.unwrap();
    sqlx::query(
        "UPDATE composite_policy_assessments SET effective_set_digest=$1 WHERE system_id=$2",
    )
    .bind(&fixture.resolved.effective_set_digest)
    .bind(fixture.system_id)
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
    assert_eq!(
        verified["items"][0]["assessment_id"],
        current_assessment_id(&pool, &fixture).await.to_string()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn removed_enforce_policy_invalidates_legacy_poam_assessment(pool: PgPool) {
    let mut fixture = assessment_fixture(&pool).await;
    let (removed_policy_id, removed_version_id) =
        add_composite_policy(&pool, fixture.system_id).await;
    let original_version_id = fixture.version_id;
    persist_legacy_assessment_group(
        &pool,
        &mut fixture,
        &[original_version_id, removed_version_id],
        &[EnforcementOutcome::Fail, EnforcementOutcome::Pass],
    )
    .await;
    sqlx::query("DELETE FROM system_policies WHERE system_id=$1 AND policy_id=$2")
        .bind(fixture.system_id)
        .bind(removed_policy_id)
        .execute(&pool)
        .await
        .unwrap();

    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 29, 15, 0, 0).unwrap());
    let result = poam_service::create(
        &pool,
        &admin_actor(fixture.user_id),
        CreatePoamRequest {
            assessment_id: Some(current_assessment_id(&pool, &fixture).await),
            finding_id: None,
            observation: None,
            title: "Removed policy evidence".into(),
            plan: "Replace stale evidence".into(),
            owner: "Security Matrix".into(),
            assignee: None,
            target_date: Some(clock.today() + chrono::Duration::days(30)),
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: Vec::new(),
        },
        &clock,
    )
    .await;
    assert!(matches!(
        result,
        Err(PoamError::Precondition("stale_finding", _, _))
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn report_only_transition_invalidates_legacy_poam_assessment(pool: PgPool) {
    let mut fixture = assessment_fixture(&pool).await;
    let (transitioned_policy_id, transitioned_version_id) =
        add_composite_policy(&pool, fixture.system_id).await;
    let assignment_id = assign_policy_through_bundle(
        &pool,
        fixture.system_id,
        transitioned_policy_id,
        transitioned_version_id,
    )
    .await;
    sqlx::query("DELETE FROM system_policies WHERE system_id=$1 AND policy_id=$2")
        .bind(fixture.system_id)
        .bind(transitioned_policy_id)
        .execute(&pool)
        .await
        .unwrap();
    let original_version_id = fixture.version_id;
    persist_legacy_assessment_group(
        &pool,
        &mut fixture,
        &[original_version_id, transitioned_version_id],
        &[EnforcementOutcome::Fail, EnforcementOutcome::Pass],
    )
    .await;

    let (bundle_version_id, current_version_id): (Uuid, Uuid) = sqlx::query_as(
        "SELECT bundle_version_id,current_version_id FROM compliance_bundle_assignments WHERE id=$1",
    )
    .bind(assignment_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let next_version_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_bundle_assignment_versions
             (assignment_id,version_number,bundle_version_id,enforcement_mode,assignment_overlay_digest)
           VALUES($1,2,$2,'report_only','poam-mode-transition') RETURNING id"#,
    )
    .bind(assignment_id)
    .bind(bundle_version_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id=$1,enforcement_mode='report_only' WHERE id=$2 AND current_version_id=$3")
        .bind(next_version_id)
        .bind(assignment_id)
        .bind(current_version_id)
        .execute(&pool)
        .await
        .unwrap();

    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 29, 16, 0, 0).unwrap());
    let result = poam_service::create(
        &pool,
        &admin_actor(fixture.user_id),
        CreatePoamRequest {
            assessment_id: Some(current_assessment_id(&pool, &fixture).await),
            finding_id: None,
            observation: None,
            title: "Report-only policy evidence".into(),
            plan: "Replace stale evidence".into(),
            owner: "Security Matrix".into(),
            assignee: None,
            target_date: Some(clock.today() + chrono::Duration::days(30)),
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: Vec::new(),
        },
        &clock,
    )
    .await;
    assert!(matches!(
        result,
        Err(PoamError::Precondition("stale_finding", _, _))
    ));
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

async fn failing_assessment_fixture(pool: &PgPool) -> AssessmentFixture {
    let fixture = assessment_fixture(pool).await;
    let mut tx = pool.begin().await.unwrap();
    persist_assessment(&mut tx, &fixture, EnforcementOutcome::Fail).await;
    tx.commit().await.unwrap();
    fixture
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
        | SuccessfulEvalWrite::PreservedBuildState { derivation_id, .. }
        | SuccessfulEvalWrite::LegacyPathConflict { derivation_id } => derivation_id,
    };
    sqlx::query("UPDATE derivations SET store_path=$1,status_id=10 WHERE id=$2")
        .bind(&store_path)
        .bind(derivation_id)
        .execute(pool)
        .await
        .unwrap();
    let mut snapshot_tx = pool.begin().await.unwrap();
    let snapshot_id =
        persist_available_snapshot_tx(&mut snapshot_tx, commit_id, &hostname, Vec::new())
            .await
            .unwrap();
    snapshot_tx.commit().await.unwrap();
    sqlx::query(
        r#"INSERT INTO evaluation_generation_snapshots(
             system_id,generation,snapshot_id,derivation_id,commit_id,
             source_store_path,configuration_name)
           VALUES($1,1,$2,$3,$4,$5,$6)"#,
    )
    .bind(system_id)
    .bind(snapshot_id)
    .bind(derivation_id)
    .bind(commit_id)
    .bind(&store_path)
    .bind(&hostname)
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
        "UPDATE system_states SET timestamp=clock_timestamp()+INTERVAL '1 day' WHERE hostname=$1 AND store_path=$2 AND generation=2",
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
            "/current-system",
            axum::routing::post(agent_state_handler::update),
        )
        .route("/api/v1/cves", get(cve_handlers::list_cves))
        .route("/api/v1/cves/grouped", get(cve_handlers::list_cves_grouped))
        .route("/api/v1/cves/stats", get(cve_handlers::get_fleet_stats))
        .route(
            "/api/v1/cves/packages",
            get(cve_handlers::list_package_names),
        )
        .route("/api/v1/cves/export", get(cve_handlers::export_cves))
        .route("/api/v1/cves/:cve_id", get(cve_handlers::get_cve_detail))
        .route(
            "/api/v1/cves/:cve_id/systems",
            get(cve_handlers::get_cve_systems),
        )
        .route(
            "/api/v1/cves/:cve_id/justifications",
            get(cve_handlers::list_justifications),
        )
        .route(
            "/api/v1/cves/:cve_id/justification",
            axum::routing::post(cve_handlers::save_justification)
                .delete(cve_handlers::revoke_justification),
        )
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
            "/api/v1/poams/assignees",
            get(poam_handlers::assignee_catalog),
        )
        .route(
            "/api/v1/cves/:cve_id/fleet",
            get(poam_handlers::fleet_cve_detail),
        )
        .route(
            "/api/v1/cves/:cve_id/triage",
            axum::routing::post(poam_handlers::triage_fleet_cve),
        )
        .route(
            "/api/v1/systems/:id/cves/:cve_id/justification",
            axum::routing::put(system_handlers::save_system_cve_justification),
        )
        .route(
            "/api/v1/systems/:id/cves/:cve_id/triage",
            get(poam_handlers::system_cve_triage_detail).post(poam_handlers::triage_system_cve),
        )
        .route(
            "/api/v1/systems/:id",
            axum::routing::patch(system_handlers::update_system_handler),
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
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
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
            assignee: None,
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
    let requirement_version_id = add_requirement_mapping(&pool, fixture.version_id).await;
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
            assessment_id: Some(assessment_id),
            observation: None,
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
            assessment_id: Some(assessment_id),
            observation: None,
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
    let original_hostname: String = sqlx::query_scalar("SELECT hostname FROM systems WHERE id=$1")
        .bind(fixture.system_id)
        .fetch_one(&pool)
        .await
        .unwrap();
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
    let closed_verification_item = closed
        .verification_attempts
        .iter()
        .find(|attempt| attempt.id == closure_id)
        .and_then(|attempt| attempt.items.first())
        .expect("close response includes its sealed verification item");
    assert_eq!(closed_verification_item.requirements.len(), 1);
    assert_eq!(
        closed_verification_item.requirements[0].framework_name,
        "Verification Framework"
    );
    assert_eq!(closed_verification_item.requirements[0].external_id, "VR-3");
    let renamed_hostname = format!("renamed-{}", Uuid::new_v4());
    sqlx::query("UPDATE systems SET hostname=$2 WHERE id=$1")
        .bind(fixture.system_id)
        .bind(&renamed_hostname)
        .execute(&pool)
        .await
        .unwrap();
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
    assert!(
        sqlx::query("DELETE FROM derivations WHERE id=$1")
            .bind(fixture.derivation_id)
            .execute(&pool)
            .await
            .is_err(),
        "a retained generation must retain its authoritative derivation"
    );
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
    let verification_item = closed_detail
        .verification_attempts
        .iter()
        .find(|attempt| attempt.id == closure_id)
        .and_then(|attempt| attempt.items.first())
        .expect("closure verification item remains visible after link retirement");
    assert_eq!(verification_item.hostname, original_hostname);
    assert_ne!(verification_item.hostname, renamed_hostname);
    assert!(
        verification_item
            .policy_name
            .starts_with("poam-race-policy-")
    );
    assert!(verification_item.policy_version.is_some());
    assert_eq!(
        verification_item.requirement_version_ids,
        vec![requirement_version_id]
    );
    assert_eq!(verification_item.requirements.len(), 1);
    assert_eq!(
        verification_item.requirements[0].framework_name,
        "Verification Framework"
    );
    assert_eq!(
        verification_item.requirements[0].framework_version,
        "Version 3"
    );
    assert_eq!(verification_item.requirements[0].external_id, "VR-3");
}

#[sqlx::test]
async fn history_and_batch_inputs_are_bounded_with_continuation(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 26, 12, 0, 0).unwrap());
    let mut tx = pool.begin().await.unwrap();
    let (poam_id, _) = create_poam(&mut tx, &fixture, "History paging", clock.today()).await;
    let mut policy_link_ids = vec![fixture.finding_id];
    for index in 0..2 {
        let system_id: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname,public_key,derivation) VALUES($1,$2,$2) RETURNING id",
        )
        .bind(format!("history-policy-{index}-{}", Uuid::new_v4()))
        .bind(format!("history-policy-key-{index}-{}", Uuid::new_v4()))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let finding_id: Uuid = sqlx::query_scalar(
            "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
        )
        .bind(system_id)
        .bind(fixture.policy_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO poam_finding_links(poam_id,finding_id,linked_by,linked_at) VALUES($1,$2,$3,$4)",
        )
        .bind(poam_id)
        .bind(finding_id)
        .bind(fixture.user_id)
        .bind(clock.now() + TimeDelta::minutes(index + 1))
        .execute(&mut *tx)
        .await
        .unwrap();
        policy_link_ids.push(finding_id);
    }
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
    let policy_first = poam_service::detail_with_history(
        &pool,
        &actor,
        poam_id,
        &PoamDetailQuery {
            finding_limit: Some(2),
            ..Default::default()
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(policy_first.findings.len(), 2);
    assert!(policy_first.cve_findings.is_empty());
    assert!(policy_first.findings_has_more);
    let policy_cursor = policy_first.findings_next_cursor.as_ref().unwrap();
    let policy_second = poam_service::detail_with_history(
        &pool,
        &actor,
        poam_id,
        &PoamDetailQuery {
            finding_limit: Some(2),
            finding_before_at: Some(policy_cursor.at),
            finding_before_id: Some(policy_cursor.id),
            ..Default::default()
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(policy_second.findings.len(), 1);
    assert!(!policy_second.findings_has_more);
    let returned_policy_ids = policy_first
        .findings
        .iter()
        .chain(&policy_second.findings)
        .map(|finding| finding.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(returned_policy_ids, policy_link_ids.into_iter().collect());
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
async fn cve_finding_history_cursor_ignores_inaccessible_retired_links(pool: PgPool) {
    let visible_environment: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("history-visible-{}", Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .unwrap();
    let hidden_environment: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("history-hidden-{}", Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .unwrap();
    let fixtures = [
        assessment_fixture(&pool).await,
        assessment_fixture(&pool).await,
        assessment_fixture(&pool).await,
    ];
    let hidden = assessment_fixture(&pool).await;
    for fixture in &fixtures {
        sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
            .bind(fixture.system_id)
            .bind(visible_environment)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(hidden.system_id)
        .bind(hidden_environment)
        .execute(&pool)
        .await
        .unwrap();
    let cve_id = "CVE-2099-4409";
    let package_name = "cursor-package";
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 9, 13, 12, 0, 0).unwrap());
    sync_user_role(&pool, fixtures[0].user_id, AuthRole::Admin)
        .await
        .unwrap();
    let mut observations = Vec::new();
    for exact_fixture in fixtures.iter().chain(std::iter::once(&hidden)) {
        observations.push(
            seal_exact_cve_scan(
                &pool,
                exact_fixture,
                clock.now(),
                Some((cve_id, package_name, "1.0.0", false)),
            )
            .await
            .unwrap(),
        );
    }
    let admin = admin_actor(fixtures[0].user_id);
    let mut detail = poam_service::create_cve(
        &pool,
        &admin,
        CreateCvePoamRequest {
            observation: observations[0].clone(),
            title: "CVE cursor history".into(),
            plan: String::new(),
            owner: "Security".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: vec![],
        },
        &clock,
    )
    .await
    .unwrap();
    let poam_id = detail.poam.id;
    let mut visible_link_ids = vec![detail.cve_findings[0].link_id];
    for index in [1_usize, 3, 2] {
        detail = poam_service::link_cve_finding(
            &pool,
            &admin,
            poam_id,
            AddCveFindingRequest {
                revision: detail.poam.revision,
                observation: observations[index].clone(),
            },
            &clock,
        )
        .await
        .unwrap();
        if index != 3 {
            visible_link_ids.push(
                detail
                    .cve_findings
                    .iter()
                    .find(|finding| finding.system_id == fixtures[index].system_id)
                    .unwrap()
                    .link_id,
            );
        }
    }
    let hidden_finding_id = detail
        .cve_findings
        .iter()
        .find(|finding| finding.system_id == hidden.system_id)
        .unwrap()
        .id;
    poam_service::unlink_cve_finding(
        &pool,
        &admin,
        poam_id,
        hidden_finding_id,
        detail.poam.revision,
        &clock,
    )
    .await
    .unwrap();

    let actor = PoamActor {
        user_id: fixtures[0].user_id,
        identifier: "finding-history-operator@example.invalid".into(),
        is_admin: false,
        can_mutate: true,
        environment_ids: vec![visible_environment],
        request_origin: None,
    };
    let first = poam_service::detail_with_history(
        &pool,
        &actor,
        poam_id,
        &PoamDetailQuery {
            finding_limit: Some(2),
            ..Default::default()
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(first.findings.is_empty());
    assert_eq!(first.cve_findings.len(), 2);
    assert!(first.findings_has_more);
    let cursor = first.findings_next_cursor.as_ref().unwrap();
    let second = poam_service::detail_with_history(
        &pool,
        &actor,
        poam_id,
        &PoamDetailQuery {
            finding_limit: Some(2),
            finding_before_at: Some(cursor.at),
            finding_before_id: Some(cursor.id),
            ..Default::default()
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(second.cve_findings.len(), 1);
    assert!(!second.findings_has_more);
    let returned_link_ids = first
        .cve_findings
        .iter()
        .chain(&second.cve_findings)
        .map(|finding| finding.link_id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(returned_link_ids, visible_link_ids.into_iter().collect());
}

#[sqlx::test]
async fn system_and_bundle_rollups_reject_scope_expansion_above_finding_ceiling(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let mut initial = pool.begin().await.unwrap();
    persist_assessment(&mut initial, &fixture, EnforcementOutcome::Fail).await;
    initial.commit().await.unwrap();
    let (assignment_id, assignment_version_id, bundle_id) =
        immutable_assignment_fixture(&pool, fixture.system_id, fixture.user_id).await;
    sqlx::query("UPDATE compliance_bundle_assignments SET active=true WHERE id=$1")
        .bind(assignment_id)
        .execute(&pool)
        .await
        .unwrap();

    let prefix = format!("poam-rollup-ceiling-{}-", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO deployment_policies(name,policy_type,config) \
         SELECT $1||ordinal,'custom_check','{}'::jsonb FROM generate_series(1,1001) ordinal",
    )
    .bind(&prefix)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO deployment_policy_versions( \
           policy_id,version,name,policy_type,config,semantic_digest,trust_state) \
         SELECT id,'1.0.0',name,policy_type,config,'rollup-ceiling-digest','trusted' \
         FROM deployment_policies WHERE name LIKE $1",
    )
    .bind(format!("{prefix}%"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO poam_findings(system_id,policy_lineage_id) \
         SELECT $1,id FROM deployment_policies WHERE name LIKE $2",
    )
    .bind(fixture.system_id)
    .bind(format!("{prefix}%"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO compliance_assignment_additions( \
           assignment_id,assignment_version_id,policy_version_id,addition_order) \
         SELECT $1,$2,version.id,(ROW_NUMBER() OVER (ORDER BY version.id)-1)::integer \
         FROM deployment_policy_versions version \
         JOIN deployment_policies policy ON policy.id=version.policy_id WHERE policy.name LIKE $3",
    )
    .bind(assignment_id)
    .bind(assignment_version_id)
    .bind(format!("{prefix}%"))
    .execute(&pool)
    .await
    .unwrap();

    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap());
    assert!(matches!(
        poam_service::system_rollups(&pool, &actor, &[fixture.system_id], &clock).await,
        Err(PoamError::Validation("rollup_scope_too_large", _))
    ));
    assert!(matches!(
        poam_service::bundle_rollups(&pool, &actor, &[bundle_id], &clock).await,
        Err(PoamError::Validation("rollup_scope_too_large", _))
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
    sync_user_role(&pool, actor.user_id, AuthRole::Admin)
        .await
        .unwrap();
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
            assignee: None,
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
            assessment_id: Some(assessment_id),
            observation: None,
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
            assessment_id: Some(primary_assessment),
            observation: None,
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
                assessment_id: Some(primary_assessment),
                observation: None,
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
            assessment_id: Some(primary_assessment),
            observation: None,
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
            assessment_id: Some(primary_assessment),
            observation: None,
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
            assessment_id: Some(primary_assessment),
            observation: None,
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
    let visible_cve_id = "CVE-2026-44020";
    let hidden_cve_id = "CVE-2026-44021";
    let package_name = "openssl";
    seal_exact_cve_scan(
        &pool,
        &primary,
        Utc::now(),
        Some((visible_cve_id, package_name, "3.0.10", false)),
    )
    .await;
    seal_exact_cve_scan(
        &pool,
        &hidden,
        Utc::now(),
        Some((hidden_cve_id, package_name, "3.0.10", false)),
    )
    .await;
    let base = poam_http_server(pool.clone()).await;
    let client = reqwest::Client::new();
    let csrf = "poam-http-csrf";
    let triage_body = serde_json::json!({
        "canonical_package_name": package_name,
        "actions": [{
            "action": "accept_risk",
            "environment_id": dev,
            "justification": "Risk is accepted for this focused HTTP test",
            "review_date": null
        }],
        "poam": null
    });
    let triage_without_csrf = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/cves/{visible_cve_id}/triage"),
        &operator,
        None,
    )
    .json(&triage_body)
    .send()
    .await
    .unwrap();
    assert_eq!(triage_without_csrf.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        triage_without_csrf
            .json::<serde_json::Value>()
            .await
            .unwrap()["error"],
        "csrf_validation_failed"
    );
    let viewer_triage = http_request(
        &client,
        reqwest::Method::POST,
        format!("{base}/api/v1/cves/{visible_cve_id}/triage"),
        &viewer,
        Some(csrf),
    )
    .json(&triage_body)
    .send()
    .await
    .unwrap();
    assert_eq!(viewer_triage.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        viewer_triage.json::<serde_json::Value>().await.unwrap()["error"],
        "forbidden"
    );
    let hidden_fleet = http_request(
        &client,
        reqwest::Method::GET,
        format!("{base}/api/v1/cves/{hidden_cve_id}/fleet?package={package_name}"),
        &operator,
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(hidden_fleet.status(), reqwest::StatusCode::NOT_FOUND);
    assert_eq!(
        hidden_fleet.json::<serde_json::Value>().await.unwrap()["error"],
        "not_found"
    );
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
    let first_historical = poam_service::close(
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
    let second =
        create_service_poam(&pool, &visible, &admin, &clock, "Second historical POAM").await;
    let second = awaiting_verification(&pool, &admin, second, &clock).await;
    let mut passing = pool.begin().await.unwrap();
    persist_assessment(&mut passing, &visible, EnforcementOutcome::Pass).await;
    passing.commit().await.unwrap();
    let second_historical =
        poam_service::close(&pool, &admin, second.poam.id, second.poam.revision, &clock)
            .await
            .unwrap();
    let mut failing = pool.begin().await.unwrap();
    persist_assessment(&mut failing, &visible, EnforcementOutcome::Fail).await;
    failing.commit().await.unwrap();
    let current_assessment = current_assessment_id(&pool, &visible).await;
    let active = create_service_poam(&pool, &visible, &admin, &clock, "Active POAM").await;

    // COMPATIBILITY: A deployed client that sends no pagination parameters
    // must continue to receive the complete relationship history.
    let legacy_relationships = poam_service::finding_relationships(
        &pool,
        &actor,
        &[current_assessment],
        None,
        None,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(legacy_relationships[0].historical_poams.len(), 2);
    assert!(!legacy_relationships[0].historical_has_more);
    assert_eq!(legacy_relationships[0].historical_next_offset, None);
    let legacy_by_finding = poam_service::finding_relationships_by_finding(
        &pool,
        &actor,
        &[legacy_relationships[0].finding_id],
        None,
        None,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(legacy_by_finding[0].historical_poams.len(), 2);
    assert!(!legacy_by_finding[0].historical_has_more);
    assert!(matches!(
        poam_service::finding_relationships(
            &pool,
            &actor,
            &[current_assessment],
            None,
            Some(1),
            &clock,
        )
        .await,
        Err(PoamError::Validation("invalid_relationship_pagination", _))
    ));

    let relationships = poam_service::finding_relationships(
        &pool,
        &actor,
        &[current_assessment, hidden_assessment, Uuid::new_v4()],
        Some(1),
        Some(0),
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
    assert_eq!(relationships[0].historical_poams[0].status, "completed");
    assert!(relationships[0].historical_has_more);
    assert_eq!(relationships[0].historical_next_offset, Some(1));
    let second_page = poam_service::finding_relationships(
        &pool,
        &actor,
        &[current_assessment],
        Some(1),
        Some(1),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(second_page[0].historical_poams.len(), 1);
    assert!(!second_page[0].historical_has_more);
    assert_eq!(second_page[0].historical_next_offset, None);
    let returned_history = [
        relationships[0].historical_poams[0].id,
        second_page[0].historical_poams[0].id,
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        returned_history,
        [first_historical.poam.id, second_historical.poam.id]
            .into_iter()
            .collect()
    );
    let original_finding_order = [
        relationships[0].historical_poams[0].id,
        second_page[0].historical_poams[0].id,
    ];
    sqlx::query("UPDATE poams SET updated_at=updated_at+INTERVAL '100 years' WHERE id=$1")
        .bind(original_finding_order[1])
        .execute(&pool)
        .await
        .unwrap();
    let finding_after_update_first = poam_service::finding_relationships(
        &pool,
        &actor,
        &[current_assessment],
        Some(1),
        Some(0),
        &clock,
    )
    .await
    .unwrap();
    let finding_after_update_second = poam_service::finding_relationships(
        &pool,
        &actor,
        &[current_assessment],
        Some(1),
        Some(1),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        [
            finding_after_update_first[0].historical_poams[0].id,
            finding_after_update_second[0].historical_poams[0].id,
        ],
        original_finding_order,
        "mutable POA&M updates must not reorder finding relationship pages"
    );

    let (assignment_id, assignment_version_id, _) =
        immutable_assignment_fixture(&pool, visible.system_id, visible.user_id).await;
    let assignment_before = assignment_snapshot(&pool, assignment_version_id).await;
    for poam_id in [
        active.poam.id,
        first_historical.poam.id,
        second_historical.poam.id,
    ] {
        sqlx::query("INSERT INTO poam_assignment_references(poam_id,assignment_id,assignment_version_id,added_by) VALUES($1,$2,$3,$4)")
            .bind(poam_id)
            .bind(assignment_id)
            .bind(assignment_version_id)
            .bind(visible.user_id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let assignments = poam_service::assignment_relationships(
        &pool,
        &actor,
        &[assignment_version_id, Uuid::new_v4()],
        Some(1),
        Some(0),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].assignment_version_id, assignment_version_id);
    assert_eq!(assignments[0].poams.len(), 1);
    assert!(assignments[0].poams_has_more);
    assert_eq!(assignments[0].poams_next_offset, Some(1));
    let assignment_second_page = poam_service::assignment_relationships(
        &pool,
        &actor,
        &[assignment_version_id],
        Some(1),
        Some(1),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(assignment_second_page[0].poams.len(), 1);
    assert!(assignment_second_page[0].poams_has_more);
    assert_eq!(assignment_second_page[0].poams_next_offset, Some(2));
    let assignment_third_page = poam_service::assignment_relationships(
        &pool,
        &actor,
        &[assignment_version_id],
        Some(1),
        Some(2),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(assignment_third_page[0].poams.len(), 1);
    assert!(!assignment_third_page[0].poams_has_more);
    let original_assignment_order = [
        assignments[0].poams[0].id,
        assignment_second_page[0].poams[0].id,
        assignment_third_page[0].poams[0].id,
    ];
    let legacy_assignments = poam_service::assignment_relationships(
        &pool,
        &actor,
        &[assignment_version_id],
        None,
        None,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        legacy_assignments[0]
            .poams
            .iter()
            .map(|poam| poam.id)
            .collect::<Vec<_>>(),
        original_assignment_order
    );
    assert!(!legacy_assignments[0].poams_has_more);
    assert_eq!(legacy_assignments[0].poams_next_offset, None);

    sqlx::query("UPDATE poams SET updated_at=updated_at+INTERVAL '200 years' WHERE id=$1")
        .bind(original_assignment_order[2])
        .execute(&pool)
        .await
        .unwrap();
    let mut assignment_order_after_update = Vec::new();
    for offset in 0..3 {
        let page = poam_service::assignment_relationships(
            &pool,
            &actor,
            &[assignment_version_id],
            Some(1),
            Some(offset),
            &clock,
        )
        .await
        .unwrap();
        assignment_order_after_update.push(page[0].poams[0].id);
    }
    assert_eq!(
        assignment_order_after_update, original_assignment_order,
        "mutable POA&M updates must not reorder assignment relationship pages"
    );
    assert_eq!(
        assignment_snapshot(&pool, assignment_version_id).await,
        assignment_before
    );
}

#[sqlx::test]
async fn legacy_relationship_requests_use_bounded_page_with_explicit_continuation(pool: PgPool) {
    let fixture = assessment_fixture(&pool).await;
    let actor = admin_actor(fixture.user_id);
    let clock = FixedClock(Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap());
    let mut initial = pool.begin().await.unwrap();
    persist_assessment(&mut initial, &fixture, EnforcementOutcome::Fail).await;
    initial.commit().await.unwrap();
    let (assignment_id, assignment_version_id, _) =
        immutable_assignment_fixture(&pool, fixture.system_id, fixture.user_id).await;

    for ordinal in 0..101 {
        if ordinal > 0 {
            let mut failing = pool.begin().await.unwrap();
            persist_assessment(&mut failing, &fixture, EnforcementOutcome::Fail).await;
            failing.commit().await.unwrap();
        }
        let created = create_service_poam(
            &pool,
            &fixture,
            &actor,
            &clock,
            &format!("Compatibility history {ordinal}"),
        )
        .await;
        sqlx::query(
            "INSERT INTO poam_assignment_references( \
               poam_id,assignment_id,assignment_version_id,added_by) VALUES($1,$2,$3,$4)",
        )
        .bind(created.poam.id)
        .bind(assignment_id)
        .bind(assignment_version_id)
        .bind(fixture.user_id)
        .execute(&pool)
        .await
        .unwrap();
        let awaiting = poam_service::transition(
            &pool,
            &actor,
            created.poam.id,
            TransitionPoamRequest {
                revision: created.poam.revision,
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
        poam_service::close(
            &pool,
            &actor,
            awaiting.poam.id,
            awaiting.poam.revision,
            &clock,
        )
        .await
        .unwrap();
    }

    let assessment_id = current_assessment_id(&pool, &fixture).await;
    let finding = finding_id(&pool, &fixture).await;
    let by_assessment =
        poam_service::finding_relationships(&pool, &actor, &[assessment_id], None, None, &clock)
            .await
            .unwrap();
    assert_eq!(by_assessment[0].historical_poams.len(), 100);
    assert!(by_assessment[0].historical_has_more);
    assert_eq!(by_assessment[0].historical_next_offset, Some(100));
    let by_finding = poam_service::finding_relationships_by_finding(
        &pool,
        &actor,
        &[finding],
        None,
        None,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(by_finding[0].historical_poams.len(), 100);
    assert!(by_finding[0].historical_has_more);
    assert_eq!(by_finding[0].historical_next_offset, Some(100));
    let assignments = poam_service::assignment_relationships(
        &pool,
        &actor,
        &[assignment_version_id],
        None,
        None,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(assignments[0].poams.len(), 100);
    assert!(assignments[0].poams_has_more);
    assert_eq!(assignments[0].poams_next_offset, Some(100));

    let final_finding_page = poam_service::finding_relationships_by_finding(
        &pool,
        &actor,
        &[finding],
        Some(100),
        Some(100),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(final_finding_page[0].historical_poams.len(), 1);
    assert!(!final_finding_page[0].historical_has_more);
    let final_assignment_page = poam_service::assignment_relationships(
        &pool,
        &actor,
        &[assignment_version_id],
        Some(100),
        Some(100),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(final_assignment_page[0].poams.len(), 1);
    assert!(!final_assignment_page[0].poams_has_more);
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
