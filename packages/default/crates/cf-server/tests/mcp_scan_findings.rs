//! Migration-backed checks for scoped configuration scan inventory and POA&M
//! planning. HTTP cases drive production handlers without opening a socket.

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::{get, post},
};
use chrono::Utc;
use crystal_forge::{
    api::models::CreateAccessTokenRequest,
    auth::session::{CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token},
    config::ServerConfig,
    handlers::{agent_request::CFState, api::poam},
    mcp::{
        self,
        tools::{ToolCaller, ToolError, call_tool},
    },
    models::auth_identity::AuthRole,
    queries::{
        auth_identity::{create_user_session, insert_user_environment_membership, sync_user_role},
        users::insert_user,
    },
    queue::QueueNotifier,
    server::jobs::BackgroundJobRegistry,
    services::access_tokens::{self, AccessTokenSettings},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn config() -> ServerConfig {
    ServerConfig {
        mcp_enable: true,
        mcp_public_url: Some("https://forge.example.invalid/mcp".into()),
        ..ServerConfig::default()
    }
}

async fn issue(pool: &PgPool, role: AuthRole, env: Uuid, scope: &str) -> (ToolCaller, String) {
    issue_scopes(pool, role, env, &[scope]).await
}

async fn issue_scopes(
    pool: &PgPool,
    role: AuthRole,
    env: Uuid,
    scopes: &[&str],
) -> (ToolCaller, String) {
    let user = insert_user(pool, &format!("{}@example.invalid", Uuid::new_v4()), None)
        .await
        .unwrap();
    sync_user_role(pool, user.id, role).await.unwrap();
    if role != AuthRole::Admin {
        insert_user_environment_membership(pool, user.id, env)
            .await
            .unwrap();
    }
    let settings = AccessTokenSettings::from_server_config(&config(), 86_400);
    let request = CreateAccessTokenRequest {
        name: Some("Scan inventory test".into()),
        audience: Some("mcp".into()),
        expires_in_days: Some(30),
        environment_ids: Some(vec![env.to_string()]),
        scopes: Some(scopes.iter().map(|scope| (*scope).to_owned()).collect()),
        confirm_production_write: None,
    };
    let issued = access_tokens::create(pool, user.id, &request, &settings)
        .await
        .unwrap();
    let secret = issued.secret.expose().to_owned();
    let validated = access_tokens::validate_presented_token(pool, &secret, Utc::now(), &settings)
        .await
        .unwrap();
    (
        ToolCaller {
            token_id: issued.metadata.id,
            owner_user_id: user.id,
            owner_is_admin: validated.owner_is_admin,
            owner_environment_ids: validated.owner_environment_ids,
            grant: validated.grant,
        },
        secret,
    )
}

async fn environment(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn flake(pool: &PgPool, name: &str) -> i32 {
    sqlx::query_scalar("INSERT INTO flakes(name,repo_url) VALUES($1,$2) RETURNING id")
        .bind(name)
        .bind(format!("https://example.invalid/{name}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn host(pool: &PgPool, flake: i32, env: Uuid, hostname: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO systems(hostname,is_active,derivation,flake_id,system_configuration_name,environment_id,management_mode) \
         VALUES($1,true,'fixture',$2,'Shared_Config',$3,'reference') RETURNING id",
    ).bind(hostname).bind(flake).bind(env).fetch_one(pool).await.unwrap()
}

async fn commit(pool: &PgPool, flake: i32, digit: &str) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES($1,$2,now()) RETURNING id",
    ).bind(flake).bind(digit.repeat(40)).fetch_one(pool).await.unwrap()
}

async fn set_head(pool: &PgPool, flake: i32, commit: i32) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM flake_branch_commit_snapshot WHERE flake_id=$1")
        .bind(flake)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO flake_branch_commit_snapshot(flake_id,commit_id,position) VALUES($1,$2,0)",
    )
    .bind(flake)
    .bind(commit)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE flakes SET snapshot_ready_at=NOW() WHERE id=$1")
        .bind(flake)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn derivation(pool: &PgPool, commit: i32, name: &str, path: &str) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path,store_path,status_id,completed_at) \
         VALUES($1,'nixos',$2,$3,$4,10,NOW()) RETURNING id",
    ).bind(commit).bind(name).bind(format!("{path}.drv")).bind(path).fetch_one(pool).await.unwrap()
}

async fn scan(pool: &PgPool, derivation: i32, status: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO cve_scans(derivation_id,scanner_name,status) VALUES($1,'inventory-fixture',$2) RETURNING id",
    ).bind(derivation).bind(status).fetch_one(pool).await.unwrap()
}

async fn seal(pool: &PgPool, id: Uuid) {
    sqlx::query(
        "UPDATE cve_scans SET status='completed',completed_at=NOW(),evidence_schema_version=1,\
                total_packages=12,total_vulnerabilities=7 WHERE id=$1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

async fn listing(pool: &PgPool, caller: &ToolCaller, args: Value) -> Value {
    call_tool(pool, caller, "list_scan_findings", args)
        .await
        .unwrap()
}

#[sqlx::test]
async fn head_is_strict_and_explicit_scans_are_exact_and_scoped(pool: PgPool) {
    let env = environment(&pool, "visible").await;
    let other_env = environment(&pool, "hidden").await;
    let repo = flake(&pool, "visible-repo").await;
    let other_repo = flake(&pool, "hidden-repo").await;
    let system = host(&pool, repo, env, "first").await;
    let hidden = host(&pool, other_repo, other_env, "hidden-host").await;
    let (caller, _) = issue(&pool, AuthRole::Admin, env, "cves:read").await;
    let older = commit(&pool, repo, "a").await;
    let current = commit(&pool, repo, "b").await;
    let other_commit = commit(&pool, other_repo, "c").await;
    let old_drv = derivation(&pool, older, "Shared_Config", "/nix/store/aaaa-old").await;
    let wrong_config = derivation(
        &pool,
        current,
        "shared_config",
        "/nix/store/bbbb-other-config",
    )
    .await;
    let wrong_flake = derivation(
        &pool,
        other_commit,
        "Shared_Config",
        "/nix/store/cccc-other-flake",
    )
    .await;
    let old_scan = scan(&pool, old_drv, "in_progress").await;
    seal(&pool, old_scan).await;
    let wrong_config_scan = scan(&pool, wrong_config, "in_progress").await;
    seal(&pool, wrong_config_scan).await;
    let wrong_flake_scan = scan(&pool, wrong_flake, "in_progress").await;
    seal(&pool, wrong_flake_scan).await;
    let args = json!({"system_id":system});
    assert_eq!(
        listing(&pool, &caller, args.clone()).await["state"],
        "head_not_ready"
    );
    set_head(&pool, repo, current).await;
    let missing = listing(&pool, &caller, args.clone()).await;
    assert_eq!(missing["state"], "target_not_ready");
    assert!(missing["counts"].is_null());
    assert!(missing["evidence_representation"].is_null());
    assert!(missing["scan_request_target"].is_null());

    let target = derivation(&pool, current, "Shared_Config", "/nix/store/dddd-head").await;
    let legacy = scan(&pool, target, "in_progress").await;
    sqlx::query("UPDATE cve_scans SET status='completed',completed_at=NOW() WHERE id=$1")
        .bind(legacy)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        listing(&pool, &caller, args.clone()).await["state"],
        "no_scan_for_head"
    );
    let queued = scan(&pool, target, "pending").await;
    let pending = listing(&pool, &caller, args).await;
    assert_eq!(pending["state"], "scan_pending_for_head");
    assert_eq!(pending["attempt"]["scan_id"], json!(queued));
    assert_eq!(
        pending["scan_request_target"]["expected_derivation_id"],
        target
    );

    let history = listing(
        &pool,
        &caller,
        json!({"system_id":system,"scan_id":old_scan}),
    )
    .await;
    assert_eq!(history["selection"], "exact_scan");
    assert_eq!(history["provenance"]["scan"]["scan_id"], json!(old_scan));
    assert_eq!(history["provenance"]["commit_id"], older);
    assert_eq!(history["head"]["configuration_derivation_id"], target);
    assert_eq!(history["provenance"]["scan_derivation_id"], old_drv);
    assert_eq!(history["is_runtime_authoritative"], false);
    for (system, scan) in [
        (system, legacy),
        (system, queued),
        (system, wrong_config_scan),
        (system, wrong_flake_scan),
        (system, Uuid::new_v4()),
        (hidden, wrong_flake_scan),
    ] {
        assert!(matches!(
            call_tool(
                &pool,
                &caller,
                "list_scan_findings",
                json!({"system_id":system,"scan_id":scan,"cursor":"malformed"})
            )
            .await,
            Err(ToolError::NotFound)
        ));
    }
}

#[sqlx::test]
async fn head_reuse_requires_proven_execution_and_cursor_stays_bound(pool: PgPool) {
    let env = environment(&pool, "visible").await;
    let repo = flake(&pool, "reuse-repo").await;
    let system = host(&pool, repo, env, "reference").await;
    let peer = host(&pool, repo, env, "peer").await;
    let (caller, _) = issue(&pool, AuthRole::Viewer, env, "cves:read").await;
    let older = commit(&pool, repo, "a").await;
    let current = commit(&pool, repo, "b").await;
    let path = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-fixture";
    let origin = derivation(&pool, older, "Shared_Config", path).await;
    let head: i32 = sqlx::query_scalar(
        "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path,expected_store_path,\
         reused_from_derivation_id,status_id) VALUES($1,'nixos','Shared_Config',$2,$3,$4,10) RETURNING id",
    ).bind(current).bind(format!("{path}.drv")).bind(path).bind(origin).fetch_one(&pool).await.unwrap();
    set_head(&pool, repo, current).await;
    let exact = scan(&pool, origin, "in_progress").await;
    sqlx::query(
        "INSERT INTO cves(id,cvss_v3_score) VALUES ('CVE-2099-0301',9.5),('CVE-2099-0302',5.0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO cve_scan_vulnerability_observations(scan_id,canonical_cve_id,canonical_package_name,\
         observed_package_name,observed_package_version,observed_derivation_path,is_whitelisted,detection_method) \
         VALUES ($1,'CVE-2099-0301','pkg','pkg-1','1','/a',false,'test'),\
                ($1,'CVE-2099-0301','pkg','pkg-2','2','/b',false,'test'),\
                ($1,'CVE-2099-0301','other','pkg','1','/c',false,'test'),\
                ($1,'CVE-2099-0302','pkg','pkg-1','1','/a',false,'test')",
    ).bind(exact).execute(&pool).await.unwrap();
    seal(&pool, exact).await;
    assert_eq!(
        listing(&pool, &caller, json!({"system_id":system})).await["state"],
        "target_not_ready"
    );
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO build_jobs(derivation_id,environment_id,status,completed_at) VALUES($1,$2,'success',NOW()) RETURNING id",
    ).bind(origin).bind(env).fetch_one(&pool).await.unwrap();
    sqlx::query("UPDATE derivations SET reused_build_job_id=$2 WHERE id=$1")
        .bind(head)
        .bind(job)
        .execute(&pool)
        .await
        .unwrap();

    let first = listing(
        &pool,
        &caller,
        json!({"system_id":system,"package":"pkg","limit":1}),
    )
    .await;
    assert_eq!(first["state"], "completed");
    assert_eq!(first["head"]["configuration_derivation_id"], head);
    assert_eq!(first["provenance"]["scan_derivation_id"], origin);
    assert_eq!(first["provenance"]["commit_id"], older);
    assert_eq!(first["provenance"]["commit_hash"], "a".repeat(40));
    assert_eq!(first["provenance"]["scanned_system_store_path"], path);
    assert_eq!(first["evidence_representation"], "schema1_observations");
    assert_eq!(first["is_runtime_authoritative"], false);
    assert_eq!(first["scan_request_target"]["expected_derivation_id"], head);
    assert_eq!(first["evidence_kind"], "configuration_scan");
    assert_eq!(first["read_only"], true);
    assert_eq!(first["counts"]["projected_unfiltered"], 3);
    assert_eq!(first["counts"]["projected_filtered"], 2);
    assert_eq!(first["counts"]["page"], 1);
    assert_eq!(first["counts"]["raw_scanner"]["total_vulnerabilities"], 7);
    assert_eq!(first["findings"][0]["package_name"], "pkg-1");
    let cursor = first["next_cursor"].as_str().unwrap();
    let second = listing(
        &pool,
        &caller,
        json!({"system_id":system,"package":"pkg","cursor":cursor}),
    )
    .await;
    assert_eq!(second["findings"][0]["cve_id"], "CVE-2099-0302");
    assert!(second["next_cursor"].is_null());
    let critical = listing(
        &pool,
        &caller,
        json!({"system_id":system,"package":"pkg","severity":["critical","high"]}),
    )
    .await;
    assert_eq!(critical["counts"]["projected_filtered"], 1);
    for args in [
        json!({"system_id":system,"cursor":cursor}),
        json!({"system_id":system,"package":"pkg","scan_id":exact,"cursor":cursor}),
        json!({"system_id":peer,"package":"pkg","cursor":cursor}),
        json!({"system_id":system,"package":"pkg","cursor":"a".repeat(8_193)}),
        json!({"system_id":system,"canonical_package_name":"pkg"}),
    ] {
        assert!(matches!(
            call_tool(&pool, &caller, "list_scan_findings", args).await,
            Err(ToolError::InvalidArguments(_))
        ));
    }

    sqlx::query(
        "UPDATE systems SET management_mode='managed',public_key='fixture-key' WHERE id=$1",
    )
    .bind(system)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO system_states(hostname,change_reason,store_path,generation, \
         generation_matches_current_store_path) VALUES('reference','startup',$1,1,true)",
    )
    .bind(path)
    .execute(&pool)
    .await
    .unwrap();
    let authoritative = listing(&pool, &caller, json!({"system_id":system})).await;
    assert_eq!(authoritative["is_runtime_authoritative"], true);
    assert_eq!(authoritative["read_only"], true);

    let explicit = listing(
        &pool,
        &caller,
        json!({"system_id":system,"scan_id":exact,"package":"pkg","limit":1}),
    )
    .await;
    let exact_cursor = explicit["next_cursor"].as_str().unwrap();
    let alternate = scan(&pool, head, "in_progress").await;
    seal(&pool, alternate).await;
    let old_exact = listing(&pool, &caller, json!({"system_id":system,"scan_id":exact})).await;
    assert_eq!(old_exact["is_runtime_authoritative"], false);
    let alternate_result = listing(
        &pool,
        &caller,
        json!({"system_id":system,"scan_id":alternate}),
    )
    .await;
    assert!(alternate_result["provenance"]["scanned_system_store_path"].is_null());
    assert!(matches!(
        call_tool(
            &pool,
            &caller,
            "list_scan_findings",
            json!({"system_id":system,"scan_id":alternate,"package":"pkg","cursor":exact_cursor})
        )
        .await,
        Err(ToolError::InvalidArguments(_))
    ));

    set_head(&pool, repo, commit(&pool, repo, "d").await).await;
    assert!(matches!(
        call_tool(
            &pool,
            &caller,
            "list_scan_findings",
            json!({
        "system_id":system,"package":"pkg","cursor":cursor})
        )
        .await,
        Err(ToolError::InvalidArguments(_))
    ));
    let exact_second = listing(
        &pool,
        &caller,
        json!({"system_id":system,"scan_id":exact,"package":"pkg","cursor":exact_cursor}),
    )
    .await;
    assert_eq!(exact_second["findings"][0]["cve_id"], "CVE-2099-0302");
}

async fn system_scans(pool: &PgPool, caller: &ToolCaller, args: Value) -> Value {
    call_tool(pool, caller, "list_system_scans", args)
        .await
        .unwrap()
}

fn scan_ids(page: &Value) -> Vec<Value> {
    page["scans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scan| scan["scan_id"].clone())
        .collect()
}

fn scan_flags(page: &Value, flag: &str) -> Vec<bool> {
    page["scans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scan| scan[flag].as_bool().unwrap())
        .collect()
}

/// Returns `(cve_id, package_name, installed_version, occurrence path)` of
/// every finding on the page, in response order.
fn finding_keys(page: &Value) -> Vec<(String, String, String, String)> {
    page["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| {
            let text = |key: &str| finding[key].as_str().unwrap().to_owned();
            (
                text("cve_id"),
                text("package_name"),
                text("installed_version"),
                text("occurrence_derivation_path"),
            )
        })
        .collect()
}

/// One observation: CVE, canonical package, observed name and version, and
/// observed derivation path.
type Observation<'a> = (&'a str, &'a str, &'a str, &'a str, &'a str);

fn owned(observations: &[Observation<'_>]) -> Vec<(String, String, String, String)> {
    observations
        .iter()
        .map(|(cve, _, name, version, path)| {
            (
                (*cve).to_owned(),
                (*name).to_owned(),
                (*version).to_owned(),
                (*path).to_owned(),
            )
        })
        .collect()
}

/// Returns a fixed completion time `minute` minutes after a base instant.
fn at(minute: i64) -> chrono::DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-11-20T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
        + chrono::Duration::minutes(minute)
}

/// Creates a sealed schema-1 scan with `observations` that completed at
/// `completed_at`. Completed evidence is immutable, so the observations and
/// the completion time are written before and while sealing.
async fn sealed_scan(
    pool: &PgPool,
    derivation: i32,
    completed_at: chrono::DateTime<Utc>,
    observations: &[Observation<'_>],
) -> Uuid {
    let id = scan(pool, derivation, "in_progress").await;
    for (cve, canonical, name, version, path) in observations {
        sqlx::query(
            "INSERT INTO cves(id,cvss_v3_score) VALUES($1,9.8) ON CONFLICT (id) DO NOTHING",
        )
        .bind(cve)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO cve_scan_vulnerability_observations(scan_id,canonical_cve_id,\
             canonical_package_name,observed_package_name,observed_package_version,\
             observed_derivation_path,is_whitelisted,detection_method) \
             VALUES ($1,$2,$3,$4,$5,$6,false,'test')",
        )
        .bind(id)
        .bind(cve)
        .bind(canonical)
        .bind(name)
        .bind(version)
        .bind(path)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE cve_scans SET status='completed',completed_at=$2,evidence_schema_version=1,\
         total_packages=$3,total_vulnerabilities=$3 WHERE id=$1",
    )
    .bind(id)
    .bind(completed_at)
    .bind(observations.len() as i32)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Asserts that following `scan_id` returns exactly `expected` findings, the
/// scan's own counts and provenance, and no runtime authority.
async fn assert_exact_scan(
    pool: &PgPool,
    caller: &ToolCaller,
    system: Uuid,
    scan_id: Uuid,
    expected: &[Observation<'_>],
    (commit_hash, derivation, store_path): (&str, i32, &str),
) {
    let page = listing(pool, caller, json!({"system_id":system,"scan_id":scan_id})).await;
    assert_eq!(page["selection"], "exact_scan");
    assert_eq!(page["state"], "completed");
    assert_eq!(page["evidence_kind"], "configuration_scan");
    assert_eq!(page["is_runtime_authoritative"], false);
    assert_eq!(finding_keys(&page), owned(expected), "{scan_id}");
    for finding in page["findings"].as_array().unwrap() {
        assert_eq!(finding["severity"], "critical");
    }
    assert_eq!(page["counts"]["projected_unfiltered"], expected.len());
    assert_eq!(page["counts"]["projected_filtered"], expected.len());
    assert_eq!(page["counts"]["page"], expected.len());
    assert_eq!(
        page["counts"]["raw_scanner"]["total_vulnerabilities"],
        expected.len()
    );
    assert_eq!(page["provenance"]["scan"]["scan_id"], json!(scan_id));
    assert_eq!(page["provenance"]["commit_hash"], commit_hash);
    assert_eq!(page["provenance"]["scan_derivation_id"], derivation);
    assert_eq!(page["provenance"]["scanned_system_store_path"], store_path);
    assert!(page["latest_sealed_scan"].is_null());
    assert!(page["next_cursor"].is_null());
}

#[sqlx::test]
async fn system_scans_follow_configuration_history_and_head_fallback(pool: PgPool) {
    let env = environment(&pool, "visible").await;
    let other_env = environment(&pool, "hidden").await;
    let repo = flake(&pool, "history-repo").await;
    let other_repo = flake(&pool, "other-repo").await;
    let system = host(&pool, repo, env, "history").await;
    let hidden = host(&pool, other_repo, other_env, "hidden-host").await;
    let (caller, _) = issue(&pool, AuthRole::Viewer, env, "cves:read").await;
    let first = commit(&pool, repo, "a").await;
    let second = commit(&pool, repo, "b").await;
    let first_path = "/nix/store/aaaa-first";
    let second_path = "/nix/store/bbbb-second";
    let first_drv = derivation(&pool, first, "Shared_Config", first_path).await;
    let second_drv = derivation(&pool, second, "Shared_Config", second_path).await;

    let old_findings = [(
        "CVE-2099-1001",
        "openssl",
        "openssl",
        "3.0.1",
        "/nix/store/o1-openssl-3.0.1.drv",
    )];
    let mid_findings = [(
        "CVE-2099-1002",
        "zlib",
        "zlib",
        "1.2.11",
        "/nix/store/z1-zlib-1.2.11.drv",
    )];
    let head_old_findings = [
        (
            "CVE-2099-1003",
            "bash",
            "bash",
            "5.1",
            "/nix/store/b1-bash-5.1.drv",
        ),
        (
            "CVE-2099-1004",
            "bash",
            "bash",
            "5.1",
            "/nix/store/b1-bash-5.1.drv",
        ),
    ];
    let head_new_findings = [(
        "CVE-2099-1005",
        "curl",
        "curl",
        "8.0.0",
        "/nix/store/c1-curl-8.0.0.drv",
    )];
    let old = sealed_scan(&pool, first_drv, at(0), &old_findings).await;
    let mid = sealed_scan(&pool, first_drv, at(10), &mid_findings).await;
    let head_old = sealed_scan(&pool, second_drv, at(20), &head_old_findings).await;
    let head_new = sealed_scan(&pool, second_drv, at(30), &head_new_findings).await;
    let newest_first = [json!(head_new), json!(head_old), json!(mid), json!(old)];

    // HEAD has no derivation yet: the state stays target_not_ready and the
    // reference names the newest sealed scan.
    set_head(&pool, repo, commit(&pool, repo, "d").await).await;
    let waiting = listing(&pool, &caller, json!({"system_id":system})).await;
    assert_eq!(waiting["state"], "target_not_ready");
    assert!(waiting["findings"].as_array().unwrap().is_empty());
    assert!(waiting["counts"].is_null());
    assert_eq!(waiting["latest_sealed_scan"]["scan_id"], json!(head_new));
    assert_eq!(waiting["latest_sealed_scan"]["commit_hash"], "b".repeat(40));
    assert_eq!(waiting["latest_sealed_scan"]["is_head"], false);
    assert_eq!(waiting["latest_sealed_scan"]["completed_at"], json!(at(30)));

    // Following the reference returns that scan's findings only: the other
    // three scans of the same flake and configuration contribute nothing.
    let fallback =
        Uuid::parse_str(waiting["latest_sealed_scan"]["scan_id"].as_str().unwrap()).unwrap();
    assert_exact_scan(
        &pool,
        &caller,
        system,
        fallback,
        &head_new_findings,
        (&"b".repeat(40), second_drv, second_path),
    )
    .await;
    // Historical configuration-only evidence keeps its own commit.
    assert_exact_scan(
        &pool,
        &caller,
        system,
        old,
        &old_findings,
        (&"a".repeat(40), first_drv, first_path),
    )
    .await;

    let all = system_scans(&pool, &caller, json!({"system_id":system})).await;
    assert_eq!(scan_ids(&all), newest_first);
    assert_eq!(all["evidence_kind"], "configuration_scan");
    assert_eq!(all["head_commit_hash"], "d".repeat(40));
    assert_eq!(scan_flags(&all, "is_head"), [false; 4]);
    assert_eq!(scan_flags(&all, "is_runtime_authoritative"), [false; 4]);
    let totals: Vec<_> = all["scans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scan| scan["counts"]["total_vulnerabilities"].clone())
        .collect();
    assert_eq!(totals, [json!(1), json!(2), json!(1), json!(1)]);
    assert_eq!(all["scans"][0]["commit_id"], second);
    assert_eq!(all["scans"][0]["derivation_id"], second_drv);
    assert_eq!(all["scans"][0]["completed_at"], json!(at(30)));
    assert_eq!(all["scans"][0]["scanner"]["name"], "inventory-fixture");
    assert_eq!(all["has_more"], false);
    assert!(all["next_cursor"].is_null());

    // HEAD configuration-only evidence: both scans of the HEAD derivation are
    // HEAD evidence; flake_head selects the newest of them.
    set_head(&pool, repo, second).await;
    let head = system_scans(&pool, &caller, json!({"system_id":system})).await;
    assert_eq!(head["head_commit_hash"], "b".repeat(40));
    assert_eq!(scan_flags(&head, "is_head"), [true, true, false, false]);
    assert_eq!(scan_flags(&head, "is_runtime_authoritative"), [false; 4]);
    let current = listing(&pool, &caller, json!({"system_id":system})).await;
    assert_eq!(current["state"], "completed");
    assert_eq!(current["selection"], "flake_head");
    assert_eq!(current["is_runtime_authoritative"], false);
    assert_eq!(current["provenance"]["scan"]["scan_id"], json!(head_new));
    assert_eq!(finding_keys(&current), owned(&head_new_findings));
    assert_eq!(current["latest_sealed_scan"]["scan_id"], json!(head_new));
    assert_eq!(current["latest_sealed_scan"]["is_head"], true);
    // The older HEAD scan stays readable with only its own findings.
    assert_exact_scan(
        &pool,
        &caller,
        system,
        head_old,
        &head_old_findings,
        (&"b".repeat(40), second_drv, second_path),
    )
    .await;

    let by_prefix = system_scans(
        &pool,
        &caller,
        json!({"system_id":system,"commit":"AAAAAAA"}),
    )
    .await;
    assert_eq!(scan_ids(&by_prefix), [json!(mid), json!(old)]);
    let by_full = system_scans(
        &pool,
        &caller,
        json!({"system_id":system,"commit":"b".repeat(40)}),
    )
    .await;
    assert_eq!(scan_ids(&by_full), [json!(head_new), json!(head_old)]);

    let page = system_scans(&pool, &caller, json!({"system_id":system,"limit":2})).await;
    let cursor = page["next_cursor"].as_str().unwrap().to_owned();
    for args in [
        json!({"system_id":system,"commit":"aaaaaaa","cursor":cursor}),
        json!({"system_id":system,"cursor":"malformed"}),
        json!({"system_id":system,"commit":"aaaaaa"}),
        json!({"system_id":system,"commit":"aaaaaaz"}),
        json!({"system_id":system,"commit":"a%aaaaa"}),
        json!({"system_id":system,"commit":" aaaaaaa"}),
        json!({"system_id":system,"commit":"aaaaaaa "}),
        json!({"system_id":system,"commit":"aaa aaaa"}),
        json!({"system_id":system,"commit":"aaaaaaa\n"}),
        json!({"system_id":system,"scan_id":old}),
    ] {
        assert!(
            matches!(
                call_tool(&pool, &caller, "list_system_scans", args.clone()).await,
                Err(ToolError::InvalidArguments(_))
            ),
            "{args}"
        );
    }
    for system in [hidden, Uuid::new_v4()] {
        assert!(matches!(
            call_tool(
                &pool,
                &caller,
                "list_system_scans",
                json!({"system_id":system})
            )
            .await,
            Err(ToolError::NotFound)
        ));
    }
}

#[sqlx::test]
async fn system_scans_report_runtime_authority_and_execution_reuse(pool: PgPool) {
    let env = environment(&pool, "visible").await;
    let (caller, _) = issue(&pool, AuthRole::Viewer, env, "cves:read").await;

    // A managed system whose running output decides runtime authority.
    let repo = flake(&pool, "runtime-repo").await;
    let system = host(&pool, repo, env, "runner").await;
    let first = commit(&pool, repo, "1").await;
    let second = commit(&pool, repo, "2").await;
    let first_path = "/nix/store/1111-runtime-first";
    let second_path = "/nix/store/2222-runtime-second";
    let first_drv = derivation(&pool, first, "Shared_Config", first_path).await;
    let second_drv = derivation(&pool, second, "Shared_Config", second_path).await;
    let historical = sealed_scan(
        &pool,
        first_drv,
        at(0),
        &[(
            "CVE-2099-2001",
            "glibc",
            "glibc",
            "2.38",
            "/nix/store/g1-glibc.drv",
        )],
    )
    .await;
    let head_old = sealed_scan(
        &pool,
        second_drv,
        at(10),
        &[(
            "CVE-2099-2002",
            "sudo",
            "sudo",
            "1.9.14",
            "/nix/store/s1-sudo.drv",
        )],
    )
    .await;
    let head_new = sealed_scan(
        &pool,
        second_drv,
        at(20),
        &[("CVE-2099-2003", "xz", "xz", "5.6.0", "/nix/store/x1-xz.drv")],
    )
    .await;
    set_head(&pool, repo, second).await;
    sqlx::query(
        "UPDATE systems SET management_mode='managed',public_key='fixture-key' WHERE id=$1",
    )
    .bind(system)
    .execute(&pool)
    .await
    .unwrap();
    let run = |store_path: &'static str, generation: i32| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO system_states(hostname,change_reason,store_path,generation, \
                 generation_matches_current_store_path) VALUES('runner','startup',$1,$2,true)",
            )
            .bind(store_path)
            .bind(generation)
            .execute(&pool)
            .await
            .unwrap();
        }
    };

    // Historical runtime authority independent of HEAD: the system runs the
    // first commit, so only the historical scan is authoritative.
    run(first_path, 1).await;
    let scans = system_scans(&pool, &caller, json!({"system_id":system})).await;
    assert_eq!(
        scan_ids(&scans),
        [json!(head_new), json!(head_old), json!(historical)]
    );
    assert_eq!(scan_flags(&scans, "is_head"), [true, true, false]);
    assert_eq!(
        scan_flags(&scans, "is_runtime_authoritative"),
        [false, false, true]
    );
    let head = listing(&pool, &caller, json!({"system_id":system})).await;
    assert_eq!(head["provenance"]["scan"]["scan_id"], json!(head_new));
    assert_eq!(head["is_runtime_authoritative"], false);
    let explicit = listing(
        &pool,
        &caller,
        json!({"system_id":system,"scan_id":historical}),
    )
    .await;
    assert_eq!(explicit["is_runtime_authoritative"], true);

    // Positive runtime authority on HEAD: the newest HEAD scan only.
    run(second_path, 2).await;
    let scans = system_scans(&pool, &caller, json!({"system_id":system})).await;
    assert_eq!(
        scan_flags(&scans, "is_runtime_authoritative"),
        [true, false, false]
    );
    let head = listing(&pool, &caller, json!({"system_id":system})).await;
    assert_eq!(head["is_runtime_authoritative"], true);
    assert_eq!(head["latest_sealed_scan"]["scan_id"], json!(head_new));

    // Proven execution reuse: HEAD reuses the build of an earlier commit, so
    // that earlier commit's scan is HEAD evidence.
    let reuse_repo = flake(&pool, "reuse-repo").await;
    let reuse_system = host(&pool, reuse_repo, env, "reuser").await;
    let origin_commit = commit(&pool, reuse_repo, "e").await;
    let head_commit = commit(&pool, reuse_repo, "f").await;
    let reuse_path = "/nix/store/eeee-reused-output";
    let origin = derivation(&pool, origin_commit, "Shared_Config", reuse_path).await;
    let head_drv: i32 = sqlx::query_scalar(
        "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path,\
         expected_store_path,reused_from_derivation_id,status_id) \
         VALUES($1,'nixos','Shared_Config',$2,$3,$4,10) RETURNING id",
    )
    .bind(head_commit)
    .bind(format!("{reuse_path}.drv"))
    .bind(reuse_path)
    .bind(origin)
    .fetch_one(&pool)
    .await
    .unwrap();
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO build_jobs(derivation_id,environment_id,status,completed_at) \
         VALUES($1,$2,'success',NOW()) RETURNING id",
    )
    .bind(origin)
    .bind(env)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE derivations SET reused_build_job_id=$2 WHERE id=$1")
        .bind(head_drv)
        .bind(job)
        .execute(&pool)
        .await
        .unwrap();
    set_head(&pool, reuse_repo, head_commit).await;
    let reused_findings = [(
        "CVE-2099-2004",
        "perl",
        "perl",
        "5.36",
        "/nix/store/p1-perl.drv",
    )];
    let reused = sealed_scan(&pool, origin, at(5), &reused_findings).await;
    let scans = system_scans(&pool, &caller, json!({"system_id":reuse_system})).await;
    assert_eq!(scan_ids(&scans), [json!(reused)]);
    assert_eq!(scans["head_commit_hash"], "f".repeat(40));
    assert_eq!(scans["scans"][0]["commit_hash"], "e".repeat(40));
    assert_eq!(scans["scans"][0]["derivation_id"], origin);
    assert_eq!(scan_flags(&scans, "is_head"), [true]);
    assert_eq!(scan_flags(&scans, "is_runtime_authoritative"), [false]);
    let head = listing(&pool, &caller, json!({"system_id":reuse_system})).await;
    assert_eq!(head["state"], "completed");
    assert_eq!(head["head"]["configuration_derivation_id"], head_drv);
    assert_eq!(head["provenance"]["scan"]["scan_id"], json!(reused));
    assert_eq!(head["latest_sealed_scan"]["is_head"], true);
    assert_eq!(finding_keys(&head), owned(&reused_findings));
}

#[sqlx::test]
async fn system_scans_exclude_ineligible_scans_and_page_through_equal_times(pool: PgPool) {
    let env = environment(&pool, "visible").await;
    let (caller, _) = issue(&pool, AuthRole::Viewer, env, "cves:read").await;
    let repo = flake(&pool, "ties-repo").await;
    let other_repo = flake(&pool, "peer-repo").await;
    let empty_repo = flake(&pool, "empty-repo").await;
    let system = host(&pool, repo, env, "ties").await;
    let peer = host(&pool, other_repo, env, "peer").await;
    let empty = host(&pool, empty_repo, env, "empty").await;
    let main = commit(&pool, repo, "3").await;
    let mock_commit = commit(&pool, repo, "4").await;
    let drv = derivation(&pool, main, "Shared_Config", "/nix/store/3333-ties").await;
    let finding = [(
        "CVE-2099-3001",
        "lz4",
        "lz4",
        "1.9.4",
        "/nix/store/l1-lz4.drv",
    )];

    // Three eligible scans completed at the same instant.
    let mut eligible = Vec::new();
    for _ in 0..3 {
        eligible.push(sealed_scan(&pool, drv, at(0), &finding).await);
    }
    // Ineligible: legacy, pending, failed, mock, other configuration, other
    // flake. Each would be newest if it were listed.
    let legacy = scan(&pool, drv, "in_progress").await;
    sqlx::query("UPDATE cve_scans SET status='completed',completed_at=$2 WHERE id=$1")
        .bind(legacy)
        .bind(at(60))
        .execute(&pool)
        .await
        .unwrap();
    scan(&pool, drv, "pending").await;
    scan(&pool, drv, "failed").await;
    let mock_drv: i32 = sqlx::query_scalar(
        "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path,\
         store_path,status_id,completed_at,policy_results) \
         VALUES($1,'nixos','Shared_Config','/nix/store/4444-mock.drv','/nix/store/4444-mock',\
         10,NOW(),'{\"execution_mode\":\"mock\"}'::jsonb) RETURNING id",
    )
    .bind(mock_commit)
    .fetch_one(&pool)
    .await
    .unwrap();
    sealed_scan(&pool, mock_drv, at(60), &finding).await;
    let other_config = derivation(&pool, main, "Other_Config", "/nix/store/5555-other").await;
    sealed_scan(&pool, other_config, at(60), &finding).await;
    let peer_commit = commit(&pool, other_repo, "6").await;
    let peer_drv = derivation(&pool, peer_commit, "Shared_Config", "/nix/store/6666-peer").await;
    let peer_scan = sealed_scan(&pool, peer_drv, at(60), &finding).await;

    // Equal completion times order by scan ID descending (PostgreSQL compares
    // UUID bytes, which matches the lowercase hex text order).
    let mut expected: Vec<Value> = eligible.iter().map(|id| json!(id)).collect();
    expected.sort_by_key(|id| std::cmp::Reverse(id.as_str().unwrap().to_owned()));
    let first = system_scans(&pool, &caller, json!({"system_id":system,"limit":2})).await;
    assert_eq!(scan_ids(&first), expected[..2]);
    assert_eq!(first["has_more"], true);
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let rest = system_scans(
        &pool,
        &caller,
        json!({"system_id":system,"limit":2,"cursor":cursor}),
    )
    .await;
    assert_eq!(scan_ids(&rest), expected[2..]);
    assert_eq!(rest["has_more"], false);
    assert!(rest["next_cursor"].is_null());
    let findings = listing(&pool, &caller, json!({"system_id":system})).await;
    assert_eq!(findings["state"], "head_not_ready");
    assert_eq!(findings["latest_sealed_scan"]["scan_id"], expected[0]);

    // A cursor is bound to its system, also for another visible system.
    assert!(matches!(
        call_tool(
            &pool,
            &caller,
            "list_system_scans",
            json!({"system_id":peer,"limit":2,"cursor":cursor})
        )
        .await,
        Err(ToolError::InvalidArguments(_))
    ));
    let peer_scans = system_scans(&pool, &caller, json!({"system_id":peer})).await;
    assert_eq!(scan_ids(&peer_scans), [json!(peer_scan)]);

    // Empty history: HEAD is ready but there is no sealed scan.
    let empty_commit = commit(&pool, empty_repo, "7").await;
    derivation(
        &pool,
        empty_commit,
        "Shared_Config",
        "/nix/store/7777-empty",
    )
    .await;
    set_head(&pool, empty_repo, empty_commit).await;
    let none = system_scans(&pool, &caller, json!({"system_id":empty})).await;
    assert_eq!(none["scans"], json!([]));
    assert_eq!(none["has_more"], false);
    assert!(none["next_cursor"].is_null());
    let none = listing(&pool, &caller, json!({"system_id":empty})).await;
    assert_eq!(none["state"], "no_scan_for_head");
    assert!(none["latest_sealed_scan"].is_null());

    // Direct invocation without cves:read is forbidden before any read.
    let (poams_only, _) = issue(&pool, AuthRole::Viewer, env, "poams:read").await;
    assert!(matches!(
        call_tool(
            &pool,
            &poams_only,
            "list_system_scans",
            json!({"system_id":system})
        )
        .await,
        Err(ToolError::Forbidden(_))
    ));
}

async fn rpc(app: &Router, secret: &str, name: &str, args: Value) -> Value {
    let request = Request::post("/mcp")
        .header("authorization", format!("Bearer {secret}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":name,"arguments":args}})
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), mcp::MAX_BODY_BYTES)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    body["result"].clone()
}

fn http_app(pool: &PgPool) -> Router {
    Router::new()
        .route("/mcp", post(mcp::handle))
        .route("/api/v1/poams", get(poam::list))
        .route("/api/v1/poams/:id/close", post(poam::close))
        .route("/api/v1/poams/cves", post(poam::create_cve))
        .route(
            "/api/v1/poams/:id/cve-findings",
            post(poam::link_cve_finding),
        )
        .with_state(CFState::new(
            pool.clone(),
            config(),
            Arc::new(QueueNotifier::new()),
            BackgroundJobRegistry::new(),
        ))
}

#[sqlx::test]
async fn http_discovery_and_findings_enforce_scope_and_current_environment(pool: PgPool) {
    let env = environment(&pool, "visible").await;
    let other = environment(&pool, "hidden").await;
    let repo = flake(&pool, "inventory-repo").await;
    let first = host(&pool, repo, env, "Host-A").await;
    host(&pool, repo, env, "Host-B").await;
    let hidden = host(&pool, repo, other, "Host-C").await;
    let (admitted, secret) = issue(&pool, AuthRole::Operator, env, "cves:read").await;
    let (_, wrong_scope) = issue(&pool, AuthRole::Admin, env, "poams:read").await;
    let app = http_app(&pool);
    let found = rpc(
        &app,
        &secret,
        "find_systems",
        json!({"query":"sHARED_cONFIG"}),
    )
    .await;
    assert_eq!(
        found["structuredContent"]["systems"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let limited = rpc(
        &app,
        &secret,
        "find_systems",
        json!({"query":"Shared_Config","limit":1}),
    )
    .await;
    assert_eq!(
        limited["structuredContent"]["systems"][0]["system_id"],
        json!(first)
    );
    assert_eq!(limited["structuredContent"]["has_more"], true);
    let matched = &limited["structuredContent"]["systems"][0];
    assert_eq!(matched["environment"], json!({"id":env,"name":"visible"}));
    assert_eq!(matched["flake_id"], repo);
    assert!(matched.get("environment_id").is_none());
    assert!(matched.get("environment_name").is_none());
    let hostname = rpc(&app, &secret, "find_systems", json!({"query":"host-a"})).await;
    assert_eq!(
        hostname["structuredContent"]["systems"][0]["system_id"],
        json!(first)
    );
    let success = rpc(
        &app,
        &secret,
        "list_scan_findings",
        json!({"system_id":first}),
    )
    .await;
    assert_eq!(success["structuredContent"]["state"], "head_not_ready");
    let obsolete = rpc(&app, &secret, "find_systems", json!({"name":"host-a"})).await;
    assert_eq!(obsolete["isError"], true);
    let denied = rpc(
        &app,
        &wrong_scope,
        "find_systems",
        json!({"query":"host-a"}),
    )
    .await;
    assert_eq!(denied["isError"], true);
    assert!(
        denied["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("forbidden")
    );
    let hidden_result = rpc(
        &app,
        &secret,
        "list_scan_findings",
        json!({"system_id":hidden,"cursor":"bad"}),
    )
    .await;
    let missing_result = rpc(
        &app,
        &secret,
        "list_scan_findings",
        json!({"system_id":Uuid::new_v4(),"cursor":"bad"}),
    )
    .await;
    assert_eq!(hidden_result, missing_result);
    assert_eq!(hidden_result["isError"], true);
    assert!(
        hidden_result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not_found")
    );

    // Use an already admitted caller: a stale in-memory membership must not
    // authorize reads after the membership or the environment changes.
    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1 AND environment_id=$2")
        .bind(admitted.owner_user_id)
        .bind(env)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        call_tool(
            &pool,
            &admitted,
            "list_scan_findings",
            json!({"system_id":first})
        )
        .await,
        Err(ToolError::NotFound)
    ));
    insert_user_environment_membership(&pool, admitted.owner_user_id, env)
        .await
        .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(first)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        call_tool(
            &pool,
            &admitted,
            "list_scan_findings",
            json!({"system_id":first})
        )
        .await,
        Err(ToolError::NotFound)
    ));
    sqlx::query("UPDATE environments SET is_active=false WHERE id=$1")
        .bind(env)
        .execute(&pool)
        .await
        .unwrap();
    let empty = call_tool(
        &pool,
        &admitted,
        "find_systems",
        json!({"query":"Shared_Config"}),
    )
    .await
    .unwrap();
    assert_eq!(empty["systems"], json!([]));
}

const PLAN_CVE: &str = "CVE-2099-0346";
const PLAN_PACKAGE: &str = "fixture-pkg";
const PLAN_STORE_PATH: &str = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-planning-head";

async fn external_planning_host(pool: &PgPool) -> (Uuid, Uuid, i32, i32) {
    let env = environment(pool, "planning").await;
    let repo = flake(pool, "planning-repo").await;
    let system = host(pool, repo, env, "planning-host").await;
    sqlx::query("UPDATE systems SET management_mode='external' WHERE id=$1")
        .bind(system)
        .execute(pool)
        .await
        .unwrap();
    let head = commit(pool, repo, "a").await;
    let drv = derivation(pool, head, "Shared_Config", PLAN_STORE_PATH).await;
    set_head(pool, repo, head).await;
    sqlx::query("INSERT INTO cves(id,cvss_v3_score) VALUES($1,7.5)")
        .bind(PLAN_CVE)
        .execute(pool)
        .await
        .unwrap();
    (env, system, head, drv)
}

async fn planning_scan(pool: &PgPool, drv: i32, version: &str, path: &str) -> Uuid {
    let id = scan(pool, drv, "in_progress").await;
    sqlx::query(
        "INSERT INTO cve_scan_vulnerability_observations(scan_id,canonical_cve_id,canonical_package_name,\
         observed_package_name,observed_package_version,observed_derivation_path,is_whitelisted,detection_method) \
         VALUES($1,$2,$3,$3,$4,$5,false,'test')",
    )
    .bind(id)
    .bind(PLAN_CVE)
    .bind(PLAN_PACKAGE)
    .bind(version)
    .bind(path)
    .execute(pool)
    .await
    .unwrap();
    seal(pool, id).await;
    id
}

fn plan_args(system: Uuid, scan_id: Uuid) -> Value {
    json!({"system_id":system,"cve_id":PLAN_CVE,"canonical_package_name":PLAN_PACKAGE,
           "expected_scan_id":scan_id,"title":"Plan package upgrade","plan":"Review update"})
}

fn tool_success(reply: &Value) -> &Value {
    assert!(reply["structuredContent"].is_object(), "{reply}");
    assert_ne!(reply["isError"], true, "{reply}");
    &reply["structuredContent"]
}

fn assert_tool_error(reply: &Value, code: &str) {
    assert_eq!(reply["isError"], true, "{reply}");
    assert!(
        reply["content"][0]["text"].as_str().unwrap().contains(code),
        "expected {code}: {reply}"
    );
}

#[sqlx::test]
async fn http_head_plan_keeps_its_baseline_and_requires_mcp_runtime_handoff(pool: PgPool) {
    let (env, system, head, drv) = external_planning_host(&pool).await;
    let first_scan = planning_scan(&pool, drv, "1", "/fixture-package-v1.drv").await;
    let (creator, create_token) = issue(&pool, AuthRole::Operator, env, "poams:create").await;
    let (writer, progress_token) = issue_scopes(
        &pool,
        AuthRole::Operator,
        env,
        &["cves:read", "poams:read", "poams:write", "poams:verify"],
    )
    .await;
    let app = http_app(&pool);

    let created = rpc(
        &app,
        &create_token,
        "create_cve_poam",
        plan_args(system, first_scan),
    )
    .await;
    let created = tool_success(&created);
    let id: Uuid = serde_json::from_value(created["poam_id"].clone()).unwrap();
    assert_eq!(created["created"], true);
    assert_eq!(created["evidence_kind"], "configuration_scan");
    assert_eq!(created["baseline_scan_id"], json!(first_scan));
    assert_eq!(created["revision"], 1);
    assert_eq!(created["status"], "open");

    let detail = || {
        rpc(
            &app,
            &progress_token,
            "get_poam_detail",
            json!({"poam_id":id}),
        )
    };
    let initial = detail().await;
    let initial = tool_success(&initial);
    assert_eq!(initial["visibility"], "full");
    assert_eq!(initial["poam"]["finding_count"], 0);
    assert_eq!(initial["poam"]["cve_finding_count"], 1);
    assert_eq!(initial["poam"]["configuration_planning_only"], true);
    let baseline = &initial["poam"]["configuration_cve_finding"];
    assert_eq!(baseline["system_id"], json!(system));
    assert_eq!(baseline["canonical_cve_id"], PLAN_CVE);
    assert_eq!(baseline["canonical_package_name"], PLAN_PACKAGE);
    assert_eq!(baseline["baseline_head_commit_id"], head);
    assert_eq!(baseline["baseline_head_commit_hash"], "a".repeat(40));
    assert_eq!(baseline["baseline_configuration_derivation_id"], drv);
    assert_eq!(baseline["baseline_scan_id"], json!(first_scan));
    assert_eq!(baseline["baseline_scan_derivation_id"], drv);
    assert_eq!(baseline["baseline_scanned_store_path"], PLAN_STORE_PATH);
    assert_eq!(
        baseline["baseline_occurrence_derivation_path"],
        "/fixture-package-v1.drv"
    );
    assert_eq!(baseline["baseline_observed_package_version"], "1");
    assert_eq!(initial["poam"]["cve_findings"], json!([]));

    let second_scan = planning_scan(&pool, drv, "2", "/fixture-package-v2.drv").await;
    let inventory = rpc(
        &app,
        &progress_token,
        "list_scan_findings",
        json!({"system_id":system}),
    )
    .await;
    let inventory = tool_success(&inventory);
    assert_eq!(
        inventory["provenance"]["scan"]["scan_id"],
        json!(second_scan)
    );
    assert_eq!(inventory["is_runtime_authoritative"], false);
    let mut retry = plan_args(system, second_scan);
    retry["title"] = json!("A retry must not rewrite the plan");
    retry["plan"] = json!("Replacement text");
    let reused = rpc(&app, &create_token, "create_cve_poam", retry).await;
    let mut expected = created.clone();
    expected["created"] = json!(false);
    assert_eq!(tool_success(&reused), &expected);
    let after_rescan = detail().await;
    assert_eq!(tool_success(&after_rescan), initial);

    let note = rpc(
        &app,
        &progress_token,
        "update_poam_progress",
        json!({"poam_id":id,"revision":1,"action":{"type":"add_note","text":"Reviewed configuration update"}}),
    )
    .await;
    assert_eq!(tool_success(&note)["revision"], 2);
    let progress = detail().await;
    let progress = tool_success(&progress);
    let activity = progress["poam"]["activity"].as_array().unwrap();
    assert_eq!(activity.len(), 2);
    for (kind, actor) in [("created", &creator), ("note", &writer)] {
        let entry = activity.iter().find(|row| row["kind"] == kind).unwrap();
        assert_eq!(entry["actor_user_id"], json!(actor.owner_user_id));
        assert_eq!(entry["access_token_id"], json!(actor.token_id));
        assert_eq!(entry["via_agent"], true);
        assert_eq!(entry["access_token_name"], "Scan inventory test");
        if kind == "note" {
            assert_eq!(entry["payload"]["text"], "Reviewed configuration update");
        }
    }

    for (tool, args) in [
        ("verify_poam", json!({"poam_id":id,"revision":2})),
        (
            "update_poam_progress",
            json!({"poam_id":id,"revision":2,
            "action":{"type":"set_status","status":"awaiting_verification"}}),
        ),
    ] {
        assert_tool_error(
            &rpc(&app, &progress_token, tool, args).await,
            "configuration_planning_only",
        );
    }

    // Exercise the actual closure route with a valid browser session and CSRF.
    let session = Uuid::new_v4().to_string();
    create_user_session(
        &pool,
        writer.owner_user_id,
        hash_token(&session),
        Utc::now() + chrono::Duration::hours(1),
        None,
        None,
        "local".into(),
    )
    .await
    .unwrap();
    let register = || async {
        let request = Request::get("/api/v1/poams")
            .header("cookie", format!("{SESSION_COOKIE_NAME}={session}"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), mcp::MAX_BODY_BYTES)
            .await
            .unwrap();
        let page: Value = serde_json::from_slice(&bytes).unwrap();
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == json!(id))
            .unwrap()
            .clone()
    };
    let registered = register().await;
    assert_eq!(registered["finding_count"], 0);
    assert_eq!(registered["cve_finding_count"], 1);
    assert_eq!(registered["configuration_planning_only"], true);
    let close = Request::post(format!("/api/v1/poams/{id}/close"))
        .header(
            "cookie",
            format!("{SESSION_COOKIE_NAME}={session}; {CSRF_COOKIE_NAME}=fixture-csrf"),
        )
        .header(CSRF_HEADER_NAME.as_str(), "fixture-csrf")
        .header("content-type", "application/json")
        .body(Body::from(json!({"revision":2}).to_string()))
        .unwrap();
    let response = app.clone().oneshot(close).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), mcp::MAX_BODY_BYTES)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "configuration_planning_only");
    let after_denials = detail().await;
    assert_eq!(tool_success(&after_denials), progress);
    assert_eq!(progress["poam"]["configuration_cve_finding"], *baseline);
    assert_eq!(progress["poam"]["status"], "open");
    assert_eq!(progress["poam"]["verification_attempts"], json!([]));

    let audits: Vec<(String, Option<Uuid>, Option<Uuid>, Value)> = sqlx::query_as(
        "SELECT action,actor_user_id,access_token_id,metadata FROM admin_audit_events \
         WHERE target=$1 ORDER BY action",
    )
    .bind(format!("poam:{id}"))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(audits.len(), 2);
    for ((action, user, token, metadata), (expected_action, actor)) in audits
        .iter()
        .zip([("poam_created", &creator), ("poam_note", &writer)])
    {
        assert_eq!(action, expected_action);
        assert_eq!(*user, Some(actor.owner_user_id));
        assert_eq!(*token, Some(actor.token_id));
        assert_eq!(metadata["via"], "mcp");
        assert_eq!(metadata["access_token_id"], json!(actor.token_id));
        assert_eq!(metadata["access_token_name"], "Scan inventory test");
    }

    sqlx::query(
        "UPDATE systems SET management_mode='managed',public_key='fixture-key' WHERE id=$1",
    )
    .bind(system)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO system_states(hostname,change_reason,store_path,generation, \
         generation_matches_current_store_path) VALUES('planning-host','startup',$1,1,true)",
    )
    .bind(PLAN_STORE_PATH)
    .execute(&pool)
    .await
    .unwrap();

    // The new authority cannot promote by itself or through browser link/create.
    let observation = json!({"system_id":system,"scan_id":second_scan,
        "occurrence_derivation_path":"/fixture-package-v2.drv",
        "canonical_cve_id":PLAN_CVE,"canonical_package_name":PLAN_PACKAGE});
    for (path, body, expected_error) in [
        (
            format!("/api/v1/poams/{id}/cve-findings"),
            json!({"revision":2,"observation":observation}),
            "incompatible_finding",
        ),
        (
            "/api/v1/poams/cves".to_owned(),
            json!({"title":"Duplicate","risk":"medium","observation":observation}),
            "configuration_plan_exists",
        ),
    ] {
        let request = Request::post(path)
            .header(
                "cookie",
                format!("{SESSION_COOKIE_NAME}={session}; {CSRF_COOKIE_NAME}=fixture-csrf"),
            )
            .header(CSRF_HEADER_NAME.as_str(), "fixture-csrf")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let bytes = to_bytes(response.into_body(), mcp::MAX_BODY_BYTES)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"], expected_error, "{body}");
    }
    assert_tool_error(
        &rpc(
            &app,
            &progress_token,
            "verify_poam",
            json!({"poam_id":id,"revision":2}),
        )
        .await,
        "configuration_planning_only",
    );

    let promote = || {
        rpc(
            &app,
            &create_token,
            "create_cve_poam",
            plan_args(system, first_scan),
        )
    };
    sqlx::query(
        "INSERT INTO system_cve_justifications(system_id,cve_id,category,reason,updated_by,updated_at) \
         VALUES($1,$2,'accepted_risk','test',$3,now())",
    )
    .bind(system)
    .bind(PLAN_CVE)
    .bind(writer.owner_user_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_tool_error(&promote().await, "cve_occurrence_justified");
    assert_eq!(tool_success(&detail().await), progress);
    sqlx::query("DELETE FROM system_cve_justifications WHERE system_id=$1 AND cve_id=$2")
        .bind(system)
        .bind(PLAN_CVE)
        .execute(&pool)
        .await
        .unwrap();

    let (promoted, retry) = tokio::join!(promote(), promote());
    let promoted = tool_success(&promoted);
    assert_eq!(promoted, tool_success(&retry));
    assert_eq!(promoted["poam_id"], json!(id));
    assert_eq!(promoted["created"], false);
    assert_eq!(promoted["revision"], 3);
    assert_eq!(promoted["status"], "open");
    assert_eq!(promoted["evidence_kind"], "runtime_cve");
    assert_eq!(promoted["baseline_scan_id"], json!(second_scan));
    let after = detail().await;
    let after = &tool_success(&after)["poam"];
    assert_eq!(after["configuration_cve_finding"], *baseline);
    assert_eq!(after["cve_findings"].as_array().unwrap().len(), 1);
    assert_eq!(after["finding_count"], 0);
    assert_eq!(after["cve_finding_count"], 1);
    assert_eq!(after["configuration_planning_only"], false);
    let registered = register().await;
    assert_eq!(registered["finding_count"], 0);
    assert_eq!(registered["cve_finding_count"], 1);
    assert_eq!(registered["configuration_planning_only"], false);
    let activity = after["activity"].as_array().unwrap();
    assert_eq!(activity.len(), 3);
    let handoff = activity
        .iter()
        .find(|row| row["kind"] == "cve_finding_linked")
        .unwrap();
    assert_eq!(handoff["access_token_id"], json!(creator.token_id));
    assert_eq!(handoff["payload"]["configuration_handoff"], true);
    assert_eq!(handoff["payload"]["observation"], observation);

    let awaiting = rpc(
        &app,
        &progress_token,
        "update_poam_progress",
        json!({"poam_id":id,"revision":3,"action":{"type":"set_status","status":"awaiting_verification"}}),
    )
    .await;
    assert_eq!(tool_success(&awaiting)["revision"], 4);
    let verified = rpc(
        &app,
        &progress_token,
        "verify_poam",
        json!({"poam_id":id,"revision":4}),
    )
    .await;
    assert_eq!(tool_success(&verified)["outcome"], "rejected");
}

#[sqlx::test]
async fn http_head_plan_create_is_scoped_and_deduplicates_concurrent_requests(pool: PgPool) {
    let (env, system, _, drv) = external_planning_host(&pool).await;
    let stale = planning_scan(&pool, drv, "1", "/fixture-package-v1.drv").await;
    let current = planning_scan(&pool, drv, "2", "/fixture-package-v2.drv").await;
    let other = environment(&pool, "outside-token-grant").await;
    let (_, creator) = issue(&pool, AuthRole::Operator, env, "poams:create").await;
    let (_, wrong_scope) = issue(&pool, AuthRole::Operator, env, "poams:write").await;
    let (_, outside) = issue_scopes(
        &pool,
        AuthRole::Admin,
        other,
        &["poams:create", "poams:read", "poams:write", "poams:verify"],
    )
    .await;
    let app = http_app(&pool);
    let create = |token, args| rpc(&app, token, "create_cve_poam", args);
    let args = plan_args(system, current);
    assert_tool_error(&create(&wrong_scope, args.clone()).await, "forbidden");
    let unknown = create(&outside, plan_args(Uuid::new_v4(), current)).await;
    assert_tool_error(&unknown, "not_found");
    assert_eq!(create(&outside, args.clone()).await, unknown);
    assert_tool_error(
        &create(&creator, plan_args(system, stale)).await,
        "head_finding_not_current",
    );

    // Poll both HTTP requests together so they can contend on the same identity.
    let (left, right) = tokio::join!(
        create(&creator, args.clone()),
        create(&creator, args.clone())
    );
    let left = tool_success(&left);
    let right = tool_success(&right);
    assert_eq!(left["poam_id"], right["poam_id"]);
    assert_eq!(left["baseline_scan_id"], json!(current));
    assert_eq!(right["baseline_scan_id"], json!(current));
    assert_ne!(
        left["created"].as_bool().unwrap(),
        right["created"].as_bool().unwrap()
    );
    assert_eq!(create(&outside, args).await, unknown);

    let id = &left["poam_id"];
    for (tool, args) in [
        ("get_poam_detail", json!({"poam_id":id})),
        ("verify_poam", json!({"poam_id":id,"revision":1})),
        (
            "update_poam_progress",
            json!({"poam_id":id,"revision":1,
            "action":{"type":"add_note","text":"Outside the token grant"}}),
        ),
    ] {
        let mut absent = args.clone();
        absent["poam_id"] = json!(Uuid::new_v4());
        let missing = rpc(&app, &outside, tool, absent).await;
        assert_tool_error(&missing, "not_found");
        assert_eq!(rpc(&app, &outside, tool, args).await, missing);
    }
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM poams), \
                (SELECT COUNT(*) FROM poam_configuration_cve_findings), \
                (SELECT COUNT(*) FROM poam_cve_finding_links), \
                (SELECT COUNT(*) FROM poam_activity)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1, 0, 1));
}
