use crystal_forge::models::poam::PoamListQuery;
use crystal_forge::queries::acceptance_register::{AcceptanceListQuery, AcceptanceSource};
use crystal_forge::services::poam::{PoamActor, SystemClock};
use crystal_forge::services::register_export_selection::{self, RegisterExportSelectionError};
use sqlx::PgPool;
use uuid::Uuid;

async fn reader(pool: &PgPool) -> (PoamActor, Uuid, Uuid) {
    let user: Uuid =
        sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email) VALUES($1,'Export','Reader',$2) RETURNING id")
            .bind(format!("export-{}", Uuid::new_v4()))
            .bind(format!("export-{}@example.invalid", Uuid::new_v4()))
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO user_role_assignments(user_id,role) VALUES($1,'viewer')")
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    let environment: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("v-{}", Uuid::new_v4()))
            .fetch_one(pool)
            .await
            .unwrap();
    let hidden: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(format!("h-{}", Uuid::new_v4()))
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(user)
        .bind(environment)
        .execute(pool)
        .await
        .unwrap();
    (
        PoamActor {
            user_id: user,
            identifier: "export reader".into(),
            is_admin: true, // A stale request claim must not expand the snapshot scope.
            can_mutate: true,
            environment_ids: vec![hidden],
            request_origin: None,
        },
        environment,
        hidden,
    )
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn pages_whole_filtered_scope_and_rechecks_reader(pool: PgPool) {
    let (actor, visible, hidden) = reader(&pool).await;
    let cve = "CVE-2099-12345";
    sqlx::query("INSERT INTO cves(id) VALUES($1)")
        .bind(cve)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) SELECT $1, 'pkg-' || n, $2, 'accepted', 'review', $3, now() FROM generate_series(1, 102) n")
        .bind(cve).bind(visible).bind(actor.user_id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) VALUES($1,'hidden',$2,'accepted','review',$3,now())")
        .bind(cve).bind(hidden).bind(actor.user_id).execute(&pool).await.unwrap();

    let query = AcceptanceListQuery {
        source: Some(AcceptanceSource::CveEnvironment),
        environment_id: Some(visible),
        limit: Some(1),
        offset: Some(101),
        ..Default::default()
    };
    let result = register_export_selection::select(
        &pool,
        &actor,
        &PoamListQuery::default(),
        &query,
        &SystemClock,
    )
    .await
    .unwrap();
    assert_eq!(result.actor_id, actor.user_id);
    assert!(!result.is_admin);
    assert_eq!(result.environment_ids, vec![visible]);
    assert!(result.poams.is_empty());
    assert_eq!(result.acceptances.len(), 102);
    assert_eq!(result.acceptance_context.len(), 102);
    assert!(
        result
            .acceptance_context
            .iter()
            .all(|context| { !context.scope_name.is_empty() && context.finding_id.is_none() })
    );
    assert!(result.acceptances.iter().all(|entry| {
        entry.source == AcceptanceSource::CveEnvironment && entry.environment_id == Some(visible)
    }));

    sqlx::query("UPDATE users SET is_active=false WHERE id=$1")
        .bind(actor.user_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        register_export_selection::select(
            &pool,
            &actor,
            &PoamListQuery::default(),
            &query,
            &SystemClock,
        )
        .await,
        Err(RegisterExportSelectionError::Forbidden)
    ));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn acceptance_only_does_not_apply_unrelated_poam_filters(pool: PgPool) {
    let (actor, visible, _) = reader(&pool).await;
    sqlx::query("INSERT INTO cves(id) VALUES('CVE-2099-12345')")
        .execute(&pool)
        .await
        .unwrap();
    let decision: Uuid = sqlx::query_scalar("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) VALUES('CVE-2099-12345','pkg',$1,'accepted','review',$2,now()) RETURNING id")
        .bind(visible).bind(actor.user_id).fetch_one(&pool).await.unwrap();
    let output = register_export_selection::select_acceptances(
        &pool,
        &actor,
        &AcceptanceListQuery {
            source: Some(AcceptanceSource::CveEnvironment),
            limit: Some(1),
            offset: Some(99),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(output.poams.is_empty());
    assert_eq!(output.acceptances.len(), 1);
    assert_eq!(output.acceptances[0].source_id, decision);
    assert_eq!(output.acceptance_context[0].source_id, decision);
    assert!(output.acceptance_context[0].scope_name.starts_with("v-"));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn policy_export_context_uses_actual_waiver_finding(pool: PgPool) {
    let (actor, visible, _) = reader(&pool).await;
    sqlx::query("UPDATE user_role_assignments SET role='admin' WHERE user_id=$1")
        .bind(actor.user_id)
        .execute(&pool)
        .await
        .unwrap();
    let system: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,public_key,derivation,environment_id) VALUES($1,'key','key',$2) RETURNING id",
    ).bind(format!("export-policy-host-{}", Uuid::new_v4()))
        .bind(visible).fetch_one(&pool).await.unwrap();
    let policy: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies(name,policy_type,config,enabled) VALUES($1,'custom_check','{}',false) RETURNING id",
    ).bind(format!("Export policy {}", Uuid::new_v4()))
        .fetch_one(&pool).await.unwrap();
    let finding: Uuid = sqlx::query_scalar(
        "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
    )
    .bind(system)
    .bind(policy)
    .fetch_one(&pool)
    .await
    .unwrap();
    let waiver: Uuid = sqlx::query_scalar(
        "INSERT INTO finding_waivers(finding_id,justification,policy_version_id,observation_token,observation_snapshot,created_by) VALUES($1,'policy justification',$2,'observed','{}',$3) RETURNING id",
    ).bind(finding).bind(Uuid::new_v4()).bind(actor.user_id)
        .fetch_one(&pool).await.unwrap();
    let result = register_export_selection::select_acceptances(
        &pool,
        &actor,
        &AcceptanceListQuery {
            source: Some(AcceptanceSource::PolicyWaiver),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(result.acceptances.len(), 1);
    assert_eq!(result.acceptances[0].source_id, waiver);
    assert_eq!(result.acceptance_context[0].finding_id, Some(finding));
    assert!(
        result.acceptance_context[0]
            .policy_name
            .as_deref()
            .unwrap()
            .starts_with("Export policy")
    );
    assert!(
        result.acceptance_context[0]
            .scope_name
            .starts_with("export-policy-host-")
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn exceeds_combined_cap_without_partial_selection(pool: PgPool) {
    let (actor, visible, _) = reader(&pool).await;
    sqlx::query("INSERT INTO cves(id) VALUES('CVE-2099-12345')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) SELECT 'CVE-2099-12345', 'pkg-' || n, $1, 'accepted', 'review', $2, now() FROM generate_series(1, 1001) n")
        .bind(visible).bind(actor.user_id).execute(&pool).await.unwrap();
    let result = register_export_selection::select(
        &pool,
        &actor,
        &PoamListQuery::default(),
        &AcceptanceListQuery::default(),
        &SystemClock,
    )
    .await;
    assert!(matches!(
        result,
        Err(RegisterExportSelectionError::TooManyRows)
    ));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn plan_context_retains_all_linked_systems_in_one_snapshot(pool: PgPool) {
    let (actor, visible, hidden) = reader(&pool).await;
    let policy: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies(name,policy_type,config,enabled) VALUES($1,'custom_check','{}',false) RETURNING id",
    ).bind(format!("Export scope {}", Uuid::new_v4())).fetch_one(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let plan: Uuid = sqlx::query_scalar(
        "INSERT INTO poams(title,risk,created_by) VALUES('Scoped export','high',$1) RETURNING id",
    )
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    for environment in [visible, hidden] {
        let host: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname,public_key,derivation,environment_id) VALUES($1,'key','key',$2) RETURNING id",
        ).bind(format!("export-{}", Uuid::new_v4())).bind(environment)
            .fetch_one(&mut *tx).await.unwrap();
        let finding: Uuid = sqlx::query_scalar(
            "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
        )
        .bind(host)
        .bind(policy)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)",
        )
        .bind(plan)
        .bind(finding)
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    // Admin sees every source-owned link. A viewer cannot export a plan that
    // combines a visible member with a hidden member.
    sqlx::query("UPDATE user_role_assignments SET role='admin' WHERE user_id=$1")
        .bind(actor.user_id)
        .execute(&pool)
        .await
        .unwrap();
    let selected = register_export_selection::select(
        &pool,
        &actor,
        &PoamListQuery::default(),
        &AcceptanceListQuery::default(),
        &SystemClock,
    )
    .await
    .unwrap();
    let plan_context = selected
        .poam_context
        .iter()
        .find(|row| row.poam_id == plan)
        .unwrap();
    assert_eq!(plan_context.system_ids.len(), 2);
    assert!(plan_context.cve_ids.is_empty());
    sqlx::query("UPDATE user_role_assignments SET role='viewer' WHERE user_id=$1")
        .bind(actor.user_id)
        .execute(&pool)
        .await
        .unwrap();
    let viewer = register_export_selection::select(
        &pool,
        &actor,
        &PoamListQuery::default(),
        &AcceptanceListQuery::default(),
        &SystemClock,
    )
    .await
    .unwrap();
    assert!(!viewer.poams.iter().any(|row| row.summary.id == plan));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn retired_link_in_same_environment_exports_but_moved_out_link_does_not(pool: PgPool) {
    let (actor, visible, hidden) = reader(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    let policy: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies(name,policy_type,config,enabled) VALUES($1,'custom_check','{}',false) RETURNING id",
    )
    .bind(format!("Export history {}", Uuid::new_v4()))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let plan: Uuid = sqlx::query_scalar(
        "INSERT INTO poams(title,risk,created_by) VALUES('Historical export','high',$1) RETURNING id",
    )
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let mut hosts = Vec::new();
    for retired in [false, true] {
        let host: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname,public_key,derivation,environment_id) VALUES($1,'key','key',$2) RETURNING id",
        )
        .bind(format!("export-history-{}", Uuid::new_v4()))
        .bind(visible)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let finding: Uuid = sqlx::query_scalar(
            "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
        )
        .bind(host)
        .bind(policy)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)",
        )
        .bind(plan)
        .bind(finding)
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        if retired {
            sqlx::query("UPDATE poam_finding_links SET retired_at=now(),retired_by=$2,retirement_reason='superseded' WHERE finding_id=$1")
                .bind(finding)
                .bind(actor.user_id)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        hosts.push(host);
    }
    tx.commit().await.unwrap();
    let selected = register_export_selection::select(
        &pool,
        &actor,
        &PoamListQuery::default(),
        &AcceptanceListQuery::default(),
        &SystemClock,
    )
    .await
    .unwrap();
    let row = selected
        .poams
        .iter()
        .find(|row| row.summary.id == plan)
        .unwrap();
    let context = selected
        .poam_context
        .iter()
        .find(|context| context.poam_id == plan)
        .unwrap();
    assert_eq!(context.system_ids.len(), 2);
    assert_eq!(context.visibility_system_ids, vec![hosts[0]]);
    assert!(hosts.iter().all(|id| row.system_ids.contains(id)));
    assert_eq!(context.links.len(), 2);
    crystal_forge::services::register_mixed_tabular::write_authorized(&selected).unwrap();
    crystal_forge::services::register_oscal_snapshot::write_authorized(&selected).unwrap();

    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(hosts[1])
        .bind(hidden)
        .execute(&pool)
        .await
        .unwrap();
    // The plan remains list-visible through its active A link. Its retired
    // B link still exists, but the reader cannot export the complete history.
    assert!(matches!(
        register_export_selection::select(
            &pool,
            &actor,
            &PoamListQuery::default(),
            &AcceptanceListQuery::default(),
            &SystemClock,
        )
        .await,
        Err(RegisterExportSelectionError::PartialContext)
    ));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn scheduled_environment_keeps_both_exact_packages(pool: PgPool) {
    let (actor, visible, _) = reader(&pool).await;
    sqlx::query("INSERT INTO cves(id) VALUES('CVE-2099-12345')")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let plan: Uuid = sqlx::query_scalar("INSERT INTO poams(title,risk,created_by) VALUES('Future remediation','high',$1) RETURNING id")
        .bind(actor.user_id).fetch_one(&mut *tx).await.unwrap();
    // A new open plan needs an active finding. A zero-link environment plan
    // requires a real prior CVE scan and an environment_moved retired link.
    let host: Uuid = sqlx::query_scalar("INSERT INTO systems(hostname,public_key,derivation,environment_id) VALUES($1,'key','key',$2) RETURNING id")
        .bind(format!("scheduled-export-{}", Uuid::new_v4())).bind(visible)
        .fetch_one(&mut *tx).await.unwrap();
    let policy: Uuid = sqlx::query_scalar("INSERT INTO deployment_policies(name,policy_type,config,enabled) VALUES($1,'custom_check','{}',false) RETURNING id")
        .bind(format!("scheduled-export-{}", Uuid::new_v4())).fetch_one(&mut *tx).await.unwrap();
    let finding: Uuid = sqlx::query_scalar(
        "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
    )
    .bind(host)
    .bind(policy)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)")
        .bind(plan)
        .bind(finding)
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    for package in ["openssl", "libssl"] {
        sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,poam_id,scheduled_by,scheduled_at) VALUES('CVE-2099-12345',$1,$2,'scheduled',$3,$4,now())")
            .bind(package).bind(visible).bind(plan).bind(actor.user_id)
            .execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    let selected = register_export_selection::select(
        &pool,
        &actor,
        &PoamListQuery::default(),
        &AcceptanceListQuery::default(),
        &SystemClock,
    )
    .await
    .unwrap();
    let context = selected
        .poam_context
        .iter()
        .find(|row| row.poam_id == plan)
        .unwrap();
    assert!(
        context
            .links
            .iter()
            .all(|link| link.canonical_cve_id.is_none())
    );
    assert_eq!(context.environment_ids, vec![visible]);
    assert_eq!(context.cve_ids, ["CVE-2099-12345"]);
    assert_eq!(context.scheduled_cve_tuples.len(), 2);
    assert_eq!(
        context.scheduled_cve_tuples[0].canonical_package_name,
        "libssl"
    );
    assert_eq!(
        context.scheduled_cve_tuples[1].canonical_package_name,
        "openssl"
    );
    let csv = crystal_forge::services::register_mixed_tabular::write_authorized(&selected)
        .unwrap()
        .csv
        .bytes;
    let rows: Vec<_> = csv::Reader::from_reader(csv.as_slice())
        .records()
        .map(Result::unwrap)
        .collect();
    let plan_rows: Vec<_> = rows
        .iter()
        .filter(|row| row.get(1) == Some(&plan.to_string()))
        .collect();
    assert_eq!(plan_rows.len(), 3);
    for package in ["libssl", "openssl"] {
        assert!(plan_rows.iter().any(|row| {
            row.get(14)
                .unwrap()
                .contains(&format!("CVE-2099-12345 / {package} (scheduled"))
                && row.get(12) == Some("")
                && row.get(13) == Some("")
        }));
    }
    let output =
        crystal_forge::services::register_oscal_snapshot::write_authorized(&selected).unwrap();
    let json: serde_json::Value = serde_json::from_str(&output.json).unwrap();
    let props = json["plan-of-action-and-milestones"]["poam-items"][0]["props"]
        .as_array()
        .unwrap();
    let tuples: Vec<serde_json::Value> = props
        .iter()
        .filter(|p| p["name"] == "scheduled-environment-cve-tuple")
        .map(|p| serde_json::from_str(p["value"].as_str().unwrap()).unwrap())
        .collect();
    assert_eq!(tuples.len(), 2);
    assert!(
        tuples
            .iter()
            .any(|t| t["canonical_package_name"] == "openssl")
    );
    assert!(
        tuples
            .iter()
            .any(|t| t["canonical_package_name"] == "libssl")
    );
    assert!(!props.iter().any(|p| p["name"] == "cve-finding-link"));
    assert!(
        json["plan-of-action-and-milestones"]
            .get("observations")
            .is_none()
    );
}
