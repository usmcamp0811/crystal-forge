// Compile the converter independently until the route owner registers its module.
pub use crystal_forge::services::{register_export_selection, register_tabular_export};
pub use crystal_forge::{models, queries, services};
#[path = "../src/services/register_mixed_tabular.rs"]
mod register_mixed_tabular;

use crystal_forge::models::poam::PoamListQuery;
use crystal_forge::queries::acceptance_register::AcceptanceListQuery;
use crystal_forge::services::poam::{PoamActor, SystemClock};
use quick_xml::events::Event;
use sqlx::PgPool;
use std::collections::HashSet;
use std::io::{Cursor, Read};
use uuid::Uuid;

fn csv_rows(bytes: &[u8]) -> Vec<Vec<String>> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(bytes)
        .records()
        .map(|row| row.unwrap().iter().map(str::to_owned).collect())
        .collect()
}

fn worksheet(bytes: &[u8]) -> Vec<Vec<String>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    assert!(archive.by_name("[Content_Types].xml").is_ok());
    let mut sheet = String::new();
    archive
        .by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut sheet)
        .unwrap();
    let mut strings = String::new();
    if let Ok(mut shared) = archive.by_name("xl/sharedStrings.xml") {
        shared.read_to_string(&mut strings).unwrap();
    }
    let mut values = Vec::new();
    let mut xml = quick_xml::Reader::from_str(&strings);
    let mut value = String::new();
    loop {
        match xml.read_event().unwrap() {
            Event::Start(tag) if tag.name().as_ref() == b"si" => value.clear(),
            Event::Text(text) => value.push_str(&text.unescape().unwrap()),
            Event::End(tag) if tag.name().as_ref() == b"si" => values.push(value.clone()),
            Event::Eof => break,
            _ => {}
        }
    }
    let mut xml = quick_xml::Reader::from_str(&sheet);
    let mut rows = Vec::new();
    let mut column = 0;
    let mut shared = false;
    let mut in_value = false;
    loop {
        match xml.read_event().unwrap() {
            Event::Start(tag) if tag.name().as_ref() == b"row" => {
                rows.push(vec![String::new(); 16])
            }
            Event::Start(tag) if tag.name().as_ref() == b"c" => {
                let reference = tag
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"r")
                    .unwrap();
                column = if reference.value[0] == b'P' {
                    15
                } else {
                    (reference.value[0] - b'A') as usize
                };
                shared = tag
                    .attributes()
                    .flatten()
                    .any(|a| a.key.as_ref() == b"t" && a.value.as_ref() == b"s");
            }
            Event::Start(tag) if tag.name().as_ref() == b"v" => in_value = true,
            Event::End(tag) if tag.name().as_ref() == b"v" => in_value = false,
            Event::Text(text) if in_value => {
                let value = text.unescape().unwrap();
                rows.last_mut().unwrap()[column] = if shared {
                    values[value.parse::<usize>().unwrap()].clone()
                } else {
                    value.into_owned()
                };
            }
            Event::Start(tag) | Event::Empty(tag) if tag.name().as_ref() == b"f" => {
                panic!("formula cell")
            }
            Event::Eof => break,
            _ => {}
        }
    }
    rows
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified disposable PG35457"]
async fn mixed_snapshot_exports_full_sources_and_rejects_combined_overflow(pool: PgPool) {
    let user: Uuid = sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email) VALUES($1,'Mixed','Admin',$2) RETURNING id")
        .bind(format!("mixed-{}", Uuid::new_v4()))
        .bind(format!("mixed-{}@example.invalid", Uuid::new_v4()))
        .fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO user_role_assignments(user_id,role) VALUES($1,'admin')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let actor = PoamActor {
        user_id: user,
        identifier: "Mixed Admin".into(),
        is_admin: false,
        can_mutate: false,
        environment_ids: Vec::new(),
        request_origin: None,
    };
    let env: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES('mixed-environment') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let hosts: Vec<Uuid> = sqlx::query_scalar("INSERT INTO systems(hostname,public_key,derivation,environment_id) VALUES('mixed-host-a','key','key',$1),('mixed-host-b','key','key',$1) RETURNING id")
        .bind(env).fetch_all(&pool).await.unwrap();
    let mut plans = Vec::new();
    let mut findings = Vec::new();
    for n in 0..2 {
        // The deferred active-finding guard checks the plan at transaction commit.
        let mut tx = pool.begin().await.unwrap();
        let policy: Uuid = sqlx::query_scalar("INSERT INTO deployment_policies(name,policy_type,config,enabled) VALUES($1,'custom_check','{}',false) RETURNING id")
            .bind(format!("mixed-policy-{n}"))
            .fetch_one(&mut *tx).await.unwrap();
        let plan: Uuid = sqlx::query_scalar("INSERT INTO poams(title,plan,target_date,risk,created_by) VALUES($1,'No source-linked scan recorded','2099-01-01','high',$2) RETURNING id")
            .bind(if n == 0 { " =SUM(1,1)" } else { "Second plan" })
            .bind(user).fetch_one(&mut *tx).await.unwrap();
        plans.push(plan);
        for host in &hosts {
            let finding: Uuid = sqlx::query_scalar(
                "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
            )
            .bind(host)
            .bind(policy)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            findings.push(finding);
            sqlx::query(
                "INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)",
            )
            .bind(plan)
            .bind(finding)
            .bind(user)
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
    }
    sqlx::query("INSERT INTO cves(id) VALUES('CVE-2099-12345')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) SELECT 'CVE-2099-12345','pkg-' || n,$1,'accepted','decision', $2, now() FROM generate_series(1, 102) n")
        .bind(env).bind(user).execute(&pool).await.unwrap();
    let mut selection = register_export_selection::select(
        &pool,
        &actor,
        &PoamListQuery {
            limit: Some(1),
            offset: Some(99),
            ..Default::default()
        },
        &AcceptanceListQuery {
            limit: Some(1),
            offset: Some(99),
            ..Default::default()
        },
        &SystemClock,
    )
    .await
    .unwrap();
    assert_eq!(selection.poams.len(), 2);
    assert_eq!(selection.acceptances.len(), 102);
    let files = register_mixed_tabular::write_authorized(&selection).unwrap();
    let csv = csv_rows(&files.csv.bytes);
    assert_eq!(worksheet(&files.xlsx.bytes), csv);
    assert_eq!(csv.len(), 107); // Header, four plan links, 102 decisions.
    assert_eq!(
        csv.iter()
            .skip(1)
            .map(|row| &row[1])
            .collect::<HashSet<_>>()
            .len(),
        104
    );
    let plan_rows = csv
        .iter()
        .filter(|row| row[1] == plans[0].to_string())
        .collect::<Vec<_>>();
    assert_eq!(plan_rows.len(), 2);
    assert_eq!(plan_rows[0][3], "' =SUM(1,1)");
    assert_eq!(plan_rows[0][8], ""); // Do not promote first CVE to a plan fact.
    assert_eq!(plan_rows[0][13], ""); // Policy link has no scan baseline.
    let ids: Vec<String> = serde_json::from_str(&plan_rows[0][6]).unwrap();
    let names: Vec<String> = serde_json::from_str(&plan_rows[0][7]).unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(names.len(), 2);
    assert_eq!(
        names.iter().map(String::as_str).collect::<HashSet<_>>(),
        ["mixed-host-a", "mixed-host-b"].into_iter().collect()
    );
    assert!(
        plan_rows
            .iter()
            .all(|row| findings.iter().any(|id| row[12] == id.to_string()))
    );
    assert!(
        csv.iter()
            .filter(|row| row[0] == "CVE decision")
            .all(|row| row[12].is_empty()
                && row[13].is_empty()
                && row[14] == "No source-linked finding or scan recorded")
    );

    selection.acceptances[0].source_id = plans[0];
    assert!(register_mixed_tabular::write_authorized(&selection).is_err());
    sqlx::query("INSERT INTO cve_environment_dispositions(canonical_cve_id,canonical_package_name,environment_id,state,justification,accepted_by,accepted_at) SELECT 'CVE-2099-12345','overflow-' || n,$1,'accepted','decision',$2,now() FROM generate_series(1, 897) n")
        .bind(env).bind(user).execute(&pool).await.unwrap();
    assert!(matches!(
        register_export_selection::select(
            &pool,
            &actor,
            &PoamListQuery::default(),
            &AcceptanceListQuery::default(),
            &SystemClock
        )
        .await,
        Err(register_export_selection::RegisterExportSelectionError::TooManyRows)
    ));
}
