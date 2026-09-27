//! Isolated PostgreSQL tests for scoped, paged CVE inventory projections.
//! Run only against a verified disposable cluster with CREATEDB privileges.

use crystal_forge::api::models::{CveFilters, CveInventoryProjectionParams};
use crystal_forge::queries::cves::{
    CveReadScope, fetch_cve_inventory_groups, fetch_cve_inventory_members,
    fetch_cve_inventory_pairs, fetch_cve_list,
};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

fn params(group_by: &str) -> CveInventoryProjectionParams {
    CveInventoryProjectionParams {
        group_by: group_by.into(),
        environment_id: None,
        severity: None,
        fix_status: None,
        triage_status: None,
        package: None,
        search: None,
        sort: None,
        offset: None,
        limit: None,
        group_id: None,
    }
}

async fn system(pool: &PgPool, suffix: &str, environment_id: Uuid) -> (Uuid, String, i32, i32) {
    let hostname = format!("projection-{suffix}");
    let flake_name = format!("flake-{suffix}");
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes(name,repo_url) VALUES($1,$2) RETURNING id")
            .bind(&flake_name)
            .bind(format!("https://example.invalid/{suffix}"))
            .fetch_one(pool)
            .await
            .expect("fixture flake");
    let commit: i32 = sqlx::query_scalar(
        "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES($1,$2,now()) RETURNING id",
    )
    .bind(flake)
    .bind(suffix)
    .fetch_one(pool)
    .await
    .expect("fixture commit");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,public_key,derivation,flake_id,system_configuration_name,environment_id) \
         VALUES($1,'test-key','test-derivation',$2,$1,$3) RETURNING id",
    )
    .bind(&hostname)
    .bind(flake)
    .bind(environment_id)
    .fetch_one(pool)
    .await
    .expect("fixture system");
    (id, hostname, commit, flake)
}

async fn derivation(pool: &PgPool, hostname: &str, commit: i32, suffix: &str) -> (i32, String) {
    let path = format!("/nix/store/{suffix}");
    let id = sqlx::query_scalar(
        "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path, \
         status_id,completed_at,store_path) VALUES($1,'nixos',$2,$3,10,now(),$4) RETURNING id",
    )
    .bind(commit)
    .bind(hostname)
    .bind(format!("{path}.drv"))
    .bind(&path)
    .fetch_one(pool)
    .await
    .expect("fixture derivation");
    (id, path)
}

async fn scan(pool: &PgPool, derivation_id: i32, suffix: &str, many: bool) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO cve_scans(derivation_id,scanner_name,status) \
         VALUES($1,'projection-test','in_progress') RETURNING id",
    )
    .bind(derivation_id)
    .fetch_one(pool)
    .await
    .expect("fixture scan");
    sqlx::query(
        "INSERT INTO cve_scan_vulnerability_observations( \
         scan_id,canonical_cve_id,canonical_package_name,observed_package_name, \
         observed_package_version,observed_derivation_path,is_whitelisted,detection_method) \
         SELECT $1, 'CVE-2099-' || lpad(n::text,4,'0'), 'pkg', 'pkg', '1.0', \
                $2 || n::text || '.drv',false,'projection-test' \
         FROM generate_series(1,CASE WHEN $3 THEN 205 ELSE 1 END) n",
    )
    .bind(id)
    .bind(format!("/nix/store/{suffix}-pkg-"))
    .bind(many)
    .execute(pool)
    .await
    .expect("fixture observations");
    if many {
        sqlx::query(
            "INSERT INTO cve_scan_vulnerability_observations( \
             scan_id,canonical_cve_id,canonical_package_name,observed_package_name, \
             observed_package_version,observed_derivation_path,is_whitelisted,detection_method) \
             VALUES($1,'CVE-2099-0001','another-pkg','another-pkg','2.0',$2,false,'projection-test'), \
                   ($1,'CVE-2099-0001','pkg','pkg','1.1',$3,false,'projection-test')",
        )
        .bind(id)
        .bind(format!("/nix/store/{suffix}-another-pkg.drv"))
        .bind(format!("/nix/store/{suffix}-duplicate-pkg.drv"))
        .execute(pool)
        .await
        .expect("distinct package and duplicate stable pair");
    }
    sqlx::query(
        "UPDATE cve_scans SET status='completed',completed_at=now(),evidence_schema_version=1 WHERE id=$1",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("seal exact scan");
    id
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires an explicitly verified disposable DATABASE_URL"]
async fn scoped_groups_and_members_keep_all_sections_and_page_past_200(pool: PgPool) {
    let suffix = Uuid::new_v4().simple().to_string();
    let a: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(format!("A-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("environment A");
    let b: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(format!("B-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("environment B");
    sqlx::query(
        "INSERT INTO cves(id,cvss_v3_score,description,published_date) \
         SELECT 'CVE-2099-' || lpad(n::text,4,'0'),8.1,'projection finding','2099-01-01' \
         FROM generate_series(1,205) n",
    )
    .execute(&pool)
    .await
    .expect("CVE metadata");
    sqlx::query("UPDATE cves SET cvss_v3_score=9.7,exploited=true WHERE id='CVE-2099-0001'")
        .execute(&pool)
        .await
        .expect("critical exploited advisory");

    let (first, first_host, first_commit, first_flake) =
        system(&pool, &format!("{suffix}-first"), a).await;
    let (first_derivation, first_path) =
        derivation(&pool, &first_host, first_commit, &format!("{suffix}-first")).await;
    scan(&pool, first_derivation, &suffix, true).await;
    sqlx::query(
        "INSERT INTO system_states(hostname,change_reason,store_path,generation, \
         generation_matches_current_store_path) VALUES($1,'startup',$2,1,true)",
    )
    .bind(&first_host)
    .bind(&first_path)
    .execute(&pool)
    .await
    .expect("first current report");

    let (second, second_host, second_commit, _) =
        system(&pool, &format!("{suffix}-second"), a).await;
    let (second_derivation, second_path) = derivation(
        &pool,
        &second_host,
        second_commit,
        &format!("{suffix}-second"),
    )
    .await;
    scan(&pool, second_derivation, &suffix, false).await;
    sqlx::query(
        "INSERT INTO system_states(hostname,change_reason,store_path,generation, \
         generation_matches_current_store_path) VALUES($1,'startup',$2,1,true)",
    )
    .bind(&second_host)
    .bind(&second_path)
    .execute(&pool)
    .await
    .expect("second current report");

    // A separate exact target for the first host overlaps its current pair.
    let scheduled_commit: i32 = sqlx::query_scalar(
        "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) \
         VALUES($1,$2,now()+interval '1 minute') RETURNING id",
    )
    .bind(first_flake)
    .bind(format!("scheduled-{suffix}"))
    .fetch_one(&pool)
    .await
    .expect("scheduled commit");
    let (scheduled_derivation, scheduled_path) = derivation(
        &pool,
        &first_host,
        scheduled_commit,
        &format!("{suffix}-scheduled"),
    )
    .await;
    scan(&pool, scheduled_derivation, &suffix, false).await;
    let artifact: Uuid = sqlx::query_scalar(
        "INSERT INTO evaluation_snapshots(commit_id,configuration_name,lifecycle) \
         VALUES($1,$2,'available') RETURNING id",
    )
    .bind(scheduled_commit)
    .bind(&first_host)
    .fetch_one(&pool)
    .await
    .expect("exact target artifact");
    sqlx::query("UPDATE evaluation_snapshots SET integrity_version=1 WHERE id=$1")
        .bind(artifact)
        .execute(&pool)
        .await
        .expect("certify empty exact artifact");
    sqlx::query(
        "INSERT INTO pending_system_deployments(system_id,target_store_path,status,expires_at, \
         requested_commit_id,requested_derivation_id,evaluation_snapshot_id, \
         evaluation_snapshot_binding_expected) \
         VALUES($1,$2,'pending',now()+interval '1 hour',$3,$4,$5,true)",
    )
    .bind(first)
    .bind(&scheduled_path)
    .bind(scheduled_commit)
    .bind(scheduled_derivation)
    .bind(artifact)
    .execute(&pool)
    .await
    .expect("scheduled target");

    let (historical, historical_host, historical_commit, _) =
        system(&pool, &format!("{suffix}-historical"), a).await;
    let (historical_derivation, _) = derivation(
        &pool,
        &historical_host,
        historical_commit,
        &format!("{suffix}-historical"),
    )
    .await;
    let historical_scan: Uuid = sqlx::query_scalar(
        "INSERT INTO cve_scans(derivation_id,scanner_name,status,completed_at) \
         VALUES($1,'legacy','completed',now()) RETURNING id",
    )
    .bind(historical_derivation)
    .fetch_one(&pool)
    .await
    .expect("historical scan");
    let package_derivation: i32 = sqlx::query_scalar(
        "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path, \
         status_id,pname,version) VALUES($1,'package','pkg',$2,10,'pkg','0.9') RETURNING id",
    )
    .bind(historical_commit)
    .bind(format!("/nix/store/{suffix}-historical-pkg.drv"))
    .fetch_one(&pool)
    .await
    .expect("historical package");
    sqlx::query("INSERT INTO scan_packages(scan_id,derivation_id) VALUES($1,$2)")
        .bind(historical_scan)
        .bind(package_derivation)
        .execute(&pool)
        .await
        .expect("historical scan package");
    sqlx::query(
        "INSERT INTO package_vulnerabilities(derivation_id,cve_id,is_whitelisted,fixed_version) \
         VALUES($1,'CVE-2099-0001',false,'1.1')",
    )
    .bind(package_derivation)
    .execute(&pool)
    .await
    .expect("historical finding");

    // Neither an unseen active host nor a reported host without completed
    // CVE evidence is a finding or proof of a clean scan.
    let (_unseen, _, _, _) = system(&pool, &format!("{suffix}-unseen"), a).await;
    let (_no_scan, no_scan_host, _, _) = system(&pool, &format!("{suffix}-no-scan"), a).await;
    sqlx::query(
        "INSERT INTO system_states(hostname,change_reason,store_path) VALUES($1,'startup',$2)",
    )
    .bind(&no_scan_host)
    .bind(format!("/nix/store/{suffix}-unscanned"))
    .execute(&pool)
    .await
    .expect("unscanned host report");

    let (hidden, hidden_host, hidden_commit, _) =
        system(&pool, &format!("{suffix}-hidden"), b).await;
    let (hidden_derivation, hidden_path) = derivation(
        &pool,
        &hidden_host,
        hidden_commit,
        &format!("{suffix}-hidden"),
    )
    .await;
    scan(&pool, hidden_derivation, &suffix, false).await;
    sqlx::query(
        "INSERT INTO system_states(hostname,change_reason,store_path,generation, \
         generation_matches_current_store_path) VALUES($1,'startup',$2,1,true)",
    )
    .bind(&hidden_host)
    .bind(&hidden_path)
    .execute(&pool)
    .await
    .expect("hidden current report");

    let scope = CveReadScope::Environments(vec![a]);
    let mut pairs_query = params("");
    pairs_query.limit = Some(200);
    for sort in ["severity", "cvss", "age", "affected"] {
        pairs_query.sort = Some(sort.into());
        let first_page = fetch_cve_inventory_pairs(&pool, &scope, &pairs_query)
            .await
            .expect("scoped pairs first page");
        assert_eq!(first_page.total, 206);
        assert_eq!(first_page.items.len(), 200);
        assert_eq!(first_page.next_offset, Some(200));
        pairs_query.offset = first_page.next_offset;
        let last_page = fetch_cve_inventory_pairs(&pool, &scope, &pairs_query)
            .await
            .expect("scoped pairs beyond the first page");
        assert_eq!(last_page.total, 206);
        assert_eq!(last_page.items.len(), 6);
        assert_eq!(last_page.next_offset, None);
        let pairs: Vec<_> = first_page
            .items
            .into_iter()
            .chain(last_page.items)
            .collect();
        let distinct: BTreeSet<_> = pairs
            .iter()
            .map(|pair| (pair.cve_id.clone(), pair.package_name.clone()))
            .collect();
        assert_eq!(distinct.len(), 206, "no duplicate stable pair across pages");
        let normal_list = fetch_cve_list(
            &pool,
            &scope,
            &CveFilters {
                sort: Some(sort.into()),
                ..CveFilters::default()
            },
        )
        .await
        .expect("existing list ordering");
        assert_eq!(
            pairs
                .iter()
                .map(|pair| (&pair.cve_id, &pair.package_name))
                .collect::<Vec<_>>(),
            normal_list
                .iter()
                .map(|pair| (&pair.cve_id, &pair.package_name))
                .collect::<Vec<_>>(),
            "pair pages retain the normal list order for {sort}"
        );
        let shared = pairs
            .iter()
            .find(|pair| {
                pair.cve_id == "CVE-2099-0001" && pair.package_name.as_deref() == Some("pkg")
            })
            .expect("overlapping pair");
        assert_eq!(
            (
                shared.current_affected_count,
                shared.scheduled_deployment_target_count,
                shared.historical_inventory_count,
                shared.affected_count
            ),
            (2, 1, 1, 2)
        );
        pairs_query.offset = None;
    }
    pairs_query.environment_id = Some(b);
    let denied_pairs = fetch_cve_inventory_pairs(&pool, &scope, &pairs_query)
        .await
        .expect("unauthorized environment pairs");
    assert_eq!(denied_pairs.total, 0);
    assert!(denied_pairs.items.is_empty());
    pairs_query.environment_id = Some(a);
    pairs_query.search = Some("CVE-2099-0205".into());
    pairs_query.limit = Some(1);
    let filtered_pairs = fetch_cve_inventory_pairs(&pool, &scope, &pairs_query)
        .await
        .expect("filters before pair page");
    assert_eq!(filtered_pairs.total, 1);
    assert_eq!(filtered_pairs.items[0].cve_id, "CVE-2099-0205");
    assert_eq!(filtered_pairs.next_offset, None);
    pairs_query.search = None;
    pairs_query.environment_id = Some(b);
    let admin_pairs = fetch_cve_inventory_pairs(&pool, &CveReadScope::All, &pairs_query)
        .await
        .expect("admin environment pairs");
    assert_eq!(admin_pairs.total, 1);
    assert_eq!(admin_pairs.items[0].current_affected_count, 1);
    assert_eq!(admin_pairs.items[0].affected_count, 1);

    let mut query = params("environment");
    let groups = fetch_cve_inventory_groups(&pool, &scope, &query)
        .await
        .expect("scoped groups");
    assert_eq!(groups.total, 1);
    let group = &groups.items[0];
    assert_eq!(group.group_id, Some(a));
    assert_eq!(group.total_active_hosts, Some(5));
    assert_eq!(
        (group.cve_count, group.cve_package_count, group.host_count),
        (205, 206, 3)
    );
    assert_eq!(
        (
            group.critical_pair_count,
            group.high_pair_count,
            group.medium_pair_count,
            group.low_pair_count,
            group.unknown_pair_count
        ),
        (2, 204, 0, 0, 0),
        "the overlapping current/scheduled/historical pair counts once"
    );
    assert_eq!(
        (group.exploited_pair_count, group.patchable_pair_count),
        (2, 1)
    );
    let serialized = serde_json::to_value(group).expect("group JSON");
    assert!(serialized.get("scan_coverage").is_none());
    assert!(serialized.get("outstanding_count").is_none());
    assert_eq!(
        (
            group.current_host_count,
            group.scheduled_host_count,
            group.historical_host_count
        ),
        (2, 1, 1)
    );
    query.environment_id = Some(b);
    let denied = fetch_cve_inventory_groups(&pool, &scope, &query)
        .await
        .expect("hidden filter");
    assert_eq!(denied.total, 0);
    assert!(denied.items.is_empty());
    let admin_b = fetch_cve_inventory_groups(&pool, &CveReadScope::All, &query)
        .await
        .expect("admin environment B");
    assert_eq!(admin_b.total, 1);
    assert_eq!(admin_b.items[0].group_id, Some(b));
    assert_eq!(admin_b.items[0].total_active_hosts, Some(1));
    assert_eq!(admin_b.items[0].critical_pair_count, 1);
    assert_eq!(admin_b.items[0].exploited_pair_count, 1);
    assert_eq!(admin_b.items[0].patchable_pair_count, 0);
    query.environment_id = None;
    query.group_id = Some(b);
    let denied = fetch_cve_inventory_members(&pool, &scope, &query)
        .await
        .expect("hidden group");
    assert_eq!(denied.total, 0);

    query.group_id = Some(a);
    query.limit = Some(200);
    let page = fetch_cve_inventory_members(&pool, &scope, &query)
        .await
        .expect("first page");
    assert_eq!(page.total, 209);
    assert_eq!(page.items.len(), 200);
    assert_eq!(page.next_offset, Some(200));
    query.offset = page.next_offset;
    let rest = fetch_cve_inventory_members(&pool, &scope, &query)
        .await
        .expect("second page");
    assert_eq!(rest.total, 209);
    assert_eq!(rest.items.len(), 9);
    assert_eq!(rest.next_offset, None);
    let members: Vec<_> = page.items.into_iter().chain(rest.items).collect();
    assert!(
        members
            .iter()
            .all(|item| item.environment_id == Some(a) && item.system_id != hidden)
    );
    assert_eq!(
        members
            .iter()
            .filter(|item| item.system_id == first && item.inventory_section == "current")
            .count(),
        206
    );
    assert!(
        members
            .iter()
            .any(|item| item.system_id == historical && item.inventory_section == "historical")
    );
    assert!(
        members.iter().any(|item| item.system_id == first
            && item.inventory_section == "scheduled_deployment_target")
    );
    assert!(
        members
            .iter()
            .any(|item| item.system_id == second && item.inventory_section == "current")
    );

    let mut host_query = params("host");
    host_query.limit = Some(1);
    let mut host_ids = Vec::new();
    loop {
        let page = fetch_cve_inventory_groups(&pool, &scope, &host_query)
            .await
            .expect("host page");
        assert_eq!(page.total, 3);
        let host = &page.items[0];
        assert_eq!(host.host_count, 1);
        assert_eq!(host.total_active_hosts, None);
        assert!(
            host.flake_name
                .as_ref()
                .is_some_and(|name| name.starts_with("flake-"))
        );
        assert!(host.deployment_status.is_some());
        host_ids.push(host.group_id.expect("exact host ID"));
        match page.next_offset {
            Some(offset) => host_query.offset = Some(offset),
            None => break,
        }
    }
    host_ids.sort();
    let mut expected = vec![first, second, historical];
    expected.sort();
    assert_eq!(host_ids, expected);
    host_query.group_id = Some(first);
    let host_members = fetch_cve_inventory_members(&pool, &scope, &host_query)
        .await
        .expect("exact host membership");
    assert_eq!(host_members.total, 207);

    // A filter must narrow the pair set before either the page or its totals.
    query.offset = None;
    query.group_id = None;
    query.search = Some("CVE-2099-0001".into());
    let filtered = fetch_cve_inventory_groups(&pool, &scope, &query)
        .await
        .expect("filtered groups");
    assert_eq!(filtered.items[0].cve_package_count, 2);
    assert_eq!(filtered.items[0].cve_count, 1);
    assert_eq!(filtered.items[0].host_count, 3);
    assert_eq!(filtered.items[0].total_active_hosts, Some(5));
    assert_eq!(filtered.items[0].critical_pair_count, 2);
    assert_eq!(filtered.items[0].exploited_pair_count, 2);
    assert_eq!(filtered.items[0].patchable_pair_count, 1);
    query.search = None;
    query.severity = Some("high".into());
    let high = fetch_cve_inventory_groups(&pool, &scope, &query)
        .await
        .expect("filtered severity group");
    assert_eq!(high.items[0].cve_package_count, 204);
    assert_eq!(high.items[0].high_pair_count, 204);
    assert_eq!(high.items[0].critical_pair_count, 0);
    assert_eq!(high.items[0].exploited_pair_count, 0);
    assert_eq!(high.items[0].patchable_pair_count, 0);
    assert_eq!(high.items[0].total_active_hosts, Some(5));
}
