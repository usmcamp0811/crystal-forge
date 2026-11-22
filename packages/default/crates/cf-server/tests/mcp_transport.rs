//! MCP transport regressions (TASK-534.4).
//!
//! Each SQLx test owns an isolated database with the full production
//! migrations. The tests mount the real `/mcp` handlers and the Admin MCP
//! status route on the same paths as `bin/server.rs` and speak HTTP to them.
//! Tokens are issued through the access-token service. Tool behavior is
//! covered elsewhere; these tests cover the transport, authentication and
//! `tools/list` scope filtering.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    extract::{DefaultBodyLimit, Request, State},
    routing::{get, post},
};
use bytes::Bytes;
use chrono::Utc;
use crystal_forge::api::models::CreateAccessTokenRequest;
use crystal_forge::auth::session::{SESSION_COOKIE_NAME, hash_token};
use crystal_forge::handlers::{agent_request::CFState, api::admin};
use crystal_forge::mcp;
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::queries::{
    auth_identity::{create_user_session, insert_user_environment_membership, sync_user_role},
    users::insert_user,
};
use crystal_forge::services::access_tokens::{self as service, AccessTokenSettings};
use crystal_forge::{
    config::ServerConfig, queue::QueueNotifier, server::jobs::BackgroundJobRegistry,
};
use futures::StreamExt;
use reqwest::{Client, RequestBuilder};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const PUBLIC_URL: &str = "https://forge.example.com/mcp";
const PUBLIC_ORIGIN: &str = "https://forge.example.com";
const CHALLENGE: &str = "Bearer realm=\"crystal-forge-mcp\"";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn enabled_config() -> ServerConfig {
    ServerConfig {
        mcp_enable: true,
        mcp_public_url: Some(PUBLIC_URL.to_owned()),
        ..ServerConfig::default()
    }
}

/// Creates a local user with one role and a live browser session; returns the
/// user id and the plaintext session cookie value.
async fn user(pool: &PgPool, role: AuthRole) -> (Uuid, String) {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("mcp-{suffix}@example.invalid"),
        Some("MCP Test"),
    )
    .await
    .expect("insert test user");
    sync_user_role(pool, user.id, role)
        .await
        .expect("assign test role");
    let session = format!("mcp-session-{suffix}");
    create_user_session(
        pool,
        user.id,
        hash_token(&session),
        Utc::now() + chrono::Duration::hours(1),
        Some("mcp-transport-test".into()),
        Some("127.0.0.1".into()),
        "local".into(),
    )
    .await
    .expect("create test session");
    (user.id, session)
}

async fn environment(pool: &PgPool) -> Uuid {
    let short = Uuid::new_v4().simple().to_string();
    sqlx::query_scalar::<_, Uuid>("INSERT INTO environments (name) VALUES ($1) RETURNING id")
        .bind(format!("mcp-env-{}", &short[..8]))
        .fetch_one(pool)
        .await
        .expect("insert environment")
}

/// An access token issued to an Operator who is a member of one environment.
struct Token {
    id: Uuid,
    owner: Uuid,
    owner_session: String,
    secret: String,
}

async fn issue_token(pool: &PgPool, scopes: &[&str]) -> Token {
    let (owner, owner_session) = user(pool, AuthRole::Operator).await;
    let env = environment(pool).await;
    insert_user_environment_membership(pool, owner, env)
        .await
        .expect("assign environment membership");
    let request = CreateAccessTokenRequest {
        name: Some("mcp transport".to_owned()),
        audience: Some("mcp".to_owned()),
        expires_in_days: Some(30),
        environment_ids: Some(vec![env.to_string()]),
        scopes: Some(scopes.iter().map(|scope| (*scope).to_owned()).collect()),
        confirm_production_write: None,
    };
    let settings = AccessTokenSettings::from_server_config(&enabled_config(), 86_400);
    let issued = service::create(pool, owner, &request, &settings)
        .await
        .expect("issue access token");
    Token {
        id: issued.metadata.id,
        owner,
        owner_session,
        secret: issued.secret.expose().to_owned(),
    }
}

// ---------------------------------------------------------------------------
// HTTP harness
// ---------------------------------------------------------------------------

struct Reply {
    status: u16,
    headers: reqwest::header::HeaderMap,
    text: String,
    body: Value,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }
}

struct Server {
    base: String,
    client: Client,
}

fn test_state(pool: &PgPool, config: ServerConfig) -> CFState {
    CFState::new(
        pool.clone(),
        config,
        Arc::new(QueueNotifier::new()),
        BackgroundJobRegistry::new(),
    )
}

async fn start(pool: &PgPool, config: ServerConfig) -> Server {
    let app = Router::new()
        .route(
            "/mcp",
            post(mcp::handle)
                .get(mcp::method_not_allowed)
                .delete(mcp::method_not_allowed)
                .layer(DefaultBodyLimit::max(mcp::MAX_BODY_BYTES)),
        )
        .route("/api/v1/admin/mcp/status", get(admin::mcp_status))
        .with_state(test_state(pool, config));
    serve(app).await
}

/// Starts an enabled server whose `POST /mcp` route bounds each request by
/// `limit` instead of [`mcp::REQUEST_TIMEOUT`].
async fn start_with_limit(pool: &PgPool, limit: Duration) -> Server {
    let handler = move |State(state): State<CFState>, request: Request| async move {
        mcp::handle_within(state, request, limit).await
    };
    let app = Router::new()
        .route(
            "/mcp",
            post(handler).layer(DefaultBodyLimit::max(mcp::MAX_BODY_BYTES)),
        )
        .with_state(test_state(pool, enabled_config()));
    serve(app).await
}

async fn serve(app: Router) -> Server {
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

async fn send(builder: RequestBuilder) -> Reply {
    let response = builder.send().await.expect("send MCP request");
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let text = response.text().await.expect("read response body");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply {
        status,
        headers,
        text,
        body,
    }
}

impl Server {
    /// Returns a JSON POST to `/mcp` with an optional bearer token.
    fn post(&self, bearer: Option<&str>, body: &Value) -> RequestBuilder {
        let mut builder = self
            .client
            .post(format!("{}/mcp", self.base))
            .header("content-type", "application/json")
            .body(body.to_string());
        if let Some(token) = bearer {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        builder
    }

    async fn rpc(&self, token: &str, method: &str, params: Value) -> Reply {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        send(self.post(Some(token), &body)).await
    }

    async fn admin_status(&self, session: &str) -> Reply {
        send(
            self.client
                .get(format!("{}/api/v1/admin/mcp/status", self.base))
                .header("cookie", format!("{SESSION_COOKIE_NAME}={session}")),
        )
        .await
    }
}

fn ping() -> Value {
    json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" })
}

fn assert_rpc_error(reply: &Reply, status: u16, code: i64) {
    assert_eq!(reply.status, status, "unexpected status: {}", reply.text);
    assert_eq!(reply.body["jsonrpc"], "2.0", "{}", reply.text);
    assert_eq!(reply.body["error"]["code"], code, "{}", reply.text);
    assert_eq!(reply.header("cache-control"), Some("no-store"));
}

fn assert_unauthorized(reply: &Reply, secret: &str) {
    assert_eq!(reply.status, 401, "unexpected status: {}", reply.text);
    assert_eq!(reply.header("www-authenticate"), Some(CHALLENGE));
    assert_eq!(reply.header("cache-control"), Some("no-store"));
    let expected = json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": { "code": -32001, "message": "Unauthorized" },
    });
    assert_eq!(reply.body, expected);
    assert!(!reply.text.contains(secret));
}

fn tool_names(reply: &Reply) -> BTreeSet<String> {
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.body["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|tool| {
            assert!(tool["inputSchema"].is_object(), "{tool}");
            assert!(tool["description"].is_string(), "{tool}");
            tool["name"].as_str().expect("tool name").to_owned()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn disabled_endpoint_returns_404_for_every_method(pool: PgPool) {
    let token = issue_token(&pool, &["cves:read"]).await;
    let server = start(&pool, ServerConfig::default()).await;
    let post = send(server.post(Some(&token.secret), &ping())).await;
    assert_eq!(post.status, 404, "{}", post.text);
    assert!(!post.text.contains(&token.secret));
    let get = send(server.client.get(format!("{}/mcp", server.base))).await;
    assert_eq!(get.status, 404);
}

#[sqlx::test]
async fn every_authentication_failure_is_the_same_401(pool: PgPool) {
    let token = issue_token(&pool, &["cves:read"]).await;
    let server = start(&pool, enabled_config()).await;
    let url = format!("{}/mcp", server.base);
    let body = ping().to_string();
    let json_post = |url: String| {
        server
            .client
            .post(url)
            .header("content-type", "application/json")
            .body(body.clone())
    };

    let missing = send(json_post(url.clone())).await;
    assert_unauthorized(&missing, &token.secret);

    let unknown = send(server.post(Some("cfat_v1_notarealtoken"), &ping())).await;
    assert_unauthorized(&unknown, &token.secret);

    let session_cookie = format!("{SESSION_COOKIE_NAME}={}", token.owner_session);
    let cookie_only = send(json_post(url.clone()).header("cookie", session_cookie)).await;
    assert_unauthorized(&cookie_only, &token.secret);

    let query = send(json_post(format!("{url}?token={}", token.secret))).await;
    assert_unauthorized(&query, &token.secret);

    let no_scheme =
        send(json_post(url.clone()).header("authorization", token.secret.clone())).await;
    assert_unauthorized(&no_scheme, &token.secret);

    let basic =
        send(json_post(url.clone()).header("authorization", format!("Basic {}", token.secret)))
            .await;
    assert_unauthorized(&basic, &token.secret);

    // The same token in the bearer header is admitted.
    let ok = send(server.post(Some(&token.secret), &ping())).await;
    assert_eq!(ok.status, 200, "{}", ok.text);
    assert_eq!(ok.body, json!({ "jsonrpc": "2.0", "id": 7, "result": {} }));
}

#[sqlx::test]
async fn transport_rejects_wrong_method_content_type_and_origin(pool: PgPool) {
    let token = issue_token(&pool, &["cves:read"]).await;
    let server = start(&pool, enabled_config()).await;
    let url = format!("{}/mcp", server.base);

    let get = send(server.client.get(&url)).await;
    assert_eq!(get.status, 405);
    assert_eq!(get.header("allow"), Some("POST"));
    let delete = send(server.client.delete(&url)).await;
    assert_eq!(delete.status, 405);

    let text_plain = send(
        server
            .client
            .post(&url)
            .header("content-type", "text/plain")
            .header("authorization", format!("Bearer {}", token.secret))
            .body(ping().to_string()),
    )
    .await;
    assert_rpc_error(&text_plain, 415, -32600);

    let foreign = send(
        server
            .post(Some(&token.secret), &ping())
            .header("origin", "https://evil.example"),
    )
    .await;
    assert_eq!(foreign.status, 403, "{}", foreign.text);
    assert_eq!(foreign.header("cache-control"), Some("no-store"));

    let same_origin = send(
        server
            .post(Some(&token.secret), &ping())
            .header("origin", PUBLIC_ORIGIN),
    )
    .await;
    assert_eq!(same_origin.status, 200, "{}", same_origin.text);

    // Without a configured public URL, no Origin is trusted; CLI requests
    // without Origin still work.
    let unset = start(
        &pool,
        ServerConfig {
            mcp_enable: true,
            ..ServerConfig::default()
        },
    )
    .await;
    let rejected = send(
        unset
            .post(Some(&token.secret), &ping())
            .header("origin", PUBLIC_ORIGIN),
    )
    .await;
    assert_eq!(rejected.status, 403, "{}", rejected.text);
    let cli = send(unset.post(Some(&token.secret), &ping())).await;
    assert_eq!(cli.status, 200, "{}", cli.text);
}

#[sqlx::test]
async fn initialize_negotiates_supported_protocol_versions(pool: PgPool) {
    let token = issue_token(&pool, &["cves:read"]).await;
    let server = start(&pool, enabled_config()).await;
    for (requested, expected) in [
        ("2025-11-25", "2025-11-25"),
        ("2025-06-18", "2025-06-18"),
        ("2025-03-26", "2025-03-26"),
        ("2099-01-01", "2025-11-25"),
    ] {
        let params = json!({
            "protocolVersion": requested,
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1" },
        });
        let reply = server.rpc(&token.secret, "initialize", params).await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        let result = &reply.body["result"];
        assert_eq!(result["protocolVersion"], expected);
        assert_eq!(result["capabilities"]["tools"]["listChanged"], false);
        assert_eq!(result["serverInfo"]["name"], "crystal-forge");
        assert!(result["instructions"].is_string());
        assert_eq!(reply.header("cache-control"), Some("no-store"));
        assert!(reply.header("mcp-session-id").is_none());
    }
}

#[sqlx::test]
async fn protocol_and_json_rpc_shape_errors(pool: PgPool) {
    let token = issue_token(&pool, &["cves:read"]).await;
    let server = start(&pool, enabled_config()).await;

    let unsupported = send(
        server
            .post(Some(&token.secret), &ping())
            .header("mcp-protocol-version", "2024-11-05"),
    )
    .await;
    assert_rpc_error(&unsupported, 400, -32600);
    let message = unsupported.body["error"]["message"].as_str().unwrap();
    assert!(message.contains("2025-11-25"), "{message}");

    let supported = send(
        server
            .post(Some(&token.secret), &ping())
            .header("mcp-protocol-version", "2025-06-18"),
    )
    .await;
    assert_eq!(supported.status, 200, "{}", supported.text);

    for notification in [
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "method": "notifications/cancelled",
                "params": { "requestId": 1 } }),
    ] {
        let reply = send(server.post(Some(&token.secret), &notification)).await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        assert!(reply.text.is_empty());
        assert_eq!(reply.header("cache-control"), Some("no-store"));
    }

    let unknown = server.rpc(&token.secret, "resources/list", json!({})).await;
    assert_rpc_error(&unknown, 200, -32601);
    assert_eq!(unknown.body["id"], 1);

    let batch = send(server.post(Some(&token.secret), &json!([ping(), ping()]))).await;
    assert_rpc_error(&batch, 400, -32600);

    let wrong_version = json!({ "jsonrpc": "1.0", "id": 1, "method": "ping" });
    let wrong = send(server.post(Some(&token.secret), &wrong_version)).await;
    assert_rpc_error(&wrong, 400, -32600);

    let malformed = send(
        server
            .client
            .post(format!("{}/mcp", server.base))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", token.secret))
            .body("{\"jsonrpc\":"),
    )
    .await;
    assert_rpc_error(&malformed, 400, -32700);

    let unknown_tool = server
        .rpc(
            &token.secret,
            "tools/call",
            json!({ "name": "no_such_tool", "arguments": {} }),
        )
        .await;
    assert_rpc_error(&unknown_tool, 200, -32602);
}

#[sqlx::test]
async fn tools_list_shows_only_tools_allowed_by_token_scopes(pool: PgPool) {
    let server = start(&pool, enabled_config()).await;

    let cves_only = issue_token(&pool, &["cves:read"]).await;
    let reply = server.rpc(&cves_only.secret, "tools/list", json!({})).await;
    assert_eq!(
        tool_names(&reply),
        BTreeSet::from([
            "get_scan_result".to_owned(),
            "get_system_security_context".to_owned(),
            "list_remediation_work".to_owned(),
        ])
    );

    let poams_only = issue_token(&pool, &["poams:read"]).await;
    let reply = server
        .rpc(&poams_only.secret, "tools/list", json!({}))
        .await;
    assert_eq!(
        tool_names(&reply),
        BTreeSet::from(["get_poam_detail".to_owned()])
    );

    let provenance_only = issue_token(&pool, &["provenance:read"]).await;
    let reply = server
        .rpc(&provenance_only.secret, "tools/list", json!({}))
        .await;
    assert!(tool_names(&reply).is_empty());

    let all = issue_token(
        &pool,
        &[
            "cves:read",
            "poams:read",
            "provenance:read",
            "policies:read",
            "bundles:read",
            "compliance:preview",
        ],
    )
    .await;
    let reply = server.rpc(&all.secret, "tools/list", json!({})).await;
    // Every read tool, and no write tool: write tools need a write scope.
    // validate_policy_draft writes nothing but needs the Admin-only
    // policies:draft scope, which an Operator owner cannot hold.
    let expected: BTreeSet<String> = mcp::MCP_TOOL_NAMES
        .iter()
        .filter(|name| !mcp::tools::is_write_tool(name) && **name != "validate_policy_draft")
        .map(|name| (*name).to_owned())
        .collect();
    assert_eq!(expected.len(), 12);
    assert_eq!(tool_names(&reply), expected);
}

#[sqlx::test]
async fn revoked_token_is_rejected_on_the_next_request(pool: PgPool) {
    let token = issue_token(&pool, &["cves:read"]).await;
    let server = start(&pool, enabled_config()).await;
    let before = send(server.post(Some(&token.secret), &ping())).await;
    assert_eq!(before.status, 200, "{}", before.text);

    service::revoke(&pool, token.owner, token.id)
        .await
        .expect("revoke token");
    let after = send(server.post(Some(&token.secret), &ping())).await;
    assert_unauthorized(&after, &token.secret);
}

#[sqlx::test]
async fn admitted_request_records_last_use_without_echoing_the_token(pool: PgPool) {
    let token = issue_token(&pool, &["cves:read", "poams:read", "provenance:read"]).await;
    let server = start(&pool, enabled_config()).await;
    assert!(last_used_at(&pool, token.id).await.is_none());

    for method in ["initialize", "ping", "tools/list"] {
        let reply = server.rpc(&token.secret, method, json!({})).await;
        assert_eq!(reply.status, 200, "{method}: {}", reply.text);
        assert_eq!(reply.header("cache-control"), Some("no-store"));
        assert!(!reply.text.contains(&token.secret), "{method}");
    }
    assert!(last_used_at(&pool, token.id).await.is_some());
}

async fn last_used_at(pool: &PgPool, token_id: Uuid) -> Option<chrono::DateTime<Utc>> {
    sqlx::query_scalar::<_, Option<chrono::DateTime<Utc>>>(
        "SELECT last_used_at FROM api_access_tokens WHERE id = $1",
    )
    .bind(token_id)
    .fetch_one(pool)
    .await
    .expect("read last_used_at")
}

#[sqlx::test]
async fn admin_status_reports_configuration_and_requires_admin(pool: PgPool) {
    let token = issue_token(&pool, &["cves:read"]).await;
    let (_, admin_session) = user(&pool, AuthRole::Admin).await;
    let server = start(&pool, enabled_config()).await;

    let status = server.admin_status(&admin_session).await;
    assert_eq!(status.status, 200, "{}", status.text);
    let body = &status.body;
    assert_eq!(body["enabled"], true);
    assert_eq!(body["public_url"], PUBLIC_URL);
    assert_eq!(body["protocol_versions"][0], "2025-11-25");
    assert_eq!(
        body["phase"],
        "read and write (write tools require a token write scope)"
    );
    assert_eq!(body["tools"].as_array().map(Vec::len), Some(21));
    assert_eq!(body["access_tokens_enabled"], true);
    assert_eq!(body["max_token_lifetime_days"], 90);
    assert_eq!(body["active_token_count"], 1);
    assert_eq!(body["warnings"], json!([]));
    assert!(!status.text.contains(&token.secret));

    let denied = server.admin_status(&token.owner_session).await;
    assert_eq!(denied.status, 403, "{}", denied.text);

    let loopback = start(
        &pool,
        ServerConfig {
            mcp_enable: true,
            mcp_public_url: Some("http://localhost:3000/mcp".to_owned()),
            ..ServerConfig::default()
        },
    )
    .await;
    let warned = loopback.admin_status(&admin_session).await;
    assert_eq!(warned.status, 200, "{}", warned.text);
    assert_eq!(warned.body["warnings"].as_array().map(Vec::len), Some(1));

    let no_url = start(
        &pool,
        ServerConfig {
            mcp_enable: true,
            ..ServerConfig::default()
        },
    )
    .await;
    let warned = no_url.admin_status(&admin_session).await;
    assert_eq!(warned.body["public_url"], Value::Null);
    assert_eq!(warned.body["warnings"].as_array().map(Vec::len), Some(1));
}

#[sqlx::test]
async fn slow_request_body_is_cut_off_by_the_request_deadline(pool: PgPool) {
    let token = issue_token(&pool, &["cves:read"]).await;
    let server = start_with_limit(&pool, Duration::from_secs(2)).await;

    // The body sends a prefix and then never completes, so only the request
    // deadline can end the body read.
    let stalled =
        futures::stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(b"{\"jsonrpc\""))])
            .chain(futures::stream::pending());
    let request = server
        .client
        .post(format!("{}/mcp", server.base))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token.secret))
        .body(reqwest::Body::wrap_stream(stalled));
    let reply = tokio::time::timeout(Duration::from_secs(10), send(request))
        .await
        .expect("the request deadline ends a stalled body read");
    assert_rpc_error(&reply, 503, -32603);
    assert_eq!(reply.body["error"]["message"], "The request timed out");
    assert!(reply.body["id"].is_null(), "{}", reply.text);
    assert!(!reply.text.contains(&token.secret));

    // The same deadline does not affect a complete request.
    let listed = tokio::time::timeout(
        Duration::from_secs(10),
        server.rpc(&token.secret, "tools/list", json!({})),
    )
    .await
    .expect("tools/list completes");
    assert_eq!(
        tool_names(&listed),
        BTreeSet::from([
            "get_scan_result".to_owned(),
            "get_system_security_context".to_owned(),
            "list_remediation_work".to_owned(),
        ])
    );
}
