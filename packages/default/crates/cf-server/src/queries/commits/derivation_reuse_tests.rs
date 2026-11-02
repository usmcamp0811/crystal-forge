//! Migration-backed AC8 contracts. Execution success here is an explicit unit
//! fixture; the owning Assurance scenario proves actual Nix execution and scans.

use crate::models::deployment_policies::PolicyCheckResult;
use crate::models::evaluate_with_policies::{
    SuccessfulSystemResult, SystemPersistenceOutcome, persist_evaluated_system,
};
use crate::queries::{build_jobs, configuration_assurance, evaluation_inventory};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const DRV: &str = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-reuse.drv";
const OUT: &str = "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-reuse";

async fn registration(pool: &PgPool) -> (i32, Uuid) {
    let flake=sqlx::query_scalar("INSERT INTO flakes(name,repo_url,branch,snapshot_ready_at) VALUES('reuse','https://example.invalid/reuse','main',now()) RETURNING id")
        .fetch_one(pool).await.unwrap();
    let system=sqlx::query_scalar("INSERT INTO systems(hostname,system_configuration_name,flake_id,management_mode,public_key,derivation,deployment_policy) VALUES('reuse-host','host',$1,'managed','fixture-key','','manual') RETURNING id")
        .bind(flake).fetch_one(pool).await.unwrap();
    (flake, system)
}

async fn evaluate(
    pool: &PgPool,
    flake: i32,
    sha: char,
    config: &str,
    allowed: bool,
) -> (
    i32,
    i32,
    SuccessfulSystemResult,
    PolicyCheckResult,
    SystemPersistenceOutcome,
) {
    let existing: Option<i32> =
        sqlx::query_scalar("SELECT id FROM commits WHERE flake_id=$1 AND git_commit_hash=$2")
            .bind(flake)
            .bind(sha.to_string().repeat(40))
            .fetch_optional(pool)
            .await
            .unwrap();
    let commit = if let Some(commit) = existing {
        commit
    } else {
        sqlx::query_scalar("INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES($1,$2,now()) RETURNING id")
            .bind(flake).bind(sha.to_string().repeat(40)).fetch_one(pool).await.unwrap()
    };
    let attempt = match super::mark_commit_evaluation_started(pool, commit)
        .await
        .unwrap()
    {
        super::EvalStartOutcome::Started { attempt } => attempt,
        _ => panic!("fixture must acquire native ownership"),
    };
    let names = vec![config.to_owned()];
    evaluation_inventory::persist_inventory(
        pool,
        commit,
        attempt,
        &names,
        &names,
        evaluation_inventory::InventoryExecutionMode::Native,
    )
    .await
    .unwrap();
    let result = SuccessfulSystemResult {
        system_name: config.into(),
        derivation_target: format!(
            "git+https://example.invalid/reuse?rev={}#nixosConfigurations.{config}",
            sha.to_string().repeat(40)
        ),
        drv_path: DRV.into(),
        expected_store_path: Some(OUT.into()),
        cf_agent_enabled: Some(true),
        build_eligible: true,
    };
    let mut check =
        PolicyCheckResult::from_assigned(config.into(), &json!({"cfAgentEnabled":true}), &[])
            .unwrap();
    if !allowed {
        check.meets_requirements = false;
        check
            .failed_policies
            .push(("fixture strict policy".into(), true));
    }
    let outcome = persist_evaluated_system(pool, commit, attempt, &result, &check, &[])
        .await
        .unwrap();
    (commit, attempt, result, check, outcome)
}

async fn finish_evaluation(pool: &PgPool, commit: i32) {
    sqlx::query("UPDATE evaluation_attempts SET status='complete',completed_at=now() WHERE commit_id=$1 AND status='in_progress'")
        .bind(commit).execute(pool).await.unwrap();
    sqlx::query("UPDATE commits SET evaluation_status='complete' WHERE id=$1")
        .bind(commit)
        .execute(pool)
        .await
        .unwrap();
}

async fn carrier(pool: &PgPool, commit: i32) -> i32 {
    sqlx::query_scalar("SELECT id FROM derivations WHERE commit_id=$1 AND derivation_type='nixos'")
        .bind(commit)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn admit(pool: &PgPool, derivation: i32) -> Option<build_jobs::BuildJobInsertOutcome> {
    let mut tx = pool.begin().await.unwrap();
    let work = build_jobs::create_build_job_for_derivation_tx(&mut tx, derivation)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    work
}

async fn successful_fixture(pool: &PgPool, derivation: i32) -> (Uuid, i32, Uuid) {
    let job = match admit(pool, derivation).await.unwrap() {
        build_jobs::BuildJobInsertOutcome::Inserted { build_job_id } => build_job_id,
        _ => panic!("fixture requires one real job identity"),
    };
    sqlx::query("UPDATE build_jobs SET status='success',completed_at=now() WHERE id=$1")
        .bind(job)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE derivations SET status_id=10,store_path=$2,completed_at=now(),build_preparation_state='queued' WHERE id=$1")
        .bind(derivation).bind(OUT).execute(pool).await.unwrap();
    let cache=sqlx::query_scalar("INSERT INTO cache_destinations(name,cache_type,enabled,push_to,niks3_public_keys,niks3_read_auth_mode,niks3_write_auth_mode,niks3_auth_token,niks3_server_url) VALUES('reuse-cache','Niks3',true,'https://cache.example.invalid',ARRAY['cache:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='],'none','token','fixture-write-token','https://cache.example.invalid') RETURNING id")
        .fetch_one(pool).await.unwrap();
    sqlx::query("INSERT INTO cache_push_jobs(derivation_id,store_path,status,completed_at,cache_destination_id,cache_destination_source,cache_destination) VALUES($1,$2,'completed',now(),$3,'database','reuse-cache')")
        .bind(derivation).bind(OUT).bind(cache).execute(pool).await.unwrap();
    sqlx::query("UPDATE cve_scans SET status='failed',completed_at=now() WHERE derivation_id=$1 AND status IN ('awaiting_build','awaiting_closure','pending')")
        .bind(derivation).execute(pool).await.unwrap();
    let scan=sqlx::query_scalar("INSERT INTO cve_scans(derivation_id,status,scanner_name) VALUES($1,'in_progress','vulnix') RETURNING id")
        .bind(derivation).fetch_one(pool).await.unwrap();
    sqlx::query("UPDATE cve_scans SET status='completed',completed_at=now(),evidence_schema_version=1 WHERE id=$1")
        .bind(scan).execute(pool).await.unwrap();
    (job, cache, scan)
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn exact_reuse_scan_coverage_counts_reused_head_by_execution_source(pool: PgPool) {
    use crate::queries::scanning::{get_scan_stats, get_scan_systems};
    let (flake, _) = registration(&pool).await;
    let (a, _, _, _, _) = evaluate(&pool, flake, 'a', "host", true).await;
    let origin = carrier(&pool, a).await;
    // A real build and publication of A, without any completed scan yet.
    let job = match admit(&pool, origin).await.unwrap() {
        build_jobs::BuildJobInsertOutcome::Inserted { build_job_id } => build_job_id,
        _ => panic!("fixture requires one real job identity"),
    };
    sqlx::query("UPDATE build_jobs SET status='success',completed_at=now() WHERE id=$1")
        .bind(job)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE derivations SET status_id=10,store_path=$2,completed_at=now(),build_preparation_state='queued' WHERE id=$1")
        .bind(origin).bind(OUT).execute(&pool).await.unwrap();
    let cache: i32 = sqlx::query_scalar("INSERT INTO cache_destinations(name,cache_type,enabled,push_to,niks3_public_keys,niks3_read_auth_mode,niks3_write_auth_mode,niks3_auth_token,niks3_server_url) VALUES('reuse-cache','Niks3',true,'https://cache.example.invalid',ARRAY['cache:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='],'none','token','fixture-write-token','https://cache.example.invalid') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO cache_push_jobs(derivation_id,store_path,status,completed_at,cache_destination_id,cache_destination_source,cache_destination) VALUES($1,$2,'completed',now(),$3,'database','reuse-cache')")
        .bind(origin).bind(OUT).bind(cache).execute(&pool).await.unwrap();
    sqlx::query("UPDATE cve_scans SET status='failed',completed_at=now() WHERE derivation_id=$1 AND status IN ('awaiting_build','awaiting_closure','pending')")
        .bind(origin).execute(&pool).await.unwrap();
    finish_evaluation(&pool, a).await;
    // B evaluates to the identical drv and adopts A's real job.
    let (b, _, _, _, _) = evaluate(&pool, flake, 'b', "host", true).await;
    let binding = carrier(&pool, b).await;
    let adopted: (Option<i32>, Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT reused_from_derivation_id,reused_build_job_id,store_path FROM derivations WHERE id=$1",
    )
    .bind(binding).fetch_one(&pool).await.unwrap();
    assert_eq!(
        adopted,
        (Some(origin), Some(job), None),
        "B is a carrier without its own build"
    );
    sqlx::query(
        "INSERT INTO flake_branch_commit_snapshot(flake_id,commit_id,position) VALUES($1,$2,0)",
    )
    .bind(flake)
    .bind(b)
    .execute(&pool)
    .await
    .unwrap();
    let external: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,system_configuration_name,flake_id,management_mode,derivation,deployment_policy)
         VALUES('reuse-external','host',$1,'external','','manual') RETURNING id",
    )
    .bind(flake).fetch_one(&pool).await.unwrap();
    let row = |rows: Vec<crate::queries::scanning::ScanSystemRow>| {
        rows.into_iter()
            .find(|r| r.system_id == external)
            .expect("external scan row")
    };

    let before = row(get_scan_systems(&pool, 100).await.unwrap());
    assert_eq!(
        before.total_configs, 2,
        "A and B remain distinct evaluated configurations"
    );
    assert_eq!(
        before.needs_build, 0,
        "reused carrier B is built through its execution source"
    );
    assert_eq!(before.unscanned, 2);
    assert_eq!(before.scanned, 0);
    // Hardening assesses the physical artifact that actually built.
    let hardening =
        crate::queries::hardening_scans::resolve_system_hardening_scan_target(&pool, external)
            .await
            .unwrap()
            .expect("reused head has a hardening target");
    assert_eq!(hardening.derivation_id, origin);
    assert_eq!(
        hardening.commit_hash,
        "a".repeat(40),
        "assessment uses the source that built A"
    );
    assert!(
        hardening.blocked_reason.is_none(),
        "reused head is not reported unbuilt"
    );

    let scan: Uuid = sqlx::query_scalar("INSERT INTO cve_scans(derivation_id,status,scanner_name) VALUES($1,'in_progress','vulnix') RETURNING id")
        .bind(origin).fetch_one(&pool).await.unwrap();
    sqlx::query("UPDATE cve_scans SET status='completed',completed_at=now(),evidence_schema_version=1 WHERE id=$1")
        .bind(scan).execute(&pool).await.unwrap();

    let stats = get_scan_stats(&pool).await.unwrap();
    assert_eq!(stats.never_scanned, 0);
    assert_eq!(
        stats.coverage_percent, 100,
        "the selected reused head's physical artifact is in the denominator and covered"
    );
    let after = row(get_scan_systems(&pool, 100).await.unwrap());
    assert_eq!(
        (after.scanned, after.unscanned, after.needs_build),
        (2, 0, 0)
    );
    assert!(
        after.current_scan_id.is_none(),
        "configuration coverage grants no Current authority"
    );

    // A scoped scan request names carrier B; admission reports B as the
    // configuration identity and A as the scanned execution source.
    use crate::queries::cve_scans::{SystemScanAdmission, admit_system_cve_scan};
    let actor: Uuid = sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email,user_type) VALUES('reuse-admin','Reuse','Admin','reuse-admin@example.invalid','human') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO user_role_assignments(user_id,role) VALUES($1,'admin')")
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        matches!(
            admit_system_cve_scan(&pool, actor, external, Some(origin), false, None, None)
                .await
                .unwrap(),
            SystemScanAdmission::TargetChanged
        ),
        "client intent is the selected carrier B, not execution source A"
    );
    let identity =
        match admit_system_cve_scan(&pool, actor, external, Some(binding), false, None, None)
            .await
            .unwrap()
        {
            SystemScanAdmission::Admitted {
                target,
                configuration_derivation_id,
                ..
            } => (target.derivation_id, configuration_derivation_id),
            SystemScanAdmission::Unavailable {
                derivation_id,
                configuration_derivation_id,
                ..
            } => (derivation_id, configuration_derivation_id),
            _ => panic!("reused head B must be admissible"),
        };
    assert_eq!(
        identity,
        (origin, binding),
        "response derivation_id is source A; configuration_derivation_id is B"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn exact_reuse_selected_b_keeps_real_a_execution_scan_and_completion(pool: PgPool) {
    let (flake, system) = registration(&pool).await;
    let (a, _, _, _, _) = evaluate(&pool, flake, 'a', "host", true).await;
    let origin = carrier(&pool, a).await;
    let (job, _, scan) = successful_fixture(&pool, origin).await;
    finish_evaluation(&pool, a).await;
    // A pre-upgrade pathless conflict is not backfilled from its expected path.
    // Only the following fresh Native producer establishes its actual drv.
    let legacy_commit:i32=sqlx::query_scalar("INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES($1,repeat('b',40),now()) RETURNING id")
        .bind(flake).fetch_one(&pool).await.unwrap();
    let legacy_id:i32=sqlx::query_scalar("INSERT INTO derivations(commit_id,derivation_name,derivation_type,expected_store_path,status_id,error_message,build_preparation_state) VALUES($1,'host','nixos',$2,5,'legacy path conflict','not_required') RETURNING id")
        .bind(legacy_commit).bind(OUT).fetch_one(&pool).await.unwrap();
    let (b, attempt, result, check, outcome) = evaluate(&pool, flake, 'b', "host", true).await;
    let binding = carrier(&pool, b).await;
    assert_eq!(
        binding, legacy_id,
        "fresh Native evaluation must preserve the old source ID"
    );
    assert!(
        matches!(outcome,SystemPersistenceOutcome::ExistingBuildJob {build_job_id,..} if build_job_id==job)
    );
    assert_ne!(binding, origin);
    let proof:(String,String,i32,Uuid,i32)=sqlx::query_as("SELECT derivation_path,expected_store_path,reused_from_derivation_id,reused_build_job_id,status_id FROM derivations WHERE id=$1")
        .bind(binding).fetch_one(&pool).await.unwrap();
    assert_eq!(proof, (DRV.into(), OUT.into(), origin, job, 5));
    let mut tx = pool.begin().await.unwrap();
    assert!(
        super::completion_committed_tx(&mut tx, b, attempt, &result, &check)
            .await
            .unwrap()
    );
    tx.rollback().await.unwrap();
    sqlx::query(
        "INSERT INTO flake_branch_commit_snapshot(flake_id,commit_id,position) VALUES($1,$2,0)",
    )
    .bind(flake)
    .bind(b)
    .execute(&pool)
    .await
    .unwrap();
    let evidence = configuration_assurance::fetch(&pool, system, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(evidence.selected_revision.unwrap().commit_id, b);
    let artifact = evidence.selected_artifact.unwrap();
    assert_eq!(artifact.derivation_id, binding);
    assert_eq!(artifact.execution_derivation_id, Some(origin));
    assert_eq!(artifact.execution_revision.unwrap().commit_id, a);
    assert_eq!(artifact.build.unwrap().job_id, job);
    assert_eq!(evidence.selected_scan.unwrap().scan_id, scan);
    assert_eq!(evidence.history.builds.len(), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM build_jobs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM evaluation_snapshots WHERE commit_id=$1 AND schema_version=3 AND integrity_version=3")
        .bind(b).fetch_one(&pool).await.unwrap(),1);
    // A real B certificate supplements A's scan provenance; it never relabels it.
    sqlx::query("INSERT INTO system_states(hostname,change_reason,store_path,generation,generation_matches_current_store_path,timestamp) VALUES('reuse-host','startup',$1,7,true,now())")
        .bind(OUT).execute(&pool).await.unwrap();
    let retained:Uuid=sqlx::query_scalar("INSERT INTO evaluation_generation_snapshots(system_id,generation,snapshot_id,derivation_id,commit_id,source_store_path,configuration_name,lineage_verified,binding_origin) SELECT $1,7,current_snapshot_id,$2,$3,$4,'host',true,'external_reconciled' FROM evaluation_snapshot_selections WHERE commit_id=$3 AND configuration_name='host' RETURNING id")
        .bind(system).bind(binding).bind(b).bind(OUT).fetch_one(&pool).await.unwrap();
    let current:(i32,i32,Uuid,Uuid)=sqlx::query_as("SELECT derivation_id,commit_id,scan_id,generation_snapshot_id FROM view_current_cve_authority WHERE system_id=$1")
        .bind(system).fetch_one(&pool).await.unwrap();
    assert_eq!(current, (origin, a, scan, retained));
    let inventory = crate::queries::cves::fetch_system_cve_inventory(&pool, system)
        .await
        .unwrap();
    assert_eq!(inventory.source.unwrap().scan_id, scan);
    assert_eq!(inventory.running_target.unwrap().derivation_id, origin);
    assert!(
        crate::queries::systems::resolve_observational_current_revision(&pool, system)
            .await
            .unwrap()
            .is_none(),
        "B's supplemental configuration certificate must not fabricate runtime Git B"
    );
    assert!(
        sqlx::query("UPDATE cve_scans SET derivation_id=$2 WHERE id=$1")
            .bind(scan)
            .bind(binding)
            .execute(&pool)
            .await
            .is_err()
    );
    // A different actual drv with the same output remains ambiguous.
    finish_evaluation(&pool, b).await;
    let c:i32=sqlx::query_scalar("INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES($1,repeat('c',40),now()) RETURNING id")
        .bind(flake).fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path,expected_store_path,status_id) VALUES($1,'nixos','host','/nix/store/cccccccccccccccccccccccccccccccc-other.drv',$2,5)")
        .bind(c).bind(OUT).execute(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM view_current_cve_authority WHERE system_id=$1"
        )
        .bind(system)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn exact_reuse_blocked_origin_admits_b_and_rejected_c_cannot_cancel_it(pool: PgPool) {
    let (flake, _) = registration(&pool).await;
    let (a, _, _, _, outcome) = evaluate(&pool, flake, 'a', "host", false).await;
    assert!(matches!(
        outcome,
        SystemPersistenceOutcome::RecordedWithoutBuild { .. }
    ));
    let origin = carrier(&pool, a).await;
    finish_evaluation(&pool, a).await;
    let (b, _, _, _, outcome) = evaluate(&pool, flake, 'b', "host", true).await;
    assert!(matches!(
        outcome,
        SystemPersistenceOutcome::NeedsBuildPreparation { .. }
    ));
    let binding = carrier(&pool, b).await;
    let job = match admit(&pool, binding).await.unwrap() {
        build_jobs::BuildJobInsertOutcome::Inserted { build_job_id } => build_job_id,
        _ => panic!("unbuilt A must not strand admitted B"),
    };
    finish_evaluation(&pool, b).await;
    let (c, _, _, _, outcome) = evaluate(&pool, flake, 'c', "host", false).await;
    assert!(matches!(
        outcome,
        SystemPersistenceOutcome::RecordedWithoutBuild { .. }
    ));
    assert!(admit(&pool, carrier(&pool, c).await).await.is_none());
    let actual: (i32, String, i32) = sqlx::query_as(
        "SELECT derivation_id,status,physical_derivation_id FROM build_jobs WHERE id=$1",
    )
    .bind(job)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(actual, (binding, "queued".into(), origin));
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT derivation_execution_source($1)")
            .bind(binding)
            .fetch_one(&pool)
            .await
            .unwrap(),
        binding,
        "B owns its real execution even though A was the first evaluator"
    );
    assert!(
        !sqlx::query_scalar::<_, bool>(
            "SELECT policy_requirements_met FROM derivations WHERE id=$1"
        )
        .bind(origin)
        .fetch_one(&pool)
        .await
        .unwrap()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn exact_reuse_inflight_work_keeps_session_and_failed_a_allows_new_b_work(pool: PgPool) {
    let (flake, _) = registration(&pool).await;
    let (a, _, _, _, _) = evaluate(&pool, flake, 'a', "host", true).await;
    let origin = carrier(&pool, a).await;
    let job = match admit(&pool, origin).await.unwrap() {
        build_jobs::BuildJobInsertOutcome::Inserted { build_job_id } => build_job_id,
        _ => panic!("initial work"),
    };
    let builder = Uuid::new_v4();
    let session = Uuid::new_v4();
    sqlx::query("INSERT INTO builders(id,name,public_key,arch,status,current_session_id) VALUES($1,'reuse-builder','fixture-key','x86_64-linux','active',$2)")
        .bind(builder).bind(session).execute(&pool).await.unwrap();
    sqlx::query(
        "UPDATE build_jobs SET status='building',builder_id=$2,builder_session_id=$3 WHERE id=$1",
    )
    .bind(job)
    .bind(builder)
    .bind(session)
    .execute(&pool)
    .await
    .unwrap();
    finish_evaluation(&pool, a).await;
    let (b, _, _, _, outcome) = evaluate(&pool, flake, 'b', "host", true).await;
    let binding = carrier(&pool, b).await;
    assert!(
        matches!(outcome,SystemPersistenceOutcome::ExistingBuildJob {build_job_id,..} if build_job_id==job)
    );
    let authorized = crate::queries::builders::get_authorized_source_archive_job(
        &pool, &job, &builder, &session,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(authorized.derivation_id, origin);
    let replacement_session = Uuid::new_v4();
    sqlx::query("UPDATE builders SET current_session_id=$2 WHERE id=$1")
        .bind(builder)
        .bind(replacement_session)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        crate::queries::builders::get_authorized_source_archive_job(
            &pool, &job, &builder, &session,
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        crate::queries::builders::get_authorized_source_archive_job(
            &pool,
            &job,
            &builder,
            &replacement_session
        )
        .await
        .unwrap()
        .is_none()
    );
    let actual: (i32, Uuid) =
        sqlx::query_as("SELECT derivation_id,builder_session_id FROM build_jobs WHERE id=$1")
            .bind(job)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(actual, (origin, session));
    assert!(
        sqlx::query(
            "INSERT INTO build_jobs(derivation_id,status,queue_position) VALUES($1,'queued',100)"
        )
        .bind(binding)
        .execute(&pool)
        .await
        .is_err()
    );
    sqlx::query("UPDATE build_jobs SET status='failed',completed_at=now() WHERE id=$1")
        .bind(job)
        .execute(&pool)
        .await
        .unwrap();
    let mut terminal = pool.begin().await.unwrap();
    crate::queries::cve_scan_leases::fail_post_build_scan_for_terminal_build_tx(
        &mut terminal,
        job,
        Some("fixture producing execution failed"),
    )
    .await
    .unwrap();
    terminal.commit().await.unwrap();
    assert!(matches!(
        admit(&pool, binding).await,
        Some(build_jobs::BuildJobInsertOutcome::Inserted { .. })
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT derivation_id FROM build_jobs WHERE status='queued'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        binding
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn exact_reuse_failed_reevaluation_preserves_bound_identity_and_a_work(pool: PgPool) {
    let (flake, system) = registration(&pool).await;
    let (a, _, _, _, _) = evaluate(&pool, flake, 'a', "host", true).await;
    let origin = carrier(&pool, a).await;
    let job = match admit(&pool, origin).await.unwrap() {
        build_jobs::BuildJobInsertOutcome::Inserted { build_job_id } => build_job_id,
        _ => panic!("initial real queue record"),
    };
    finish_evaluation(&pool, a).await;
    let (b, _, result, _, _) = evaluate(&pool, flake, 'b', "host", true).await;
    let binding = carrier(&pool, b).await;
    finish_evaluation(&pool, b).await;
    super::reset_commit_evaluation(&pool, b).await.unwrap();
    let attempt = match super::mark_commit_evaluation_started(&pool, b)
        .await
        .unwrap()
    {
        super::EvalStartOutcome::Started { attempt } => attempt,
        _ => panic!("fresh failure owner"),
    };
    let names = vec!["host".to_owned()];
    evaluation_inventory::persist_inventory(
        &pool,
        b,
        attempt,
        &names,
        &names,
        evaluation_inventory::InventoryExecutionMode::Native,
    )
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    crate::queries::evaluation_native_outcomes::persist_native_outcome_tx(
        &mut tx,
        b,
        attempt,
        "host",
        crate::queries::evaluation_native_outcomes::NativeEvalState::EvalFailed,
        None,
        Some(false),
        &json!({"evaluation_attempt":attempt,"evaluation_failure":true}),
        Some("fresh native evaluation failed"),
    )
    .await
    .unwrap();
    crate::queries::derivations::record_synthetic_eval_failure_in_tx(
        &mut tx,
        Some(b),
        "host",
        "nixos",
        Some(&result.derivation_target),
        "fresh native evaluation failed",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let preserved: (String, String, i32, Uuid, bool) = sqlx::query_as("SELECT derivation_path,expected_store_path,reused_from_derivation_id,reused_build_job_id,policy_requirements_met FROM derivations WHERE id=$1")
        .bind(binding).fetch_one(&pool).await.unwrap();
    assert_eq!(preserved, (DRV.into(), OUT.into(), origin, job, false));
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM build_jobs WHERE id=$1")
            .bind(job)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "queued"
    );
    assert!(admit(&pool, binding).await.is_none());
    let evidence = configuration_assurance::fetch(&pool, system, None)
        .await
        .unwrap()
        .unwrap();
    assert!(
        evidence.history.evaluations.iter().any(|event| {
            event.revision.commit_id == b && event.evaluation.state == "eval_failed"
        })
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn exact_reuse_queued_source_revocation_does_not_strand_authorized_b(pool: PgPool) {
    let (flake, system) = registration(&pool).await;
    let (a, _, _, _, _) = evaluate(&pool, flake, 'a', "host", true).await;
    let origin = carrier(&pool, a).await;
    let old_job = match admit(&pool, origin).await.unwrap() {
        build_jobs::BuildJobInsertOutcome::Inserted { build_job_id } => build_job_id,
        _ => panic!("initial source queue record"),
    };
    finish_evaluation(&pool, a).await;
    // This fixture represents A's own saved admission revocation. B's policy
    // is not the reason A's queued job is unclaimable, and A never started.
    sqlx::query("UPDATE derivations SET policy_requirements_met=false,build_preparation_state='not_required' WHERE id=$1")
        .bind(origin).execute(&pool).await.unwrap();
    let environment: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES('authorized-b-context') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(system)
        .bind(environment)
        .execute(&pool)
        .await
        .unwrap();
    let (b, _, _, _, outcome) = evaluate(&pool, flake, 'b', "host", true).await;
    assert!(matches!(
        outcome,
        SystemPersistenceOutcome::NeedsBuildPreparation { .. }
    ));
    let binding = carrier(&pool, b).await;
    let job = match admit(&pool, binding).await.unwrap() {
        build_jobs::BuildJobInsertOutcome::Inserted { build_job_id } => build_job_id,
        _ => panic!("B needs one authorized execution, not a policy-rejected A job"),
    };
    let old: (String, i32, Option<Uuid>) =
        sqlx::query_as("SELECT status,derivation_id,environment_id FROM build_jobs WHERE id=$1")
            .bind(old_job)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(old, ("cancelled".into(), origin, None));
    let actual: (i32, Uuid, i32) = sqlx::query_as(
        "SELECT derivation_id,environment_id,physical_derivation_id FROM build_jobs WHERE id=$1",
    )
    .bind(job)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(actual, (binding, environment, origin));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM build_jobs WHERE status IN ('queued','building','cancelling')",
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn exact_reuse_prior_publication_survives_a_later_failed_attempt(pool: PgPool) {
    let (flake, system) = registration(&pool).await;
    let (a, _, _, _, _) = evaluate(&pool, flake, 'a', "host", true).await;
    let origin = carrier(&pool, a).await;
    let (job, _, _) = successful_fixture(&pool, origin).await;
    sqlx::query("INSERT INTO build_jobs(derivation_id,status,attempt_number,parent_job_id,root_job_id,completed_at) VALUES($1,'failed',2,$2,$2,now())")
        .bind(origin).bind(job).execute(&pool).await.unwrap();
    sqlx::query("UPDATE derivations SET status_id=12,error_message='later fixture attempt failed' WHERE id=$1")
        .bind(origin).execute(&pool).await.unwrap();
    finish_evaluation(&pool, a).await;
    sqlx::query("UPDATE systems SET management_mode='reference',public_key=NULL WHERE id=$1")
        .bind(system)
        .execute(&pool)
        .await
        .unwrap();
    let (b, _, _, _, outcome) = evaluate(&pool, flake, 'b', "host", true).await;
    assert!(
        matches!(outcome,SystemPersistenceOutcome::ExistingBuildJob {build_job_id,..} if build_job_id==job)
    );
    let binding = carrier(&pool, b).await;
    let state: (i32, i32) = sqlx::query_as(
        "SELECT status_id,execution_status_id FROM view_derivation_execution_evidence WHERE id=$1",
    )
    .bind(binding)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, (5, 10));
    sqlx::query(
        "INSERT INTO flake_branch_commit_snapshot(flake_id,commit_id,position) VALUES($1,$2,0)",
    )
    .bind(flake)
    .bind(b)
    .execute(&pool)
    .await
    .unwrap();
    let target = crate::queries::cve_scans::resolve_system_cve_scan_target(&pool, system)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(target.derivation_id, binding);
    assert!(target.blocked_reason.is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM build_jobs WHERE derivation_id=$1")
            .bind(binding)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn exact_reuse_mismatch_scope_cache_and_gc_fail_closed(pool: PgPool) {
    let (flake, system) = registration(&pool).await;
    let (a, _, _, _, _) = evaluate(&pool, flake, 'a', "host", true).await;
    let origin = carrier(&pool, a).await;
    let (_, cache, _) = successful_fixture(&pool, origin).await;
    finish_evaluation(&pool, a).await;
    let (b, _, _, _, _) = evaluate(&pool, flake, 'b', "host", true).await;
    let binding = carrier(&pool, b).await;
    assert!(
        sqlx::query("UPDATE derivations SET expected_store_path='/nix/store/mismatch' WHERE id=$1")
            .bind(binding)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE derivations SET derivation_path='/nix/store/changed.drv' WHERE id=$1")
            .bind(origin)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM derivations WHERE id=$1")
            .bind(origin)
            .execute(&pool)
            .await
            .is_err()
    );
    let environment: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES('private-origin-cache') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO cache_destination_environments(cache_destination_id,environment_id) VALUES($1,$2)")
        .bind(cache).bind(environment).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let capabilities = cf_protocol::agent::AgentCapabilities {
        supports_niks3: true,
        ..Default::default()
    };
    assert!(
        crate::queries::cache_publication_reads::resolve_publication_read_tx(
            &mut tx,
            binding,
            OUT,
            None,
            true,
            capabilities
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        build_jobs::reusable_physical_build_tx(&mut tx, binding)
            .await
            .unwrap()
            .is_none()
    );
    tx.rollback().await.unwrap();
    let inaccessible: Uuid = Uuid::new_v4();
    assert!(
        configuration_assurance::fetch(&pool, system, Some(inaccessible))
            .await
            .unwrap()
            .is_none()
    );
    finish_evaluation(&pool, b).await;
    // Identical bytes in another configuration do not reveal host's job/scan.
    let (c, _, _, _, outcome) = evaluate(&pool, flake, 'c', "other-configuration", true).await;
    assert!(matches!(
        outcome,
        SystemPersistenceOutcome::NeedsBuildPreparation { .. }
    ));
    let other = carrier(&pool, c).await;
    let mut tx = pool.begin().await.unwrap();
    assert!(
        build_jobs::reusable_physical_build_tx(&mut tx, other)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT derivation_execution_source($1)")
            .bind(other)
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        other
    );
    tx.rollback().await.unwrap();

    finish_evaluation(&pool, c).await;
    let consumer_environment: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES('reuse-consumer-env') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let other_flake: i32 = sqlx::query_scalar("INSERT INTO flakes(name,repo_url,branch,snapshot_ready_at) VALUES('isolated-reuse','https://example.invalid/isolated-reuse','main',now()) RETURNING id")
        .fetch_one(&pool).await.unwrap();
    let consumer: Uuid = sqlx::query_scalar("INSERT INTO systems(hostname,system_configuration_name,flake_id,environment_id,management_mode,public_key,derivation,deployment_policy) VALUES('isolated-reuse-host','host',$1,$2,'reference',NULL,'','manual') RETURNING id")
        .bind(other_flake).bind(consumer_environment).fetch_one(&pool).await.unwrap();
    let viewer: Uuid = sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email) VALUES('reuse-viewer','Reuse','Viewer','reuse-viewer@example.invalid') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(viewer)
        .bind(consumer_environment)
        .execute(&pool)
        .await
        .unwrap();
    let (d, _, _, _, _) = evaluate(&pool, other_flake, 'd', "host", true).await;
    let isolated = carrier(&pool, d).await;
    sqlx::query(
        "INSERT INTO flake_branch_commit_snapshot(flake_id,commit_id,position) VALUES($1,$2,0)",
    )
    .bind(other_flake)
    .bind(d)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        configuration_assurance::fetch(&pool, system, Some(viewer))
            .await
            .unwrap()
            .is_none()
    );
    let private_origin = configuration_assurance::fetch(&pool, consumer, Some(viewer))
        .await
        .unwrap()
        .unwrap();
    let artifact = private_origin.selected_artifact.unwrap();
    assert_eq!(artifact.derivation_id, isolated);
    assert_eq!(artifact.derivation_path.as_deref(), Some(DRV));
    assert_eq!(artifact.physical_derivation_id, None);
    assert_eq!(artifact.execution_derivation_id, None);
    assert!(artifact.execution_revision.is_none());
    assert!(artifact.build.is_none());
    assert!(private_origin.selected_scan.is_none());
    assert!(private_origin.history.builds.is_empty());
    assert!(private_origin.history.scans.is_empty());
}
