// Compile the new service independently until the route owner registers its
// module in services/mod.rs. Its dependencies remain the production modules.
pub use crystal_forge::{models, services};
#[path = "../src/services/register_poam_tabular.rs"]
mod register_poam_tabular;

use crystal_forge::models::poam::PoamListQuery;
use crystal_forge::services::poam::{PoamActor, SystemClock};
use register_poam_tabular::{ExportError, export};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

async fn actor(pool: &PgPool) -> PoamActor {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users(username,first_name,last_name,email) VALUES($1,'Export','Admin',$2) RETURNING id",
    )
    .bind(format!("export-{}", Uuid::new_v4()))
    .bind(format!("export-{}@example.invalid", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO user_role_assignments(user_id,role) VALUES($1,'admin')")
        .bind(user_id)
        .execute(pool)
        .await
        .unwrap();
    PoamActor {
        user_id,
        identifier: "Export Admin".into(),
        is_admin: false,
        can_mutate: false,
        environment_ids: Vec::new(),
        request_origin: None,
    }
}

async fn system(pool: &PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO systems(hostname,public_key,derivation) VALUES($1,'key','key') RETURNING id",
    )
    .bind(format!("export-host-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn policy(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO deployment_policies(name,policy_type,config,enabled) VALUES($1,'custom_check','{}',false) RETURNING id")
        .bind(format!("Export policy {}", Uuid::new_v4()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn plan(pool: &PgPool, actor: &PoamActor, systems: &[Uuid], title: &str) -> Uuid {
    let mut tx = pool.begin().await.unwrap();
    let poam: Uuid = sqlx::query_scalar("INSERT INTO poams(title,target_date,risk,created_by) VALUES($1,'2099-01-01','high',$2) RETURNING id")
        .bind(title).bind(actor.user_id).fetch_one(&mut *tx).await.unwrap();
    let policy_id = policy(pool).await;
    for system_id in systems {
        let finding: Uuid = sqlx::query_scalar(
            "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
        )
        .bind(system_id)
        .bind(policy_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)",
        )
        .bind(poam)
        .bind(finding)
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    poam
}

async fn assignment_reference(
    pool: &PgPool,
    actor: &PoamActor,
    poam_id: Uuid,
    system_id: Option<Uuid>,
    environment_id: Option<Uuid>,
) {
    let bundle: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundles(name,framework,version,description,layer,owner) VALUES($1,'NIST','1.0','Export fixture','fleet','Security') RETURNING id",
    )
    .bind(format!("export-bundle-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .unwrap();
    let version: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_versions(bundle_id,version,publication_state,name,framework,framework_version,description,layer,owner,semantic_digest,trust_state) VALUES($1,'1.0','draft','Export fixture','NIST','1.0','Export fixture','fleet','Security','export-v1','trusted') RETURNING id",
    )
    .bind(bundle)
    .fetch_one(pool)
    .await
    .unwrap();
    let assignment: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_assignments(bundle_id,bundle_version_id,system_id,environment_id,scope_type,active,enforcement_mode,assignment_overlay_digest,created_by) VALUES($1,$2,$3,$4,$5,false,'report_only','export-v1',$6) RETURNING id",
    )
    .bind(bundle)
    .bind(version)
    .bind(system_id)
    .bind(environment_id)
    .bind(if system_id.is_some() { "system" } else { "environment" })
    .bind(actor.user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let snapshot: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_assignment_versions(assignment_id,version_number,bundle_version_id,enforcement_mode,assignment_overlay_digest,created_by) VALUES($1,1,$2,'report_only','export-v1',$3) RETURNING id",
    )
    .bind(assignment)
    .bind(version)
    .bind(actor.user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id=$2 WHERE id=$1")
        .bind(assignment)
        .bind(snapshot)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO poam_assignment_references(poam_id,assignment_id,assignment_version_id,added_by) VALUES($1,$2,$3,$4)")
        .bind(poam_id)
        .bind(assignment)
        .bind(snapshot)
        .bind(actor.user_id)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn assignment_scope_is_exact_deduplicated_and_hidden_scope_is_not_exported(pool: PgPool) {
    let admin = actor(&pool).await;
    let visible: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("visible-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .unwrap();
    let hidden: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(format!("hidden-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let a = system(&pool).await;
    let b = system(&pool).await;
    for (host, environment) in [(a, visible), (b, hidden)] {
        sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
            .bind(host)
            .bind(environment)
            .execute(&pool)
            .await
            .unwrap();
    }
    let poam = plan(&pool, &admin, &[a], "Policy A, assignment B").await;
    assignment_reference(&pool, &admin, poam, Some(a), None).await;
    assignment_reference(&pool, &admin, poam, Some(b), None).await;
    assignment_reference(&pool, &admin, poam, None, Some(hidden)).await;
    let files = export(&pool, &admin, &PoamListQuery::default(), &SystemClock)
        .await
        .unwrap();
    let rows = csv::ReaderBuilder::new()
        .from_reader(files.csv.bytes.as_slice())
        .records()
        .map(|row| row.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    let kinds: Vec<String> = serde_json::from_str(rows[0].get(5).unwrap()).unwrap();
    let ids: Vec<String> = serde_json::from_str(rows[0].get(6).unwrap()).unwrap();
    let names: Vec<String> = serde_json::from_str(rows[0].get(7).unwrap()).unwrap();
    assert_eq!(kinds, ["environment", "system", "system"]);
    assert_eq!(
        ids.into_iter().collect::<HashSet<_>>(),
        [hidden, a, b]
            .into_iter()
            .map(|id| id.to_string())
            .collect()
    );
    let environment_name: String = sqlx::query_scalar("SELECT name FROM environments WHERE id=$1")
        .bind(hidden)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(names[0], environment_name);
    for (index, system_id) in [a, b].into_iter().enumerate() {
        let hostname: String = sqlx::query_scalar("SELECT hostname FROM systems WHERE id=$1")
            .bind(system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(names.contains(&hostname), "missing scope name {index}");
    }

    sqlx::query("UPDATE user_role_assignments SET role='viewer' WHERE user_id=$1")
        .bind(admin.user_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(admin.user_id)
        .bind(visible)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(admin.user_id)
        .bind(hidden)
        .execute(&pool)
        .await
        .unwrap();
    let authorized = export(&pool, &admin, &PoamListQuery::default(), &SystemClock)
        .await
        .unwrap();
    assert_eq!(
        csv::ReaderBuilder::new()
            .from_reader(authorized.csv.bytes.as_slice())
            .records()
            .count(),
        1
    );
    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1 AND environment_id=$2")
        .bind(admin.user_id)
        .bind(hidden)
        .execute(&pool)
        .await
        .unwrap();
    // The canonical list already removes plans with hidden assignment scope.
    // If a plan passes that filter, the selector's full-scope guard still
    // rejects hidden assignments instead of projecting a partial plan.
    let hidden_files = export(&pool, &admin, &PoamListQuery::default(), &SystemClock)
        .await
        .unwrap();
    assert_eq!(
        csv::ReaderBuilder::new()
            .from_reader(hidden_files.csv.bytes.as_slice())
            .records()
            .count(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn exports_all_authorized_pages_with_exact_evidence_and_safe_cells(pool: PgPool) {
    let actor = actor(&pool).await;
    let system_id = system(&pool).await;
    let first = plan(&pool, &actor, &[system_id], " =SUM(1,1)").await;
    for n in 0..101 {
        plan(&pool, &actor, &[system_id], &format!("Plan {n}")).await;
    }
    let files = export(
        &pool,
        &actor,
        &PoamListQuery {
            status: Some("active".into()),
            limit: Some(1),
            offset: Some(101),
            ..Default::default()
        },
        &SystemClock,
    )
    .await
    .unwrap();
    let csv = String::from_utf8(files.csv.bytes).unwrap();
    let rows = csv::ReaderBuilder::new()
        .from_reader(csv.as_bytes())
        .records()
        .map(|r| r.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 102);
    assert_eq!(
        rows.iter()
            .map(|row| row.get(1).unwrap())
            .collect::<HashSet<_>>()
            .len(),
        102
    );
    assert!(csv.contains(&first.to_string()));
    assert!(csv.contains("\"' =SUM(1,1)\""));
    assert!(csv.contains(&system_id.to_string()));
    assert!(csv.contains("\"2099-01-01\""));
    assert!(files.xlsx.bytes.starts_with(b"PK"));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn multi_scope_and_revoked_role(pool: PgPool) {
    let actor = actor(&pool).await;
    let a = system(&pool).await;
    let b = system(&pool).await;
    let multi = plan(&pool, &actor, &[a, b], "Two affected systems").await;
    let files = export(&pool, &actor, &PoamListQuery::default(), &SystemClock)
        .await
        .unwrap();
    let rows = csv::ReaderBuilder::new()
        .from_reader(files.csv.bytes.as_slice())
        .records()
        .map(|r| r.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    let multi_rows = rows
        .iter()
        .filter(|row| row.get(1) == Some(multi.to_string().as_str()))
        .collect::<Vec<_>>();
    assert_eq!(multi_rows.len(), 2);
    for row in multi_rows {
        let ids: Vec<String> = serde_json::from_str(row.get(6).unwrap()).unwrap();
        let kinds: Vec<String> = serde_json::from_str(row.get(5).unwrap()).unwrap();
        assert_eq!(kinds, ["system", "system"]);
        assert_eq!(
            ids.iter().collect::<HashSet<_>>(),
            [a.to_string(), b.to_string()].iter().collect()
        );
        assert!(row.get(12).unwrap().parse::<Uuid>().is_ok());
    }
    sqlx::query("UPDATE users SET is_active=false WHERE id=$1")
        .bind(actor.user_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        export(&pool, &actor, &PoamListQuery::default(), &SystemClock)
            .await
            .unwrap_err(),
        ExportError::Forbidden
    );
}
