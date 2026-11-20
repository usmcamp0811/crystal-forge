//! MCP scan tool and agent attribution regressions (TASK-534.5).
//!
//! Each SQLx test owns an isolated database with the full production
//! migrations. Tokens are issued through the access-token service and admitted
//! with `validate_presented_token`, as the MCP transport does, so the
//! in-transaction token-row check sees real rows. Tests call the tool
//! functions directly with the effective environments that the dispatcher
//! would compute.
//!
//! Scan targets are Reference registrations whose build has an eligible
//! producing builder session. Admission then always queues a `pending` scan
//! for that producer and never starts the server-local vulnix runner, so the
//! results do not depend on whether vulnix is installed on the test host.

use std::sync::Arc;

use axum::{Router, routing::post};
use chrono::Utc;
use crystal_forge::api::models::CreateAccessTokenRequest;
use crystal_forge::auth::session::{
    CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
};
use crystal_forge::handlers::{agent_request::CFState, api::systems};
use crystal_forge::mcp::scan_tools::{get_scan_result, request_cve_scan};
use crystal_forge::mcp::tools::{ToolCaller, ToolError};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::models::builders::CreateBuilderRequest;
use crystal_forge::queries::access_tokens::insert_mcp_write_denied;
use crystal_forge::queries::admin::list_admin_audit_events;
use crystal_forge::queries::auth_identity::{
    create_user_session, insert_user_environment_membership, sync_user_role,
};
use crystal_forge::queries::builders::{create_builder, establish_builder_session};
use crystal_forge::queries::cve_scan_leases::record_session_cve_capabilities;
use crystal_forge::queries::users::insert_user;
use crystal_forge::services::access_tokens::{
    self as token_service, AccessTokenError, AccessTokenSettings, validate_presented_token,
};
use crystal_forge::{
    config::ServerConfig, queue::QueueNotifier, server::jobs::BackgroundJobRegistry,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const SCAN: &[&str] = &["cves:read", "scans:request"];
const CSRF: &str = "mcp-write-scan-csrf";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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

async fn environment(pool: &PgPool, label: &str) -> Uuid {
    let short = Uuid::new_v4().simple().to_string();
    sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(format!("{label}-{}", &short[..8]))
        .fetch_one(pool)
        .await
        .expect("insert environment")
}

async fn user(pool: &PgPool, role: AuthRole, environments: &[Uuid]) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("mcp-scan-{suffix}@example.invalid"),
        Some("MCP Scan Test"),
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
    user.id
}

fn token_request(name: &str, scopes: &[&str], environments: &[Uuid]) -> CreateAccessTokenRequest {
    serde_json::from_value(json!({
        "name": name,
        "audience": "mcp",
        "expires_in_days": 30,
        "environment_ids": environments.iter().map(Uuid::to_string).collect::<Vec<_>>(),
        "scopes": scopes,
    }))
    .expect("token request")
}

/// One issued token: the request caller and the token name.
struct Token {
    caller: ToolCaller,
    name: String,
}

/// Issues a token through the service and admits it like the transport.
async fn issue(pool: &PgPool, owner: Uuid, scopes: &[&str], environments: &[Uuid]) -> Token {
    let name = format!("agent-{}", &Uuid::new_v4().simple().to_string()[..8]);
    let issued = token_service::create(
        pool,
        owner,
        &token_request(&name, scopes, environments),
        &settings(),
    )
    .await
    .expect("issue token");
    let validated = validate_presented_token(pool, issued.secret.expose(), Utc::now(), &settings())
        .await
        .expect("admit token");
    Token {
        caller: ToolCaller {
            token_id: issued.metadata.id,
            owner_user_id: validated.owner_user_id,
            owner_is_admin: validated.owner_is_admin,
            owner_environment_ids: validated.owner_environment_ids,
            grant: validated.grant,
        },
        name,
    }
}

/// One Reference registration with a scannable configuration artifact.
struct Host {
    system_id: Uuid,
    derivation_id: i32,
}

/// Creates a Reference system whose branch-head NixOS build was produced by
/// a CVE-capable builder session and published to a cache. The admission
/// selects that build and queues its scan for the producing session.
async fn scan_host(pool: &PgPool, environment_id: Uuid) -> Host {
    let suffix = Uuid::new_v4().simple().to_string();
    let hostname = format!("mcp-scan-{suffix}");
    let flake: i32 = sqlx::query_scalar(
        "INSERT INTO flakes(name,repo_url,snapshot_ready_at) VALUES($1,$2,now()) RETURNING id",
    )
    .bind(format!("mcp-scan-flake-{suffix}"))
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
    let store_path = format!("/nix/store/{suffix}-mcp-scan");
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
            name: format!("mcp-scan-producer-{suffix}"),
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
    establish_builder_session(pool, &builder.id, &session, 60, "mcp scan test")
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

fn scan_arguments(host: &Host) -> Value {
    json!({"system_id": host.system_id, "expected_derivation_id": host.derivation_id})
}

async fn scan_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM cve_scans")
        .fetch_one(pool)
        .await
        .expect("count scans")
}

async fn scan_audit_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit_events WHERE action='cve_scan_requested'")
        .fetch_one(pool)
        .await
        .expect("count scan audit events")
}

fn scan_id_of(result: &Value) -> Uuid {
    result["scan_id"]
        .as_str()
        .expect("scan id")
        .parse()
        .expect("scan id is a UUID")
}

// ---------------------------------------------------------------------------
// request_cve_scan
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn request_cve_scan_is_accepted_and_attributed_to_the_token(pool: PgPool) {
    let env = environment(&pool, "scan-a").await;
    let owner = user(&pool, AuthRole::Operator, &[env]).await;
    let host = scan_host(&pool, env).await;
    let token = issue(&pool, owner, SCAN, &[env]).await;

    let result = request_cve_scan(&pool, &token.caller, &[env], scan_arguments(&host))
        .await
        .expect("scan request is accepted");
    assert_eq!(result["status"], "accepted");
    assert_eq!(result["derivation_id"], host.derivation_id);
    assert_eq!(result["configuration_derivation_id"], host.derivation_id);
    assert_eq!(
        result["note"],
        "Acceptance is not a result; poll get_scan_result."
    );
    let scan_id = scan_id_of(&result);

    // The human owner is the actor; the token is recorded beside it.
    let (actor, token_id, metadata): (Option<Uuid>, Option<Uuid>, Value) = sqlx::query_as(
        "SELECT actor_user_id, access_token_id, metadata FROM admin_audit_events \
         WHERE action='cve_scan_requested'",
    )
    .fetch_one(&pool)
    .await
    .expect("read scan audit event");
    assert_eq!(actor, Some(owner));
    assert_eq!(token_id, Some(token.caller.token_id));
    assert_eq!(metadata["via"], "mcp");
    assert_eq!(
        metadata["access_token_id"],
        token.caller.token_id.to_string()
    );
    assert_eq!(metadata["access_token_name"], token.name);
    assert_eq!(metadata["scan_id"], scan_id.to_string());

    let events = list_admin_audit_events(&pool).await.expect("list audit");
    let event = events
        .iter()
        .find(|event| event.action == "cve_scan_requested")
        .expect("audit list shows the scan request");
    assert_eq!(event.access_token_id, Some(token.caller.token_id));
    assert_eq!(
        event.access_token_name.as_deref(),
        Some(token.name.as_str())
    );

    // A repeated request returns the active scan of the same derivation.
    let again = request_cve_scan(&pool, &token.caller, &[env], scan_arguments(&host))
        .await
        .expect("repeated request is accepted");
    assert_eq!(scan_id_of(&again), scan_id);
    assert_eq!(scan_count(&pool).await, 1);
}

#[sqlx::test]
async fn wrong_expected_derivation_reports_scan_target_changed(pool: PgPool) {
    let env = environment(&pool, "scan-stale").await;
    let owner = user(&pool, AuthRole::Operator, &[env]).await;
    let host = scan_host(&pool, env).await;
    let token = issue(&pool, owner, SCAN, &[env]).await;

    let stale = json!({
        "system_id": host.system_id,
        "expected_derivation_id": host.derivation_id + 1000,
    });
    match request_cve_scan(&pool, &token.caller, &[env], stale).await {
        Err(ToolError::InvalidArguments(message)) => {
            assert!(message.starts_with("scan_target_changed"), "{message}");
            assert!(
                message.contains(&format!("derivation_id {}", host.derivation_id)),
                "{message}"
            );
        }
        other => panic!("expected scan_target_changed, got {other:?}"),
    }
    assert_eq!(scan_count(&pool).await, 0);
    assert_eq!(scan_audit_count(&pool).await, 0);
}

#[sqlx::test]
async fn system_outside_the_token_environments_is_not_found(pool: PgPool) {
    let env_a = environment(&pool, "scan-own").await;
    let env_b = environment(&pool, "scan-other").await;
    let other_host = scan_host(&pool, env_b).await;

    // An Operator assigned to both environments with a token for one.
    let operator = user(&pool, AuthRole::Operator, &[env_a, env_b]).await;
    let operator_token = issue(&pool, operator, SCAN, &[env_a]).await;
    // An Admin passes the browser write check everywhere; the token still
    // limits it to its grant.
    let admin = user(&pool, AuthRole::Admin, &[]).await;
    let admin_token = issue(&pool, admin, SCAN, &[env_a]).await;

    for token in [&operator_token, &admin_token] {
        for effective in [vec![env_a], vec![env_a, env_b]] {
            // The second set models a dispatcher that computed too much: the
            // admission still rejects the system outside the token grant.
            let result = request_cve_scan(
                &pool,
                &token.caller,
                &effective,
                scan_arguments(&other_host),
            )
            .await;
            assert!(
                matches!(result, Err(ToolError::NotFound)),
                "expected not found, got {result:?}"
            );
        }
    }
    let unknown = json!({"system_id": Uuid::new_v4(), "expected_derivation_id": 1});
    assert!(matches!(
        request_cve_scan(&pool, &operator_token.caller, &[env_a], unknown).await,
        Err(ToolError::NotFound)
    ));
    assert_eq!(scan_count(&pool).await, 0);
    assert_eq!(scan_audit_count(&pool).await, 0);
}

/// A revoked or expired token and a demoted owner return not found from the
/// admission. The transport rejects a revoked or expired token before any
/// tool runs; these calls model a revocation or demotion that commits between
/// the transport check and the admission transaction.
#[sqlx::test]
async fn revoked_token_or_demoted_owner_cannot_request_a_scan(pool: PgPool) {
    let env = environment(&pool, "scan-revoke").await;
    let owner = user(&pool, AuthRole::Operator, &[env]).await;
    let host = scan_host(&pool, env).await;

    let revoked = issue(&pool, owner, SCAN, &[env]).await;
    token_service::revoke(&pool, owner, revoked.caller.token_id)
        .await
        .expect("revoke token");
    assert!(matches!(
        request_cve_scan(&pool, &revoked.caller, &[env], scan_arguments(&host)).await,
        Err(ToolError::NotFound)
    ));

    let expired = issue(&pool, owner, SCAN, &[env]).await;
    sqlx::query(
        "UPDATE api_access_tokens SET expires_at = now() - interval '1 minute', \
         created_at = now() - interval '1 day' WHERE id = $1",
    )
    .bind(expired.caller.token_id)
    .execute(&pool)
    .await
    .expect("expire token");
    assert!(matches!(
        request_cve_scan(&pool, &expired.caller, &[env], scan_arguments(&host)).await,
        Err(ToolError::NotFound)
    ));

    let demoted = issue(&pool, owner, SCAN, &[env]).await;
    sync_user_role(&pool, owner, AuthRole::Viewer)
        .await
        .expect("demote owner");
    assert!(matches!(
        request_cve_scan(&pool, &demoted.caller, &[env], scan_arguments(&host)).await,
        Err(ToolError::NotFound)
    ));
    assert_eq!(scan_count(&pool).await, 0);
    assert_eq!(scan_audit_count(&pool).await, 0);
}

#[sqlx::test]
async fn viewer_cannot_create_a_scan_request_token(pool: PgPool) {
    let env = environment(&pool, "scan-viewer").await;
    let viewer = user(&pool, AuthRole::Viewer, &[env]).await;
    match token_service::create(
        &pool,
        viewer,
        &token_request("viewer agent", SCAN, &[env]),
        &settings(),
    )
    .await
    {
        Err(AccessTokenError::Validation(fields)) => {
            let message = fields.get("scopes").expect("scope message");
            assert!(message.contains("Requires Operator"), "{message}");
        }
        Err(other) => panic!("expected a scope validation error, got {other:?}"),
        Ok(_) => panic!("a Viewer must not receive scans:request"),
    }
    let tokens: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_access_tokens")
        .fetch_one(&pool)
        .await
        .expect("count tokens");
    assert_eq!(tokens, 0);
}

// ---------------------------------------------------------------------------
// get_scan_result
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn get_scan_result_reads_the_requested_scan_and_hides_unrelated_scans(pool: PgPool) {
    let env = environment(&pool, "scan-result").await;
    let owner = user(&pool, AuthRole::Operator, &[env]).await;
    let host = scan_host(&pool, env).await;
    let neighbor = scan_host(&pool, env).await;
    let token = issue(&pool, owner, SCAN, &[env]).await;

    let accepted = request_cve_scan(&pool, &token.caller, &[env], scan_arguments(&host))
        .await
        .expect("scan request is accepted");
    let scan_id = scan_id_of(&accepted);

    let result = get_scan_result(
        &pool,
        &token.caller,
        &[env],
        json!({"system_id": host.system_id, "scan_id": scan_id}),
    )
    .await
    .expect("scan result is readable");
    assert_eq!(result["scan_id"], scan_id.to_string());
    assert_eq!(result["system_id"], host.system_id.to_string());
    assert_eq!(result["status"], "pending");
    assert_eq!(result["derivation_id"], host.derivation_id);
    assert_eq!(result["is_current_authority"], false);
    assert!(
        result["counts"].is_null(),
        "incomplete scans report no counts"
    );
    assert!(result.get("scan_metadata").is_none());

    // A scan of another system, read through a visible system, is not found.
    let unrelated: Uuid = sqlx::query_scalar(
        "INSERT INTO cve_scans(derivation_id,scanner_name,status) VALUES($1,'vulnix','in_progress') RETURNING id",
    )
    .bind(neighbor.derivation_id)
    .fetch_one(&pool)
    .await
    .expect("insert unrelated scan");
    for scan in [unrelated, Uuid::new_v4()] {
        let result = get_scan_result(
            &pool,
            &token.caller,
            &[env],
            json!({"system_id": host.system_id, "scan_id": scan}),
        )
        .await;
        assert!(
            matches!(result, Err(ToolError::NotFound)),
            "expected not found, got {result:?}"
        );
    }
    // The system itself must be in the effective environments.
    assert!(matches!(
        get_scan_result(
            &pool,
            &token.caller,
            &[],
            json!({"system_id": host.system_id, "scan_id": scan_id}),
        )
        .await,
        Err(ToolError::NotFound)
    ));
}

// ---------------------------------------------------------------------------
// Browser path and denied-write audit
// ---------------------------------------------------------------------------

/// Starts the real browser scan route on an ephemeral port and returns its
/// base URL.
async fn start_browser_api(pool: &PgPool) -> String {
    let state = CFState::new(
        pool.clone(),
        ServerConfig::default(),
        Arc::new(QueueNotifier::new()),
        BackgroundJobRegistry::new(),
    );
    let app = Router::new()
        .route(
            "/api/v1/systems/:id/cve-scan",
            post(systems::trigger_system_cve_scan),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind browser test server");
    let address = listener.local_addr().expect("read test server address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve browser test API")
    });
    format!("http://{address}")
}

#[sqlx::test]
async fn browser_scan_request_records_no_access_token(pool: PgPool) {
    let env = environment(&pool, "scan-browser").await;
    let owner = user(&pool, AuthRole::Operator, &[env]).await;
    let host = scan_host(&pool, env).await;
    let session = format!("mcp-scan-session-{}", Uuid::new_v4().simple());
    create_user_session(
        &pool,
        owner,
        hash_token(&session),
        Utc::now() + chrono::Duration::hours(1),
        Some("mcp-write-scan-test".into()),
        Some("127.0.0.1".into()),
        "local".into(),
    )
    .await
    .expect("create browser session");
    let base = start_browser_api(&pool).await;

    let response = reqwest::Client::new()
        .post(format!(
            "{base}/api/v1/systems/{}/cve-scan?expected_derivation_id={}",
            host.system_id, host.derivation_id
        ))
        .header(
            "cookie",
            format!("{SESSION_COOKIE_NAME}={session}; {CSRF_COOKIE_NAME}={CSRF}"),
        )
        .header(CSRF_HEADER_NAME.as_str(), CSRF)
        .send()
        .await
        .expect("send browser scan request");
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, 202, "{body}");
    assert_eq!(body["status"], "accepted");

    let (token_id, metadata): (Option<Uuid>, Value) = sqlx::query_as(
        "SELECT access_token_id, metadata FROM admin_audit_events \
         WHERE action='cve_scan_requested'",
    )
    .fetch_one(&pool)
    .await
    .expect("read scan audit event");
    assert_eq!(token_id, None);
    assert!(metadata.get("via").is_none(), "{metadata}");
    assert!(metadata.get("access_token_id").is_none(), "{metadata}");
}

#[sqlx::test]
async fn denied_write_audit_records_the_token_tool_and_reason(pool: PgPool) {
    let env = environment(&pool, "scan-denied").await;
    let owner = user(&pool, AuthRole::Operator, &[env]).await;
    let token = issue(&pool, owner, &["cves:read"], &[env]).await;

    insert_mcp_write_denied(
        &pool,
        owner,
        token.caller.token_id,
        "request_cve_scan",
        "scope_missing",
    )
    .await
    .expect("write denied audit event");

    let (actor, target, token_id, metadata): (Option<Uuid>, String, Option<Uuid>, Value) =
        sqlx::query_as(
            "SELECT actor_user_id, target, access_token_id, metadata FROM admin_audit_events \
             WHERE action='mcp_write_denied'",
        )
        .fetch_one(&pool)
        .await
        .expect("read denied audit event");
    assert_eq!(actor, Some(owner));
    assert_eq!(target, token.caller.token_id.simple().to_string());
    assert_eq!(token_id, Some(token.caller.token_id));
    assert_eq!(
        metadata,
        json!({
            "tool": "request_cve_scan",
            "reason": "scope_missing",
            "access_token_name": token.name,
        })
    );

    let events = list_admin_audit_events(&pool).await.expect("list audit");
    let event = events
        .iter()
        .find(|event| event.action == "mcp_write_denied")
        .expect("audit list shows the denied call");
    assert_eq!(
        event.access_token_name.as_deref(),
        Some(token.name.as_str())
    );
}
