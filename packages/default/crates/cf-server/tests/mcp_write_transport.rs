//! MCP write tools over the real `/mcp` HTTP path (TASK-534.5).
//!
//! Each SQLx test owns an isolated database with the full production
//! migrations. The tests mount the real `/mcp` handler and the Admin Audit Log
//! route on the same paths as `bin/server.rs`, issue tokens through the
//! access-token service and speak JSON-RPC over HTTP. They prove the
//! end-to-end contract that the direct tool tests in `mcp_write_poam` and
//! `mcp_write_scan` cannot: `tools/list` filtering, `tools/call` result
//! envelopes, transport authentication between calls, the `mcp_write_denied`
//! audit row that only the transport writes, and the agent marker in the
//! Audit Log after a token is deleted.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use chrono::Utc;
use crystal_forge::api::models::CreateAccessTokenRequest;
use crystal_forge::auth::authorization::CredentialIdentity;
use crystal_forge::auth::session::{SESSION_COOKIE_NAME, hash_token};
use crystal_forge::handlers::{agent_request::CFState, api::admin};
use crystal_forge::mcp;
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::models::builders::CreateBuilderRequest;
use crystal_forge::models::poam::{CreateCvePoamRequest, CveObservationReference, PoamRisk};
use crystal_forge::queries::auth_identity::{
    create_user_session, insert_user_environment_membership, sync_user_role,
};
use crystal_forge::queries::builders::{create_builder, establish_builder_session};
use crystal_forge::queries::cve_scan_leases::record_session_cve_capabilities;
use crystal_forge::queries::users::insert_user;
use crystal_forge::services::access_tokens::{self as token_service, AccessTokenSettings};
use crystal_forge::services::poam::{self as poam_service, PoamActor, SystemClock};
use crystal_forge::{
    config::ServerConfig, queue::QueueNotifier, server::jobs::BackgroundJobRegistry,
};
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const READ: &[&str] = &["cves:read", "poams:read"];
const WRITE: &[&str] = &["cves:read", "poams:read", "poams:write"];
const WRITE_VERIFY: &[&str] = &["cves:read", "poams:read", "poams:write", "poams:verify"];
const SCAN: &[&str] = &["cves:read", "scans:request"];
const ALL_WRITE: &[&str] = &[
    "cves:read",
    "poams:read",
    "scans:request",
    "poams:write",
    "poams:verify",
];

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn enabled_config() -> ServerConfig {
    ServerConfig {
        mcp_enable: true,
        mcp_public_url: Some("https://forge.example.com/mcp".to_owned()),
        ..ServerConfig::default()
    }
}

fn settings() -> AccessTokenSettings {
    AccessTokenSettings {
        enabled: true,
        default_lifetime_days: 30,
        max_lifetime_days: 90,
        entitlement_max_age_seconds: 86_400,
        mcp_enabled: true,
        mcp_public_url: None,
    }
}

async fn environment(pool: &PgPool) -> Uuid {
    let short = Uuid::new_v4().simple().to_string();
    sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(format!("mcp-http-{}", &short[..8]))
        .fetch_one(pool)
        .await
        .expect("insert environment")
}

/// Creates a local user with one role, the given memberships and a live
/// browser session; returns the user id and the plaintext session value.
async fn user(pool: &PgPool, role: AuthRole, environments: &[Uuid]) -> (Uuid, String) {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("mcp-http-{suffix}@example.invalid"),
        Some("MCP HTTP Test"),
    )
    .await
    .expect("insert user");
    sync_user_role(pool, user.id, role)
        .await
        .expect("assign role");
    for environment in environments {
        insert_user_environment_membership(pool, user.id, *environment)
            .await
            .expect("assign membership");
    }
    let session = format!("mcp-http-session-{suffix}");
    create_user_session(
        pool,
        user.id,
        hash_token(&session),
        Utc::now() + chrono::Duration::hours(1),
        Some("mcp-write-transport-test".into()),
        Some("127.0.0.1".into()),
        "local".into(),
    )
    .await
    .expect("create session");
    (user.id, session)
}

/// One issued token with its plaintext bearer secret.
struct Token {
    id: Uuid,
    name: String,
    secret: String,
}

async fn issue(pool: &PgPool, owner: Uuid, scopes: &[&str], environments: &[Uuid]) -> Token {
    let name = format!("agent-{}", &Uuid::new_v4().simple().to_string()[..8]);
    let request: CreateAccessTokenRequest = serde_json::from_value(json!({
        "name": name,
        "audience": "mcp",
        "expires_in_days": 30,
        "environment_ids": environments.iter().map(Uuid::to_string).collect::<Vec<_>>(),
        "scopes": scopes,
    }))
    .expect("token request");
    let issued = token_service::create(pool, owner, &request, &settings())
        .await
        .expect("issue token");
    Token {
        id: issued.metadata.id,
        name,
        secret: issued.secret.expose().to_owned(),
    }
}

/// Creates a managed system with a running report and one sealed scan that
/// finds `CVE-2099-0801` in `pkg`; returns the observation reference.
async fn scanned_host(pool: &PgPool, environment_id: Uuid) -> CveObservationReference {
    let suffix = Uuid::new_v4().simple().to_string();
    let hostname = format!("mcp-http-{suffix}");
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes(name,repo_url) VALUES($1,$2) RETURNING id")
            .bind(format!("mcp-http-flake-{suffix}"))
            .bind(format!("https://example.invalid/{suffix}.git"))
            .fetch_one(pool)
            .await
            .expect("insert flake");
    let commit: i32 = sqlx::query_scalar(
        "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES($1,$2,now()) RETURNING id",
    )
    .bind(flake)
    .bind(format!("{suffix}{}", &suffix[..8]))
    .fetch_one(pool)
    .await
    .expect("insert commit");
    let system_id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,is_active,public_key,derivation,flake_id,system_configuration_name,environment_id) \
         VALUES($1,true,$2,'test-derivation',$3,$1,$4) RETURNING id",
    )
    .bind(&hostname)
    .bind(format!("mcp-http-key-{suffix}"))
    .bind(flake)
    .bind(environment_id)
    .fetch_one(pool)
    .await
    .expect("insert system");
    let store_path = format!("/nix/store/{suffix}-mcp-http-system");
    let derivation_id: i32 = sqlx::query_scalar(
        "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path, \
         status_id,completed_at,store_path) VALUES($1,'nixos',$2,$3,10,now(),$4) RETURNING id",
    )
    .bind(commit)
    .bind(&hostname)
    .bind(format!("{store_path}.drv"))
    .bind(&store_path)
    .fetch_one(pool)
    .await
    .expect("insert system derivation");
    sqlx::query(
        "INSERT INTO system_states(hostname,change_reason,store_path,generation, \
         generation_matches_current_store_path) VALUES($1,'startup',$2,1,true)",
    )
    .bind(&hostname)
    .bind(&store_path)
    .execute(pool)
    .await
    .expect("insert running report");
    let scan_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO cve_scans(id,derivation_id,scanner_name,status) VALUES($1,$2,'mcp-test','in_progress')",
    )
    .bind(scan_id)
    .bind(derivation_id)
    .execute(pool)
    .await
    .expect("insert scan");
    sqlx::query(
        "INSERT INTO cves(id,cvss_v3_score,description) VALUES('CVE-2099-0801',5.0,'HTTP write fixture') \
         ON CONFLICT(id) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("insert CVE");
    let path = format!("/nix/store/{}-pkg.drv", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO cve_scan_vulnerability_observations( \
         scan_id,canonical_cve_id,canonical_package_name,observed_package_name, \
         observed_package_version,observed_derivation_path,is_whitelisted,detection_method) \
         VALUES($1,'CVE-2099-0801','pkg','pkg','1.0',$2,false,'mcp-test')",
    )
    .bind(scan_id)
    .bind(&path)
    .execute(pool)
    .await
    .expect("insert observation");
    sqlx::query(
        "UPDATE cve_scans SET status='completed',completed_at=now(),evidence_schema_version=1 WHERE id=$1",
    )
    .bind(scan_id)
    .execute(pool)
    .await
    .expect("seal scan");
    CveObservationReference {
        system_id,
        scan_id,
        occurrence_derivation_path: path,
        canonical_cve_id: "CVE-2099-0801".into(),
        canonical_package_name: "pkg".into(),
    }
}

/// Creates an open CVE POA&M in `environment_id` as a browser Admin and
/// returns its id and revision.
async fn open_poam(pool: &PgPool, environment_id: Uuid) -> (Uuid, i64) {
    let observation = scanned_host(pool, environment_id).await;
    let (admin_id, _) = user(pool, AuthRole::Admin, &[]).await;
    let actor = PoamActor {
        credential: CredentialIdentity::BrowserSession,
        user_id: admin_id,
        identifier: format!("mcp-http-admin-{admin_id}@example.invalid"),
        is_admin: true,
        can_mutate: true,
        environment_ids: Vec::new(),
        request_origin: None,
    };
    let detail = poam_service::create_cve(
        pool,
        &actor,
        CreateCvePoamRequest {
            observation,
            title: "Upgrade vulnerable package".into(),
            plan: "Update the nixpkgs input".into(),
            owner: "MCP owner".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: Vec::new(),
        },
        &SystemClock,
    )
    .await
    .expect("create CVE POA&M");
    (detail.poam.id, detail.poam.revision)
}

/// One Reference registration with a scannable configuration artifact.
struct Host {
    system_id: Uuid,
    derivation_id: i32,
}

/// Creates a Reference system whose branch-head NixOS build was produced by
/// a CVE-capable builder session and published to a cache. The admission
/// queues a `pending` scan for that producer and never starts the
/// server-local vulnix runner.
async fn scan_host(pool: &PgPool, environment_id: Uuid) -> Host {
    let suffix = Uuid::new_v4().simple().to_string();
    let hostname = format!("mcp-http-scan-{suffix}");
    let flake: i32 = sqlx::query_scalar(
        "INSERT INTO flakes(name,repo_url,snapshot_ready_at) VALUES($1,$2,now()) RETURNING id",
    )
    .bind(format!("mcp-http-scan-flake-{suffix}"))
    .bind(format!("https://example.invalid/{suffix}.git"))
    .fetch_one(pool)
    .await
    .expect("insert flake");
    let commit: i32 = sqlx::query_scalar(
        "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES($1,$2,now()) RETURNING id",
    )
    .bind(flake)
    .bind(format!("{suffix}{}", &suffix[..8]))
    .fetch_one(pool)
    .await
    .expect("insert commit");
    sqlx::query(
        "INSERT INTO flake_branch_commit_snapshot(flake_id,commit_id,position) VALUES($1,$2,0)",
    )
    .bind(flake)
    .bind(commit)
    .execute(pool)
    .await
    .expect("insert branch head");
    let system_id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,is_active,public_key,derivation,flake_id,system_configuration_name, \
         environment_id,management_mode) VALUES($1,true,NULL,'test-derivation',$2,$1,$3,'reference') RETURNING id",
    )
    .bind(&hostname)
    .bind(flake)
    .bind(environment_id)
    .fetch_one(pool)
    .await
    .expect("insert system");
    let store_path = format!("/nix/store/{suffix}-mcp-http-scan");
    let derivation_id: i32 = sqlx::query_scalar(
        "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path, \
         status_id,completed_at,store_path) VALUES($1,'nixos',$2,$3,10,now(),$4) RETURNING id",
    )
    .bind(commit)
    .bind(&hostname)
    .bind(format!("{store_path}.drv"))
    .bind(&store_path)
    .fetch_one(pool)
    .await
    .expect("insert derivation");
    let (builder, _) = create_builder(
        pool,
        &CreateBuilderRequest {
            name: format!("mcp-http-producer-{suffix}"),
            host: None,
            arch: "x86_64-linux".into(),
            public_key: None,
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: Some(1),
            enabled: Some(true),
            environment_ids: vec![environment_id],
        },
    )
    .await
    .expect("create builder");
    let session = Uuid::new_v4();
    establish_builder_session(pool, &builder.id, &session, 60, "mcp http test")
        .await
        .expect("establish builder session");
    assert!(
        record_session_cve_capabilities(
            pool,
            builder.id,
            session,
            cf_protocol::builder::BuilderCapabilities::current_cve_scanner("vulnix test".into()),
        )
        .await
        .expect("record scanner capability"),
        "the builder session must accept CVE work"
    );
    sqlx::query(
        "INSERT INTO build_jobs(derivation_id,environment_id,builder_id,builder_session_id,status,completed_at) \
         VALUES($1,$2,$3,$4,'success',NOW())",
    )
    .bind(derivation_id)
    .bind(environment_id)
    .bind(builder.id)
    .bind(session)
    .execute(pool)
    .await
    .expect("insert producing build job");
    sqlx::query(
        "INSERT INTO cache_push_jobs(derivation_id,store_path,status,completed_at,cache_destination) \
         VALUES($1,$2,'completed',NOW(),'test-cache')",
    )
    .bind(derivation_id)
    .bind(&store_path)
    .execute(pool)
    .await
    .expect("insert cache publication");
    Host {
        system_id,
        derivation_id,
    }
}

async fn current_revision(pool: &PgPool, poam_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT revision FROM poams WHERE id=$1")
        .bind(poam_id)
        .fetch_one(pool)
        .await
        .expect("POA&M revision")
}

async fn audit_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit_events")
        .fetch_one(pool)
        .await
        .expect("count audit events")
}

/// Returns `(actor_user_id, metadata)` of every `mcp_write_denied` row of
/// one token, oldest first.
async fn denied_rows(pool: &PgPool, token_id: Uuid) -> Vec<(Option<Uuid>, Value)> {
    sqlx::query_as(
        "SELECT actor_user_id, metadata FROM admin_audit_events \
         WHERE action='mcp_write_denied' AND access_token_id=$1 ORDER BY created_at",
    )
    .bind(token_id)
    .fetch_all(pool)
    .await
    .expect("read denied audit events")
}

/// Asserts one denied row: the owner is the actor and the metadata holds only
/// the tool name, the reason code and the token name, never the arguments.
fn assert_denied(
    row: &(Option<Uuid>, Value),
    owner: Uuid,
    token: &Token,
    tool: &str,
    reason: &str,
) {
    assert_eq!(row.0, Some(owner));
    assert_eq!(
        row.1,
        json!({ "tool": tool, "reason": reason, "access_token_name": token.name }),
        "the denied audit metadata must not contain tool arguments"
    );
}

// ---------------------------------------------------------------------------
// HTTP harness
// ---------------------------------------------------------------------------

struct Reply {
    status: u16,
    text: String,
    body: Value,
}

struct Server {
    base: String,
    client: Client,
}

async fn start(pool: &PgPool) -> Server {
    let state = CFState::new(
        pool.clone(),
        enabled_config(),
        Arc::new(QueueNotifier::new()),
        BackgroundJobRegistry::new(),
    );
    let app = Router::new()
        .route(
            "/mcp",
            post(mcp::handle).layer(DefaultBodyLimit::max(mcp::MAX_BODY_BYTES)),
        )
        .route("/api/v1/admin/audit-events", get(admin::list_audit_events))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind MCP test server");
    let address = listener.local_addr().expect("read test server address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve MCP test API")
    });
    Server {
        base: format!("http://{address}"),
        client: Client::new(),
    }
}

async fn send(builder: reqwest::RequestBuilder) -> Reply {
    let response = builder.send().await.expect("send request");
    let status = response.status().as_u16();
    let text = response.text().await.expect("read response body");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply { status, text, body }
}

impl Server {
    async fn rpc(&self, token: &Token, method: &str, params: Value) -> Reply {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        send(
            self.client
                .post(format!("{}/mcp", self.base))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token.secret))
                .body(body.to_string()),
        )
        .await
    }

    async fn call(&self, token: &Token, tool: &str, arguments: Value) -> Reply {
        self.rpc(
            token,
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        )
        .await
    }

    async fn audit_events(&self, admin_session: &str) -> Vec<Value> {
        let reply = send(
            self.client
                .get(format!(
                    "{}/api/v1/admin/audit-events?per_page=200",
                    self.base
                ))
                .header("cookie", format!("{SESSION_COOKIE_NAME}={admin_session}")),
        )
        .await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        reply.body["items"].as_array().expect("audit items").clone()
    }
}

fn tool_names(reply: &Reply) -> BTreeSet<String> {
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.body["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name").to_owned())
        .collect()
}

/// Returns `structuredContent` of a successful tool call.
fn structured(reply: &Reply) -> &Value {
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["result"]["isError"], false, "{}", reply.text);
    &reply.body["result"]["structuredContent"]
}

/// Returns the `(error, message)` pair of a tool call that reports `isError`.
fn tool_error(reply: &Reply) -> (String, String) {
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["result"]["isError"], true, "{}", reply.text);
    assert!(
        reply.body["result"].get("structuredContent").is_none(),
        "{}",
        reply.text
    );
    let text = reply.body["result"]["content"][0]["text"]
        .as_str()
        .expect("error text");
    let error: Value = serde_json::from_str(text).expect("error JSON");
    (
        error["error"].as_str().expect("error code").to_owned(),
        error["message"].as_str().expect("error message").to_owned(),
    )
}

fn note(poam_id: Uuid, revision: i64, text: &str) -> Value {
    json!({
        "poam_id": poam_id,
        "revision": revision,
        "action": { "type": "add_note", "text": text },
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn tools_list_shows_write_tools_only_with_write_scopes(pool: PgPool) {
    let env = environment(&pool).await;
    let (owner, _) = user(&pool, AuthRole::Operator, &[env]).await;
    let server = start(&pool).await;
    let read_tools = BTreeSet::from([
        "find_systems".to_owned(),
        "get_poam_detail".to_owned(),
        "get_scan_result".to_owned(),
        "get_system_security_context".to_owned(),
        "list_remediation_work".to_owned(),
        "list_scan_findings".to_owned(),
    ]);

    let read_only = issue(&pool, owner, READ, &[env]).await;
    let listed = tool_names(&server.rpc(&read_only, "tools/list", json!({})).await);
    assert_eq!(listed, read_tools);

    let writer = issue(&pool, owner, ALL_WRITE, &[env]).await;
    let listed = tool_names(&server.rpc(&writer, "tools/list", json!({})).await);
    let mut expected = read_tools;
    expected.extend(["request_cve_scan", "update_poam_progress", "verify_poam"].map(str::to_owned));
    assert_eq!(listed, expected);
}

#[sqlx::test]
async fn update_poam_progress_returns_the_new_revision_and_rejects_a_stale_one(pool: PgPool) {
    let env = environment(&pool).await;
    let (owner, _) = user(&pool, AuthRole::Operator, &[env]).await;
    let token = issue(&pool, owner, WRITE, &[env]).await;
    let (poam_id, revision) = open_poam(&pool, env).await;
    let server = start(&pool).await;

    let reply = server
        .call(
            &token,
            "update_poam_progress",
            note(poam_id, revision, "Bumped nixpkgs"),
        )
        .await;
    let result = structured(&reply);
    assert_eq!(result["poam_id"], json!(poam_id));
    assert_eq!(result["revision"], revision + 1);
    assert_eq!(result["status"], "open");
    assert_eq!(result["changed"]["text"], "Bumped nixpkgs");

    let stale = server
        .call(
            &token,
            "update_poam_progress",
            note(poam_id, revision, "Again"),
        )
        .await;
    let (code, message) = tool_error(&stale);
    assert_eq!(code, "invalid_arguments");
    assert!(message.starts_with("stale_revision:"), "{message}");
    assert_eq!(current_revision(&pool, poam_id).await, revision + 1);
    // A stale revision is not an authorization denial.
    assert!(denied_rows(&pool, token.id).await.is_empty());
}

#[sqlx::test]
async fn request_cve_scan_is_accepted_and_readable_with_get_scan_result(pool: PgPool) {
    let env = environment(&pool).await;
    let (owner, _) = user(&pool, AuthRole::Operator, &[env]).await;
    let host = scan_host(&pool, env).await;
    let token = issue(&pool, owner, SCAN, &[env]).await;
    let server = start(&pool).await;

    let reply = server
        .call(
            &token,
            "request_cve_scan",
            json!({ "system_id": host.system_id, "expected_derivation_id": host.derivation_id }),
        )
        .await;
    let accepted = structured(&reply);
    assert_eq!(accepted["status"], "accepted");
    assert_eq!(accepted["derivation_id"], host.derivation_id);
    let scan_id = accepted["scan_id"].as_str().expect("scan id").to_owned();

    let reply = server
        .call(
            &token,
            "get_scan_result",
            json!({ "system_id": host.system_id, "scan_id": scan_id }),
        )
        .await;
    let result = structured(&reply);
    assert_eq!(result["scan_id"], scan_id);
    assert_eq!(result["system_id"], host.system_id.to_string());
    assert_eq!(result["status"], "pending");
}

#[sqlx::test]
async fn demoted_owner_write_is_denied_and_audited_without_arguments(pool: PgPool) {
    let env = environment(&pool).await;
    let (owner, _) = user(&pool, AuthRole::Operator, &[env]).await;
    let token = issue(&pool, owner, WRITE, &[env]).await;
    let (poam_id, revision) = open_poam(&pool, env).await;
    let server = start(&pool).await;

    // The owner loses Operator after issuing; the token still authenticates.
    sync_user_role(&pool, owner, AuthRole::Viewer)
        .await
        .expect("demote owner");
    let secret_note = "argument-text-that-must-not-be-audited";
    let reply = server
        .call(
            &token,
            "update_poam_progress",
            note(poam_id, revision, secret_note),
        )
        .await;
    let (code, _) = tool_error(&reply);
    assert!(
        code == "forbidden" || code == "not_found",
        "unexpected error code {code}"
    );
    assert_eq!(current_revision(&pool, poam_id).await, revision);

    let rows = denied_rows(&pool, token.id).await;
    assert_eq!(rows.len(), 1);
    assert_denied(&rows[0], owner, &token, "update_poam_progress", &code);
    assert!(!rows[0].1.to_string().contains(secret_note));
}

#[sqlx::test]
async fn revoked_token_gets_401_on_the_next_call_and_writes_nothing(pool: PgPool) {
    let env = environment(&pool).await;
    let (owner, _) = user(&pool, AuthRole::Operator, &[env]).await;
    let token = issue(&pool, owner, WRITE, &[env]).await;
    let (poam_id, revision) = open_poam(&pool, env).await;
    let server = start(&pool).await;

    let first = server
        .call(
            &token,
            "update_poam_progress",
            note(poam_id, revision, "First"),
        )
        .await;
    assert_eq!(structured(&first)["revision"], revision + 1);

    token_service::revoke(&pool, owner, token.id)
        .await
        .expect("revoke token");
    let audits_before = audit_count(&pool).await;
    let second = server
        .call(
            &token,
            "update_poam_progress",
            note(poam_id, revision + 1, "Second"),
        )
        .await;
    assert_eq!(second.status, 401, "{}", second.text);
    assert_eq!(second.body["error"]["code"], -32001, "{}", second.text);
    assert!(!second.text.contains(&token.secret));

    assert_eq!(current_revision(&pool, poam_id).await, revision + 1);
    assert_eq!(audit_count(&pool).await, audits_before);
    assert!(denied_rows(&pool, token.id).await.is_empty());
}

#[sqlx::test]
async fn write_call_without_the_scope_is_forbidden_and_audited(pool: PgPool) {
    let env = environment(&pool).await;
    let (owner, _) = user(&pool, AuthRole::Operator, &[env]).await;
    let token = issue(&pool, owner, READ, &[env]).await;
    let (poam_id, revision) = open_poam(&pool, env).await;
    let server = start(&pool).await;

    // A client that ignores (or cached an older) tools/list still calls the
    // write tool; the server checks the scope on every call.
    let listed = tool_names(&server.rpc(&token, "tools/list", json!({})).await);
    assert!(!listed.contains("update_poam_progress"));
    let reply = server
        .call(
            &token,
            "update_poam_progress",
            note(poam_id, revision, "No scope"),
        )
        .await;
    let (code, message) = tool_error(&reply);
    assert_eq!(code, "forbidden");
    assert!(message.contains("poams:write"), "{message}");
    assert_eq!(current_revision(&pool, poam_id).await, revision);

    let rows = denied_rows(&pool, token.id).await;
    assert_eq!(rows.len(), 1);
    assert_denied(&rows[0], owner, &token, "update_poam_progress", "forbidden");
}

#[sqlx::test]
async fn verify_poam_on_a_poam_not_awaiting_verification_is_invalid_arguments(pool: PgPool) {
    let env = environment(&pool).await;
    let (owner, _) = user(&pool, AuthRole::Operator, &[env]).await;
    let token = issue(&pool, owner, WRITE_VERIFY, &[env]).await;
    let (poam_id, revision) = open_poam(&pool, env).await;
    let server = start(&pool).await;

    let reply = server
        .call(
            &token,
            "verify_poam",
            json!({ "poam_id": poam_id, "revision": revision }),
        )
        .await;
    let (code, message) = tool_error(&reply);
    assert_eq!(code, "invalid_arguments");
    assert!(message.starts_with("invalid_transition:"), "{message}");
    assert_eq!(current_revision(&pool, poam_id).await, revision);
    let attempts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM poam_verification_attempts WHERE poam_id=$1")
            .bind(poam_id)
            .fetch_one(&pool)
            .await
            .expect("count attempts");
    assert_eq!(attempts, 0);
}

/// Mirrors the direct `verify_poam_records_an_attributed_attempt_and_never_closes`
/// test in `mcp_write_poam` over `/mcp`. The scan still reports the CVE, so
/// the attempt is `rejected`; the tool never closes the POA&M.
#[sqlx::test]
async fn verify_poam_over_http_records_a_sealed_attributed_attempt(pool: PgPool) {
    let env = environment(&pool).await;
    let (owner, _) = user(&pool, AuthRole::Operator, &[env]).await;
    let token = issue(&pool, owner, WRITE_VERIFY, &[env]).await;
    let (poam_id, revision) = open_poam(&pool, env).await;
    let server = start(&pool).await;

    let awaiting = server
        .call(
            &token,
            "update_poam_progress",
            json!({
                "poam_id": poam_id,
                "revision": revision,
                "action": { "type": "set_status", "status": "awaiting_verification" },
            }),
        )
        .await;
    let awaiting = structured(&awaiting);
    assert_eq!(awaiting["status"], "awaiting_verification");
    let before = awaiting["revision"].as_i64().expect("revision");

    let reply = server
        .call(
            &token,
            "verify_poam",
            json!({ "poam_id": poam_id, "revision": before }),
        )
        .await;
    let result = structured(&reply);
    assert_eq!(result["outcome"], "rejected", "{}", reply.text);
    assert_eq!(result["revision"], before + 1, "{}", reply.text);
    let attempt_id: Uuid =
        serde_json::from_value(result["attempt_id"].clone()).expect("attempt id");
    assert_eq!(current_revision(&pool, poam_id).await, before + 1);

    let (sealed, access_token_id, via_agent, access_token_name, poam_revision, attempted_by): (
        bool,
        Option<Uuid>,
        bool,
        Option<String>,
        i64,
        Uuid,
    ) = sqlx::query_as(
        "SELECT sealed_at IS NOT NULL,access_token_id,via_agent,access_token_name, \
         poam_revision,attempted_by FROM poam_verification_attempts WHERE id=$1 AND poam_id=$2",
    )
    .bind(attempt_id)
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .expect("attempt row");
    assert!(sealed);
    assert_eq!(access_token_id, Some(token.id));
    assert!(via_agent);
    assert_eq!(access_token_name.as_deref(), Some(token.name.as_str()));
    // The attempt records the revision that it verified, before the bump.
    assert_eq!(poam_revision, before);
    assert_eq!(attempted_by, owner);

    let (status, closed): (String, bool) =
        sqlx::query_as("SELECT status,closed_at IS NOT NULL FROM poams WHERE id=$1")
            .bind(poam_id)
            .fetch_one(&pool)
            .await
            .expect("POA&M state");
    assert_ne!(status, "completed");
    assert_eq!(status, "awaiting_verification");
    assert!(!closed);

    let activity_token: Option<Uuid> = sqlx::query_scalar(
        "SELECT access_token_id FROM poam_activity WHERE poam_id=$1 AND kind='verification_attempted'",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .expect("verification activity");
    assert_eq!(activity_token, Some(token.id));

    let (audit_token, via): (Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT access_token_id,metadata->>'via' FROM admin_audit_events \
         WHERE action='poam_verification_attempted' AND target=$1",
    )
    .bind(format!("poam:{poam_id}"))
    .fetch_one(&pool)
    .await
    .expect("verification audit row");
    assert_eq!(audit_token, Some(token.id));
    assert_eq!(via.as_deref(), Some("mcp"));
    assert!(denied_rows(&pool, token.id).await.is_empty());
}

/// A token hard delete sets `admin_audit_events.access_token_id` to `NULL`.
/// The Audit Log still marks the events as agent actions and shows the name
/// snapshot. The token has only audit rows (a scan request and a denied
/// write), because `poam_activity` rows reject the `SET NULL` update.
#[sqlx::test]
async fn audit_log_keeps_the_agent_marker_after_token_deletion(pool: PgPool) {
    let env = environment(&pool).await;
    let (owner, _) = user(&pool, AuthRole::Operator, &[env]).await;
    let (_, admin_session) = user(&pool, AuthRole::Admin, &[]).await;
    let host = scan_host(&pool, env).await;
    let token = issue(&pool, owner, SCAN, &[env]).await;
    let server = start(&pool).await;

    let reply = server
        .call(
            &token,
            "request_cve_scan",
            json!({ "system_id": host.system_id, "expected_derivation_id": host.derivation_id }),
        )
        .await;
    assert_eq!(structured(&reply)["status"], "accepted");
    let denied = server.call(&token, "update_poam_progress", json!({})).await;
    assert_eq!(tool_error(&denied).0, "forbidden");

    let deleted = sqlx::query("DELETE FROM api_access_tokens WHERE id=$1")
        .bind(token.id)
        .execute(&pool)
        .await
        .expect("delete token")
        .rows_affected();
    assert_eq!(deleted, 1);

    let events = server.audit_events(&admin_session).await;
    for action in ["cve_scan_requested", "mcp_write_denied"] {
        let event = events
            .iter()
            .find(|event| event["action"] == action)
            .unwrap_or_else(|| panic!("the Audit Log lists {action}"));
        assert!(event["access_token_id"].is_null(), "{event}");
        assert_eq!(event["via_agent"], true, "{event}");
        assert_eq!(event["access_token_name"], token.name, "{event}");
    }
    // A browser action of the same owner carries no agent marker.
    let created = events
        .iter()
        .find(|event| event["action"] == "access_token_created")
        .expect("the Audit Log lists the token creation");
    assert_eq!(created["via_agent"], false, "{created}");
}
