//! Migration-backed checks for scoped configuration scan inventory. The HTTP
//! test drives the production route in-process without opening a socket.

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::post,
};
use chrono::Utc;
use crystal_forge::{
    api::models::CreateAccessTokenRequest,
    config::ServerConfig,
    handlers::agent_request::CFState,
    mcp::{
        self,
        tools::{ToolCaller, ToolError, call_tool},
    },
    models::auth_identity::AuthRole,
    queries::{
        auth_identity::{insert_user_environment_membership, sync_user_role},
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
        scopes: Some(vec![scope.into()]),
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
    let app = Router::new()
        .route("/mcp", post(mcp::handle))
        .with_state(CFState::new(
            pool.clone(),
            config(),
            Arc::new(QueueNotifier::new()),
            BackgroundJobRegistry::new(),
        ));
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
