//! Owner access-token lifecycle regressions (TASK-534.3).
//!
//! Each SQLx test owns an isolated database with the full production
//! migrations. HTTP tests mount the real access-token handlers on the same
//! routes as `bin/server.rs` and authenticate with the browser session cookie
//! and the double-submit CSRF pair, as the web UI does.

use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use chrono::Utc;
use crystal_forge::api::models::CreateAccessTokenRequest;
use crystal_forge::auth::access_token::{TOKEN_PREFIX, generate_access_token};
use crystal_forge::auth::authorization::CredentialIdentity;
use crystal_forge::auth::session::{
    CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
};
use crystal_forge::handlers::{
    agent_request::CFState,
    api::{access_tokens, admin},
};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::queries::{
    auth_identity::{
        clear_user_environment_memberships, create_user_session,
        insert_user_environment_membership, sync_user_role,
    },
    oidc_entitlements::{ReconciliationOutcome, reconcile, reserve_entitlement_generation},
    users::insert_user,
};
use crystal_forge::services::access_tokens::{
    self as service, AccessTokenError, AccessTokenSettings, TokenRejected, validate_presented_token,
};
use crystal_forge::{
    config::ServerConfig, queue::QueueNotifier, server::jobs::BackgroundJobRegistry,
};
use reqwest::{Client, Method, RequestBuilder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

const CSRF: &str = "access-token-csrf";
const POLICY: &str = "/api/v1/user/access-token-policy";
const TOKENS: &str = "/api/v1/user/access-tokens";
const READ_SCOPES: [&str; 3] = ["cves:read", "poams:read", "provenance:read"];
/// Entitlement freshness bound used by service-level calls, in seconds.
const MAX_AGE: u64 = 86_400;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A signed-in user: the user id and the plaintext session cookie value.
#[derive(Clone)]
struct Actor {
    id: Uuid,
    session: String,
}

async fn open_session(pool: &PgPool, user_id: Uuid, source: &str) -> String {
    let token = format!("access-token-session-{}", Uuid::new_v4().simple());
    create_user_session(
        pool,
        user_id,
        hash_token(&token),
        Utc::now() + chrono::Duration::hours(1),
        Some("access-tokens-test".into()),
        Some("127.0.0.1".into()),
        source.into(),
    )
    .await
    .expect("create test session");
    token
}

/// Creates a local user with exactly one role and a live browser session.
async fn actor(pool: &PgPool, role: AuthRole) -> Actor {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("at-{suffix}@example.invalid"),
        Some("Access Token Test"),
    )
    .await
    .expect("insert test user");
    sync_user_role(pool, user.id, role)
        .await
        .expect("assign test role");
    let session = open_session(pool, user.id, "local").await;
    Actor {
        id: user.id,
        session,
    }
}

async fn environment(pool: &PgPool, label: &str) -> Uuid {
    let short = Uuid::new_v4().simple().to_string();
    sqlx::query_scalar::<_, Uuid>("INSERT INTO environments (name) VALUES ($1) RETURNING id")
        .bind(format!("{label}-{}", &short[..8]))
        .fetch_one(pool)
        .await
        .expect("insert environment")
}

async fn assign(pool: &PgPool, user: Uuid, environment: Uuid) {
    insert_user_environment_membership(pool, user, environment)
        .await
        .expect("assign environment membership");
}

async fn deactivate_environment(pool: &PgPool, environment: Uuid) {
    sqlx::query("UPDATE environments SET is_active = FALSE WHERE id = $1")
        .bind(environment)
        .execute(pool)
        .await
        .expect("deactivate environment");
}

/// Creates a user with `role` and a membership in each of `environments`.
async fn member(pool: &PgPool, role: AuthRole, environments: &[Uuid]) -> Actor {
    let who = actor(pool, role).await;
    for environment in environments {
        assign(pool, who.id, *environment).await;
    }
    who
}

/// Creates an Operator assigned to one fresh environment.
async fn operator_with_environment(pool: &PgPool) -> (Actor, Uuid) {
    let env = environment(pool, "at-env").await;
    (member(pool, AuthRole::Operator, &[env]).await, env)
}

// ---------------------------------------------------------------------------
// HTTP harness
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Csrf {
    Valid,
    Missing,
    Mismatched,
}

/// One decoded HTTP response. `text` keeps the raw body for secret scans.
struct Reply {
    status: u16,
    cache_control: Option<String>,
    text: String,
    body: Value,
}

struct Api {
    base: String,
    client: Client,
}

/// Starts the real access-token routes (as mounted in `bin/server.rs`) plus the
/// admin audit-log route on an ephemeral port.
async fn start_with(pool: &PgPool, config: ServerConfig) -> Api {
    let state = CFState::new(
        pool.clone(),
        config,
        Arc::new(QueueNotifier::new()),
        BackgroundJobRegistry::new(),
    );
    let app = Router::new()
        .route(
            "/api/v1/user/access-token-policy",
            get(access_tokens::get_policy),
        )
        .route(
            "/api/v1/user/access-tokens",
            get(access_tokens::list_access_tokens).post(access_tokens::create_access_token),
        )
        .route(
            "/api/v1/user/access-tokens/:id",
            get(access_tokens::get_access_token).delete(access_tokens::revoke_access_token),
        )
        .route(
            "/api/v1/user/access-tokens/:id/rotate",
            post(access_tokens::rotate_access_token),
        )
        .route("/api/v1/admin/audit-events", get(admin::list_audit_events))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind access token test server");
    let address = listener.local_addr().expect("read test server address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve access token test API")
    });
    Api {
        base: format!("http://{address}"),
        client: Client::new(),
    }
}

async fn start(pool: &PgPool) -> Api {
    start_with(pool, ServerConfig::default()).await
}

impl Api {
    fn builder(
        &self,
        method: Method,
        path: &str,
        session: Option<&str>,
        csrf: Csrf,
    ) -> RequestBuilder {
        let mut cookies = Vec::new();
        if let Some(session) = session {
            cookies.push(format!("{SESSION_COOKIE_NAME}={session}"));
        }
        if !matches!(csrf, Csrf::Missing) {
            cookies.push(format!("{CSRF_COOKIE_NAME}={CSRF}"));
        }
        let mut builder = self.client.request(method, format!("{}{path}", self.base));
        if !cookies.is_empty() {
            builder = builder.header("cookie", cookies.join("; "));
        }
        match csrf {
            Csrf::Valid => builder.header(CSRF_HEADER_NAME.as_str(), CSRF),
            Csrf::Mismatched => builder.header(CSRF_HEADER_NAME.as_str(), "wrong"),
            Csrf::Missing => builder,
        }
    }

    /// Sends a request with full control over session, CSRF, bearer and body.
    async fn raw(
        &self,
        method: Method,
        path: &str,
        session: Option<&str>,
        csrf: Csrf,
        bearer: Option<&str>,
        body: Option<&Value>,
    ) -> Reply {
        let mut builder = self.builder(method, path, session, csrf);
        if let Some(bearer) = bearer {
            builder = builder.header("authorization", format!("Bearer {bearer}"));
        }
        if let Some(body) = body {
            builder = builder.json(body);
        }
        let response = builder.send().await.expect("send access token request");
        let status = response.status().as_u16();
        let cache_control = response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let text = response.text().await.expect("read response body");
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Reply {
            status,
            cache_control,
            text,
            body,
        }
    }

    async fn call(&self, method: Method, path: &str, who: &Actor, body: Option<&Value>) -> Reply {
        self.raw(method, path, Some(&who.session), Csrf::Valid, None, body)
            .await
    }

    async fn create(&self, who: &Actor, body: &Value) -> Reply {
        self.call(Method::POST, TOKENS, who, Some(body)).await
    }

    async fn policy(&self, who: &Actor) -> Reply {
        self.call(Method::GET, POLICY, who, None).await
    }

    async fn list(&self, who: &Actor, query: &str) -> Reply {
        self.call(Method::GET, &format!("{TOKENS}?{query}"), who, None)
            .await
    }

    async fn detail(&self, who: &Actor, id: impl std::fmt::Display) -> Reply {
        self.call(Method::GET, &format!("{TOKENS}/{id}"), who, None)
            .await
    }

    async fn revoke(&self, who: &Actor, id: impl std::fmt::Display) -> Reply {
        self.call(Method::DELETE, &format!("{TOKENS}/{id}"), who, None)
            .await
    }

    async fn rotate(&self, who: &Actor, id: impl std::fmt::Display) -> Reply {
        self.call(Method::POST, &format!("{TOKENS}/{id}/rotate"), who, None)
            .await
    }
}

/// The six owner endpoints: policy, list, create, detail, revoke and rotate.
fn endpoints(id: Uuid) -> Vec<(Method, String)> {
    vec![
        (Method::GET, POLICY.to_owned()),
        (Method::GET, TOKENS.to_owned()),
        (Method::POST, TOKENS.to_owned()),
        (Method::GET, format!("{TOKENS}/{id}")),
        (Method::DELETE, format!("{TOKENS}/{id}")),
        (Method::POST, format!("{TOKENS}/{id}/rotate")),
    ]
}

fn create_body(name: &str, environments: &[Uuid], scopes: &[&str], days: i64) -> Value {
    json!({
        "name": name,
        "audience": "mcp",
        "expires_in_days": days,
        "environment_ids": environments,
        "scopes": scopes,
    })
}

fn read_body(name: &str, environments: &[Uuid]) -> Value {
    create_body(name, environments, &READ_SCOPES, 30)
}

fn with_field(base: &Value, key: &str, value: Value) -> Value {
    let mut body = base.clone();
    body[key] = value;
    body
}

fn without_field(base: &Value, key: &str) -> Value {
    let mut body = base.clone();
    body.as_object_mut()
        .expect("create body is an object")
        .remove(key);
    body
}

fn id_of(body: &Value) -> Uuid {
    body["id"]
        .as_str()
        .expect("response has id")
        .parse()
        .expect("id is a UUID")
}

/// Asserts a 201 and returns the new token id and its one-time plaintext.
fn issued(reply: &Reply) -> (Uuid, String) {
    assert_eq!(reply.status, 201, "expected issued token: {}", reply.text);
    let secret = reply.body["token"]
        .as_str()
        .expect("issued response carries token")
        .to_owned();
    (id_of(&reply.body), secret)
}

async fn create_token(api: &Api, who: &Actor, name: &str, environments: &[Uuid]) -> (Uuid, String) {
    issued(&api.create(who, &read_body(name, environments)).await)
}

fn assert_error(reply: &Reply, status: u16, code: &str) {
    assert_eq!(reply.status, status, "unexpected status: {}", reply.text);
    assert_eq!(
        reply.body["error"], code,
        "unexpected error: {}",
        reply.text
    );
    assert_eq!(reply.cache_control.as_deref(), Some("no-store"));
}

fn assert_field_error(reply: &Reply, field: &str) {
    assert_error(reply, 400, "validation_failed");
    assert!(
        reply.body["details"]["fields"][field].is_string(),
        "expected details.fields.{field}: {}",
        reply.text
    );
}

// ---------------------------------------------------------------------------
// Database helpers
// ---------------------------------------------------------------------------

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(pool)
        .await
        .expect("count rows")
}

async fn token_count(pool: &PgPool) -> i64 {
    count(pool, "SELECT COUNT(*) FROM api_access_tokens").await
}

async fn token_audit_count(pool: &PgPool, action: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM admin_audit_events WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .expect("count audit events")
}

async fn assert_nothing_issued(pool: &PgPool) {
    assert_eq!(token_count(pool).await, 0);
    assert_eq!(
        count(pool, "SELECT COUNT(*) FROM api_access_token_scopes").await,
        0
    );
    assert_eq!(
        count(pool, "SELECT COUNT(*) FROM api_access_token_environments").await,
        0
    );
    assert_eq!(
        count(
            pool,
            "SELECT COUNT(*) FROM admin_audit_events WHERE action LIKE 'access_token_%'"
        )
        .await,
        0
    );
}

async fn is_revoked(pool: &PgPool, id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT revoked_at IS NOT NULL FROM api_access_tokens WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("read revocation state")
}

/// Moves a token into the past without violating `expires_at > created_at`.
async fn expire(pool: &PgPool, id: Uuid) {
    sqlx::query(
        "UPDATE api_access_tokens
         SET expires_at = now() - interval '1 minute', created_at = now() - interval '1 day'
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("expire token");
}

/// Inserts `count` live tokens created two hours ago, which count toward the
/// live-token limit but not toward the hourly issue limit.
async fn seed_live_tokens(pool: &PgPool, owner: Uuid, count: i32) -> Vec<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO api_access_tokens
             (user_id, name, audience, secret_hash, display_hint, created_at, expires_at)
         SELECT $1::uuid, 'seed-' || n::text, 'mcp',
                sha256(convert_to(gen_random_uuid()::text, 'UTF8')), 'cfat_v1_seed',
                now() - interval '2 hours', now() + interval '30 days'
         FROM generate_series(1, $2::int) AS n
         RETURNING id",
    )
    .bind(owner)
    .bind(count)
    .fetch_all(pool)
    .await
    .expect("seed live tokens")
}

fn secret_part(secret: &str) -> &str {
    &secret[TOKEN_PREFIX.len() + 33..]
}

/// Asserts the plaintext appears in no column of any token table or audit row.
async fn assert_secret_absent(pool: &PgPool, secret: &str) {
    for sql in [
        "SELECT t::text FROM api_access_tokens t",
        "SELECT s::text FROM api_access_token_scopes s",
        "SELECT e::text FROM api_access_token_environments e",
        "SELECT a::text FROM admin_audit_events a",
    ] {
        let rows = sqlx::query_scalar::<_, String>(sql)
            .fetch_all(pool)
            .await
            .expect("read rows as text");
        for row in rows {
            assert!(!row.contains(secret), "plaintext token stored by `{sql}`");
            assert!(
                !row.contains(secret_part(secret)),
                "token secret stored by `{sql}`"
            );
        }
    }
}

fn service_request(
    name: &str,
    environments: &[Uuid],
    scopes: &[&str],
    days: i64,
) -> CreateAccessTokenRequest {
    CreateAccessTokenRequest {
        name: Some(name.to_owned()),
        audience: Some("mcp".to_owned()),
        expires_in_days: Some(days),
        environment_ids: Some(environments.iter().map(Uuid::to_string).collect()),
        scopes: Some(scopes.iter().map(|scope| (*scope).to_owned()).collect()),
        confirm_production_write: None,
    }
}

fn settings(max_age: u64) -> AccessTokenSettings {
    AccessTokenSettings {
        enabled: true,
        default_lifetime_days: 30,
        max_lifetime_days: 90,
        entitlement_max_age_seconds: max_age,
        mcp_enabled: false,
        mcp_public_url: None,
    }
}

// ---------------------------------------------------------------------------
// Create: roles, one-time secret, hash-only storage
// ---------------------------------------------------------------------------

/// Creates a token and proves the secret is shown once and stored only as a
/// SHA-256 hash.
async fn assert_secure_create(pool: &PgPool, api: &Api, who: &Actor, env: Uuid, name: &str) {
    let reply = api.create(who, &read_body(name, &[env])).await;
    assert_eq!(reply.status, 201, "create failed: {}", reply.text);
    assert_eq!(reply.cache_control.as_deref(), Some("no-store"));
    let (id, secret) = issued(&reply);
    let simple = id.simple().to_string();

    assert!(secret.starts_with(TOKEN_PREFIX));
    assert_eq!(secret.len(), 84);
    assert_eq!(&secret[TOKEN_PREFIX.len()..TOKEN_PREFIX.len() + 32], simple);
    assert_eq!(reply.body["name"], name);
    assert_eq!(reply.body["audience"], "mcp");
    assert_eq!(reply.body["status"], "active");
    assert_eq!(
        reply.body["scopes"],
        json!(["cves:read", "poams:read", "provenance:read"])
    );
    assert_eq!(reply.body["environments"][0]["id"], env.to_string());
    assert_eq!(reply.body["environments"][0]["is_active"], true);
    assert_eq!(
        reply.body["display_hint"],
        format!("{TOKEN_PREFIX}{}\u{2026}", &simple[..4])
    );
    assert!(reply.body["revoked_at"].is_null());
    assert!(reply.body["last_used_at"].is_null());
    assert!(reply.body.get("secret_hash").is_none());
    assert!(reply.body.get("hash").is_none());

    let (length, stored): (i32, Vec<u8>) = sqlx::query_as(
        "SELECT octet_length(secret_hash), secret_hash FROM api_access_tokens WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("read stored hash");
    assert_eq!(length, 32);
    assert_eq!(stored, Sha256::digest(secret.as_bytes()).to_vec());
    assert_secret_absent(pool, &secret).await;

    let (actor_identifier, metadata): (String, Value) = sqlx::query_as(
        "SELECT actor_identifier, metadata FROM admin_audit_events
         WHERE action = 'access_token_created' AND target = $1",
    )
    .bind(&simple)
    .fetch_one(pool)
    .await
    .expect("read created audit event");
    assert_eq!(actor_identifier, who.id.to_string());
    let object = metadata.as_object().expect("audit metadata is an object");
    for key in [
        "name",
        "audience",
        "scopes",
        "environment_ids",
        "expires_at",
    ] {
        assert!(object.contains_key(key), "audit metadata lacks {key}");
    }
    for key in object.keys() {
        assert!(!["token", "secret", "hash", "secret_hash"].contains(&key.as_str()));
    }
    assert_eq!(metadata["environment_ids"], json!([env.to_string()]));

    // Later reads never return the secret again.
    let list = api.list(who, "").await;
    assert_eq!(list.status, 200);
    assert_eq!(list.cache_control.as_deref(), Some("no-store"));
    assert!(!list.text.contains(secret_part(&secret)));
    assert_eq!(list.body["items"][0]["id"], id.to_string());
    let detail = api.detail(who, id).await;
    assert_eq!(detail.status, 200);
    assert_eq!(detail.cache_control.as_deref(), Some("no-store"));
    assert!(!detail.text.contains(secret_part(&secret)));
    assert!(detail.body.get("token").is_none());
    assert_eq!(
        detail.body["recent_events"][0]["action"],
        "access_token_created"
    );
    assert_eq!(
        detail.body["effective"]["environments"][0]["available"],
        true
    );
    assert_eq!(detail.body["effective"]["scopes"][0]["permitted"], true);
}

#[sqlx::test]
async fn viewer_creates_token_with_one_time_secret_and_hash_only_storage(pool: PgPool) {
    let api = start(&pool).await;
    let env = environment(&pool, "viewer-env").await;
    let viewer = member(&pool, AuthRole::Viewer, &[env]).await;
    assert_secure_create(&pool, &api, &viewer, env, "viewer agent").await;
}

#[sqlx::test]
async fn operator_creates_token_with_one_time_secret_and_hash_only_storage(pool: PgPool) {
    let api = start(&pool).await;
    let (operator, env) = operator_with_environment(&pool).await;
    assert_secure_create(&pool, &api, &operator, env, "operator agent").await;
}

#[sqlx::test]
async fn admin_creates_token_for_unassigned_environment_with_hash_only_storage(pool: PgPool) {
    let api = start(&pool).await;
    let env = environment(&pool, "admin-env").await;
    let admin_user = actor(&pool, AuthRole::Admin).await;
    // The Admin has no membership; an explicit environment id is still enough.
    assert_secure_create(&pool, &api, &admin_user, env, "admin agent").await;
}

#[sqlx::test]
async fn created_secrets_are_unique_and_never_stored_in_any_row(pool: PgPool) {
    let api = start(&pool).await;
    let (operator, env) = operator_with_environment(&pool).await;
    let (first_id, first) = create_token(&api, &operator, "first", &[env]).await;
    let (second_id, second) = create_token(&api, &operator, "second", &[env]).await;
    assert_ne!(first, second);
    assert_ne!(first_id, second_id);
    assert_secret_absent(&pool, &first).await;
    assert_secret_absent(&pool, &second).await;
    let hashes = sqlx::query_scalar::<_, Vec<u8>>("SELECT secret_hash FROM api_access_tokens")
        .fetch_all(&pool)
        .await
        .expect("read hashes");
    assert_eq!(hashes.len(), 2);
    assert!(hashes.iter().all(|hash| hash.len() == 32));
}

// ---------------------------------------------------------------------------
// Authentication boundary: CSRF, session, bearer
// ---------------------------------------------------------------------------

fn assert_csrf_rejected(reply: &Reply) {
    assert_eq!(reply.status, 403, "expected CSRF rejection: {}", reply.text);
    assert_eq!(reply.body["error"], "csrf_validation_failed");
    assert_eq!(reply.cache_control.as_deref(), Some("no-store"));
}

#[sqlx::test]
async fn mutations_reject_missing_and_mismatched_csrf_without_changing_state(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (id, _) = create_token(&api, &owner, "csrf-target", &[env]).await;
    let audits = count(
        &pool,
        "SELECT COUNT(*) FROM admin_audit_events WHERE action LIKE 'access_token_%'",
    )
    .await;

    for csrf in [Csrf::Missing, Csrf::Mismatched] {
        let body = read_body("csrf-blocked", &[env]);
        let create = api
            .raw(
                Method::POST,
                TOKENS,
                Some(&owner.session),
                csrf,
                None,
                Some(&body),
            )
            .await;
        assert_csrf_rejected(&create);
        let revoke = api
            .raw(
                Method::DELETE,
                &format!("{TOKENS}/{id}"),
                Some(&owner.session),
                csrf,
                None,
                None,
            )
            .await;
        assert_csrf_rejected(&revoke);
        let rotate = api
            .raw(
                Method::POST,
                &format!("{TOKENS}/{id}/rotate"),
                Some(&owner.session),
                csrf,
                None,
                None,
            )
            .await;
        assert_csrf_rejected(&rotate);
    }

    assert_eq!(token_count(&pool).await, 1);
    assert!(!is_revoked(&pool, id).await);
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM admin_audit_events WHERE action LIKE 'access_token_%'"
        )
        .await,
        audits
    );

    // Reads do not need the CSRF pair.
    for path in [
        POLICY.to_owned(),
        TOKENS.to_owned(),
        format!("{TOKENS}/{id}"),
    ] {
        let read = api
            .raw(
                Method::GET,
                &path,
                Some(&owner.session),
                Csrf::Missing,
                None,
                None,
            )
            .await;
        assert_eq!(read.status, 200, "{path}: {}", read.text);
    }
}

#[sqlx::test]
async fn requests_without_a_valid_session_get_401_on_all_six_endpoints(pool: PgPool) {
    let api = start(&pool).await;
    let env = environment(&pool, "anon-env").await;
    let id = Uuid::new_v4();
    let body = read_body("anonymous", &[env]);

    for session in [None, Some("not-a-real-session")] {
        for csrf in [Csrf::Valid, Csrf::Missing] {
            for (method, path) in endpoints(id) {
                let payload = (method == Method::POST).then_some(&body);
                let reply = api.raw(method, &path, session, csrf, None, payload).await;
                assert_error(&reply, 401, "unauthorized");
            }
        }
    }
    assert_nothing_issued(&pool).await;
}

#[sqlx::test]
async fn bearer_access_token_is_rejected_by_all_six_endpoints(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (id, secret) = create_token(&api, &owner, "bearer-target", &[env]).await;
    let body = read_body("bearer-blocked", &[env]);
    let bearers = [secret.as_str(), "cfat_v1_not-a-real-token"];

    for bearer in bearers {
        for session in [None, Some(owner.session.as_str())] {
            for (method, path) in endpoints(id) {
                let payload = (method == Method::POST).then_some(&body);
                let reply = api
                    .raw(method, &path, session, Csrf::Valid, Some(bearer), payload)
                    .await;
                assert_error(&reply, 401, "unauthorized");
            }
        }
    }

    // Nothing the bearer requests ran: the token is untouched and unique.
    assert_eq!(token_count(&pool).await, 1);
    assert!(!is_revoked(&pool, id).await);
    assert_eq!(token_audit_count(&pool, "access_token_created").await, 1);
    assert_eq!(token_audit_count(&pool, "access_token_revoked").await, 0);
    assert_eq!(token_audit_count(&pool, "access_token_rotated").await, 0);
}

#[sqlx::test]
async fn another_users_token_is_not_found_for_detail_revoke_and_rotate(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let intruder = member(&pool, AuthRole::Operator, &[env]).await;
    let admin_user = actor(&pool, AuthRole::Admin).await;
    let (id, _) = create_token(&api, &owner, "private", &[env]).await;

    for outsider in [&intruder, &admin_user] {
        let missing = api.detail(outsider, Uuid::new_v4()).await;
        assert_error(&missing, 404, "resource_not_found");
        for reply in [
            api.detail(outsider, id).await,
            api.revoke(outsider, id).await,
            api.rotate(outsider, id).await,
            api.detail(outsider, "not-a-uuid").await,
        ] {
            assert_error(&reply, 404, "resource_not_found");
            // Another user's token is indistinguishable from a missing one.
            assert_eq!(reply.body, missing.body);
        }
        let list = api.list(outsider, "").await;
        assert_eq!(list.status, 200);
        assert_eq!(list.body["items"], json!([]));
    }

    assert_eq!(token_count(&pool).await, 1);
    assert!(!is_revoked(&pool, id).await);
    assert_eq!(token_audit_count(&pool, "access_token_revoked").await, 0);
    assert_eq!(token_audit_count(&pool, "access_token_rotated").await, 0);
    let own = api.detail(&owner, id).await;
    assert_eq!(own.status, 200);
    assert_eq!(own.body["status"], "active");
}

// ---------------------------------------------------------------------------
// Create: validation and role bounds (400, nothing created)
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn invalid_or_unpermitted_operator_requests_are_rejected_not_narrowed(pool: PgPool) {
    let api = start(&pool).await;
    let env = environment(&pool, "bounds-ok").await;
    let unassigned = environment(&pool, "bounds-other").await;
    let inactive = environment(&pool, "bounds-off").await;
    let operator = member(&pool, AuthRole::Operator, &[env, inactive]).await;
    deactivate_environment(&pool, inactive).await;
    let unknown = Uuid::new_v4();
    let base = read_body("bounds", &[env]);
    let environments = |value: Value| with_field(&base, "environment_ids", value);
    let scopes = |value: Value| with_field(&base, "scopes", value);

    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "unassigned environment",
            environments(json!([unassigned])),
            "environment_ids",
        ),
        (
            "assigned plus unassigned environment",
            environments(json!([env, unassigned])),
            "environment_ids",
        ),
        (
            "inactive environment",
            environments(json!([inactive])),
            "environment_ids",
        ),
        (
            "duplicate environment ids",
            environments(json!([env, env])),
            "environment_ids",
        ),
        (
            "unknown environment",
            environments(json!([unknown])),
            "environment_ids",
        ),
        (
            "malformed environment id",
            environments(json!(["not-a-uuid"])),
            "environment_ids",
        ),
        (
            "empty environments",
            environments(json!([])),
            "environment_ids",
        ),
        (
            "missing environments",
            without_field(&base, "environment_ids"),
            "environment_ids",
        ),
        ("unknown scope", scopes(json!(["admin"])), "scopes"),
        (
            "unreleased poams:close",
            scopes(json!(["poams:close"])),
            "scopes",
        ),
        (
            "readable plus poams:close",
            scopes(json!(["cves:read", "poams:close"])),
            "scopes",
        ),
        (
            "duplicate scope",
            scopes(json!(["cves:read", "cves:read"])),
            "scopes",
        ),
        ("empty scopes", scopes(json!([])), "scopes"),
        (
            "lifetime 0",
            with_field(&base, "expires_in_days", json!(0)),
            "expires_in_days",
        ),
        (
            "lifetime 91",
            with_field(&base, "expires_in_days", json!(91)),
            "expires_in_days",
        ),
        (
            "lifetime -1",
            with_field(&base, "expires_in_days", json!(-1)),
            "expires_in_days",
        ),
        ("empty name", with_field(&base, "name", json!("")), "name"),
        (
            "blank name",
            with_field(&base, "name", json!("   ")),
            "name",
        ),
        (
            "name of 101 characters",
            with_field(&base, "name", json!("n".repeat(101))),
            "name",
        ),
        ("missing name", without_field(&base, "name"), "name"),
        (
            "audience cli",
            with_field(&base, "audience", json!("cli")),
            "audience",
        ),
        (
            "unknown json field",
            with_field(&base, "extra", json!(true)),
            "body",
        ),
        (
            "wrong field type",
            with_field(&base, "expires_in_days", json!("30")),
            "body",
        ),
    ];

    for (label, body, field) in cases {
        let reply = api.create(&operator, &body).await;
        assert_field_error(&reply, field);
        assert!(
            reply.body["details"]["fields"].as_object().is_some(),
            "{label}: details.fields must be an object"
        );
        assert_nothing_issued(&pool).await;
    }

    // The unmodified base request is valid, so every rejection above was
    // caused by its single mutation.
    let accepted = api.create(&operator, &base).await;
    assert_eq!(accepted.status, 201, "{}", accepted.text);
    assert_eq!(token_count(&pool).await, 1);
}

#[sqlx::test]
async fn viewer_cannot_select_operator_scopes_or_unassigned_environments(pool: PgPool) {
    let api = start(&pool).await;
    let assigned = environment(&pool, "viewer-ok").await;
    let other = environment(&pool, "viewer-other").await;
    let viewer = member(&pool, AuthRole::Viewer, &[assigned]).await;

    let unassigned = api
        .create(&viewer, &read_body("viewer-env", &[other]))
        .await;
    assert_field_error(&unassigned, "environment_ids");
    let operator_scope = api
        .create(
            &viewer,
            &create_body("viewer-scope", &[assigned], &["scans:request"], 30),
        )
        .await;
    assert_field_error(&operator_scope, "scopes");
    assert_nothing_issued(&pool).await;
}

const WRITE_SCOPES: [&str; 3] = ["scans:request", "poams:write", "poams:verify"];
const COMPLIANCE_READ_SCOPES: [&str; 2] = ["policies:read", "bundles:read"];
const AUTHORING_SCOPES: [&str; 3] = ["policies:draft", "policies:import", "bundles:draft"];

#[sqlx::test]
async fn compliance_read_scopes_issue_to_viewers_and_authoring_scopes_need_admin(pool: PgPool) {
    let api = start(&pool).await;
    let env = environment(&pool, "compliance").await;
    let viewer = member(&pool, AuthRole::Viewer, &[env]).await;
    let operator = member(&pool, AuthRole::Operator, &[env]).await;
    let admin_user = actor(&pool, AuthRole::Admin).await;

    // The database accepts the new scope names: a Viewer token stores both
    // compliance read scopes.
    let created = api
        .create(
            &viewer,
            &create_body("viewer compliance", &[env], &COMPLIANCE_READ_SCOPES, 30),
        )
        .await;
    assert_eq!(created.status, 201, "{}", created.text);
    let granted: Vec<String> = sqlx::query_scalar(
        "SELECT scope FROM api_access_token_scopes s
           JOIN api_access_tokens t ON t.id = s.token_id
          WHERE t.name = 'viewer compliance' ORDER BY scope",
    )
    .fetch_all(&pool)
    .await
    .expect("read stored scopes");
    assert_eq!(granted, ["bundles:read", "policies:read"]);

    // An authoring scope is refused with the role reason for non-Admins; an
    // Admin receives every authoring scope (TASK-534.7 released the policy
    // scopes, TASK-534.8 released bundles:draft). A refusal issues nothing.
    for scope in AUTHORING_SCOPES {
        for (who, reason) in [(&viewer, "Requires Admin"), (&operator, "Requires Admin")] {
            let denied = api
                .create(
                    who,
                    &create_body(
                        &format!("author {scope} {reason}"),
                        &[env],
                        &["policies:read", scope],
                        30,
                    ),
                )
                .await;
            assert_field_error(&denied, "scopes");
            let message = denied.body["details"]["fields"]["scopes"]
                .as_str()
                .unwrap_or_default();
            assert!(message.contains(reason), "{scope}: {message}");
        }
    }
    for scope in AUTHORING_SCOPES {
        let created = api
            .create(
                &admin_user,
                &create_body(
                    &format!("admin {scope}"),
                    &[env],
                    &["policies:read", scope],
                    30,
                ),
            )
            .await;
        assert_eq!(created.status, 201, "{scope}: {}", created.text);
    }
    // compliance:preview writes nothing and is selectable by every role.
    let preview = api
        .create(
            &viewer,
            &create_body("preview", &[env], &["compliance:preview"], 30),
        )
        .await;
    assert_eq!(preview.status, 201, "{}", preview.text);
    assert_eq!(token_count(&pool).await, 5);
}

#[sqlx::test]
async fn demoted_admin_cannot_receive_or_rotate_a_policy_authoring_scope(pool: PgPool) {
    let api = start(&pool).await;
    let env = environment(&pool, "authoring").await;
    let admin_user = actor(&pool, AuthRole::Admin).await;
    let created = api
        .create(
            &admin_user,
            &create_body("drafting agent", &[env], &["policies:draft"], 30),
        )
        .await;
    assert_eq!(created.status, 201, "{}", created.text);
    let id = created.body["id"].as_str().expect("token id").to_owned();

    // The owner becomes an Operator. A new token with the scope is refused
    // with the role reason, and the existing token cannot be rotated.
    sync_user_role(&pool, admin_user.id, AuthRole::Operator)
        .await
        .expect("demote owner");
    let denied = api
        .create(
            &admin_user,
            &create_body("drafting agent 2", &[env], &["policies:draft"], 30),
        )
        .await;
    assert_field_error(&denied, "scopes");
    assert!(
        denied.body["details"]["fields"]["scopes"]
            .as_str()
            .unwrap_or_default()
            .contains("Requires Admin"),
        "{}",
        denied.text
    );
    let rotation = api.rotate(&admin_user, &id).await;
    assert_error(&rotation, 409, "conflict");
    assert_eq!(token_count(&pool).await, 1);
}

async fn mark_production(pool: &PgPool, environment: Uuid) {
    sqlx::query("UPDATE environments SET is_production = TRUE WHERE id = $1")
        .bind(environment)
        .execute(pool)
        .await
        .expect("mark environment production");
}

async fn created_audit_metadata(pool: &PgPool, id: Uuid) -> Value {
    sqlx::query_scalar::<_, Value>(
        "SELECT metadata FROM admin_audit_events
         WHERE action = 'access_token_created' AND target = $1",
    )
    .bind(id.simple().to_string())
    .fetch_one(pool)
    .await
    .expect("read created audit event")
}

#[sqlx::test]
async fn operator_creates_each_write_scope_and_viewer_requires_operator(pool: PgPool) {
    let api = start(&pool).await;
    let (operator, env) = operator_with_environment(&pool).await;
    let viewer = member(&pool, AuthRole::Viewer, &[env]).await;

    for scope in WRITE_SCOPES {
        let denied = api
            .create(
                &viewer,
                &create_body(
                    &format!("viewer {scope}"),
                    &[env],
                    &["cves:read", scope],
                    30,
                ),
            )
            .await;
        assert_field_error(&denied, "scopes");
        let message = denied.body["details"]["fields"]["scopes"]
            .as_str()
            .unwrap_or_default();
        assert!(message.contains("Requires Operator"), "{scope}: {message}");
    }
    assert_nothing_issued(&pool).await;

    for scope in WRITE_SCOPES {
        let reply = api
            .create(
                &operator,
                &create_body(
                    &format!("operator {scope}"),
                    &[env],
                    &["cves:read", scope],
                    30,
                ),
            )
            .await;
        let (id, _) = issued(&reply);
        assert_eq!(reply.body["scopes"], json!(["cves:read", scope]));
        // A grant without a production environment needs no confirmation.
        assert_eq!(
            created_audit_metadata(&pool, id).await["production_write_confirmed"],
            false
        );
    }
    assert_eq!(token_count(&pool).await, 3);
}

#[sqlx::test]
async fn production_write_requires_explicit_confirmation_and_rotation_keeps_it(pool: PgPool) {
    let api = start(&pool).await;
    let (operator, prod) = operator_with_environment(&pool).await;
    mark_production(&pool, prod).await;
    let staging = environment(&pool, "at-staging").await;
    assign(&pool, operator.id, staging).await;

    let policy = api.policy(&operator).await;
    assert_eq!(
        environment_entry(&policy.body["environments"], prod)["is_production"],
        true
    );
    assert_eq!(
        environment_entry(&policy.body["environments"], staging)["is_production"],
        false
    );

    let body = create_body(
        "prod writer",
        &[staging, prod],
        &["cves:read", "scans:request"],
        30,
    );
    for confirmation in [None, Some(false)] {
        let request = match confirmation {
            None => body.clone(),
            Some(value) => with_field(&body, "confirm_production_write", json!(value)),
        };
        let reply = api.create(&operator, &request).await;
        assert_field_error(&reply, "confirm_production_write");
        assert_eq!(
            reply.body["details"]["fields"]["confirm_production_write"],
            "Confirm the production write access."
        );
        assert_nothing_issued(&pool).await;
    }

    // Read-only access to production needs no confirmation.
    let read_only = api
        .create(&operator, &read_body("prod reader", &[prod]))
        .await;
    let (read_id, _) = issued(&read_only);
    assert_eq!(
        created_audit_metadata(&pool, read_id).await["production_write_confirmed"],
        false
    );

    let confirmed = api
        .create(
            &operator,
            &with_field(&body, "confirm_production_write", json!(true)),
        )
        .await;
    let (id, _) = issued(&confirmed);
    assert_eq!(
        created_audit_metadata(&pool, id).await["production_write_confirmed"],
        true
    );

    // Rotation keeps the confirmed grant and asks for nothing again.
    let rotated = api.rotate(&operator, id).await;
    let (replacement, _) = issued(&rotated);
    assert_ne!(replacement, id);
    assert_eq!(
        rotated.body["scopes"],
        json!(["cves:read", "scans:request"])
    );
}

#[sqlx::test]
async fn admin_must_pass_explicit_active_environment_ids_and_released_scopes(pool: PgPool) {
    let api = start(&pool).await;
    let active = environment(&pool, "admin-on").await;
    let other = environment(&pool, "admin-other").await;
    let inactive = environment(&pool, "admin-off").await;
    deactivate_environment(&pool, inactive).await;
    let admin_user = actor(&pool, AuthRole::Admin).await;
    let base = read_body("admin-bounds", &[active]);

    for (body, field) in [
        (
            with_field(&base, "environment_ids", json!([])),
            "environment_ids",
        ),
        (without_field(&base, "environment_ids"), "environment_ids"),
        (
            with_field(&base, "environment_ids", json!([inactive])),
            "environment_ids",
        ),
        (
            with_field(&base, "environment_ids", json!([Uuid::new_v4()])),
            "environment_ids",
        ),
        (
            with_field(&base, "scopes", json!(["poams:close"])),
            "scopes",
        ),
    ] {
        assert_field_error(&api.create(&admin_user, &body).await, field);
    }
    assert_nothing_issued(&pool).await;

    // An Admin may name any active environment without a membership.
    let reply = api
        .create(&admin_user, &read_body("admin-ok", &[other, active]))
        .await;
    let (id, _) = issued(&reply);
    assert_eq!(reply.body["environments"].as_array().map(Vec::len), Some(2));
    assert_eq!(token_count(&pool).await, 1);
    assert!(!is_revoked(&pool, id).await);
}

#[sqlx::test]
async fn name_and_lifetime_boundaries_are_inclusive_and_names_are_trimmed(pool: PgPool) {
    let api = start(&pool).await;
    let (operator, env) = operator_with_environment(&pool).await;

    let one_day = api
        .create(
            &operator,
            &create_body(&"n".repeat(100), &[env], &READ_SCOPES, 1),
        )
        .await;
    let (one_day_id, _) = issued(&one_day);
    assert_eq!(one_day.body["status"], "expiring_soon");

    // Length is counted in characters, not bytes. The database CHECK uses
    // `char_length`, which counts bytes in a SQL_ASCII cluster (the sandbox
    // cluster is created with `initdb --no-locale`), so a multi-byte character
    // is only a valid probe when the server encoding is UTF8.
    let encoding: String = sqlx::query_scalar("SHOW server_encoding")
        .fetch_one(&pool)
        .await
        .expect("read server encoding");
    eprintln!("server_encoding={encoding}");
    let probe = if encoding == "UTF8" { "\u{e9}" } else { "m" };
    let unicode = api
        .create(
            &operator,
            &create_body(&probe.repeat(100), &[env], &READ_SCOPES, 90),
        )
        .await;
    let (max_id, _) = issued(&unicode);
    assert_eq!(unicode.body["status"], "active");
    let too_long = api
        .create(
            &operator,
            &create_body(&probe.repeat(101), &[env], &READ_SCOPES, 30),
        )
        .await;
    assert_field_error(&too_long, "name");

    let padded = api
        .create(&operator, &read_body("  padded  ", &[env]))
        .await;
    issued(&padded);
    assert_eq!(padded.body["name"], "padded");

    for (id, days) in [(one_day_id, 1.0_f64), (max_id, 90.0)] {
        let seconds: f64 = sqlx::query_scalar(
            "SELECT EXTRACT(EPOCH FROM (expires_at - created_at))::float8
             FROM api_access_tokens WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("read token lifetime");
        assert!(
            (seconds - days * 86_400.0).abs() < 1.0,
            "lifetime {seconds}"
        );
    }
}

// ---------------------------------------------------------------------------
// Limits: name uniqueness, live count, hourly rate, concurrency
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn token_names_are_unique_per_owner_among_non_revoked_tokens_ignoring_case(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let neighbour = member(&pool, AuthRole::Operator, &[env]).await;
    let (first_id, _) = create_token(&api, &owner, "Agent-One", &[env]).await;

    for duplicate in ["agent-one", "AGENT-ONE", "  Agent-One  "] {
        let reply = api.create(&owner, &read_body(duplicate, &[env])).await;
        assert_error(&reply, 409, "conflict");
        assert!(reply.body["details"]["fields"]["name"].is_string());
    }
    assert_eq!(token_count(&pool).await, 1);

    // Uniqueness is per owner.
    create_token(&api, &neighbour, "Agent-One", &[env]).await;

    // A revoked token frees its name.
    assert_eq!(api.revoke(&owner, first_id).await.status, 200);
    let (second_id, _) = create_token(&api, &owner, "agent-one", &[env]).await;
    assert_ne!(first_id, second_id);
    let active = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM api_access_tokens
         WHERE user_id = $1 AND revoked_at IS NULL AND lower(name) = 'agent-one'",
    )
    .bind(owner.id)
    .fetch_one(&pool)
    .await
    .expect("count active named tokens");
    assert_eq!(active, 1);
}

#[sqlx::test]
async fn live_token_limit_is_twenty_and_expired_or_revoked_tokens_do_not_count(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let seeds = seed_live_tokens(&pool, owner.id, 20).await;
    assert_eq!(seeds.len(), 20);

    let blocked = api.create(&owner, &read_body("one-too-many", &[env])).await;
    assert_error(&blocked, 409, "conflict");
    assert!(
        blocked.body["message"]
            .as_str()
            .is_some_and(|message| message.contains("20"))
    );
    assert_eq!(token_count(&pool).await, 20);

    // An expired token no longer counts.
    expire(&pool, seeds[0]).await;
    create_token(&api, &owner, "after-expiry", &[env]).await;
    let full = api.create(&owner, &read_body("full-again", &[env])).await;
    assert_error(&full, 409, "conflict");

    // A revoked token no longer counts.
    assert_eq!(api.revoke(&owner, seeds[1]).await.status, 200);
    create_token(&api, &owner, "after-revoke", &[env]).await;
    let full = api.create(&owner, &read_body("full-final", &[env])).await;
    assert_error(&full, 409, "conflict");

    let live = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM api_access_tokens
         WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(owner.id)
    .fetch_one(&pool)
    .await
    .expect("count live tokens");
    assert_eq!(live, 20);
}

#[sqlx::test]
async fn at_most_ten_creates_or_rotations_per_owner_per_hour(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let neighbour = member(&pool, AuthRole::Operator, &[env]).await;
    let mut ids = Vec::new();
    for index in 0..10 {
        ids.push(
            create_token(&api, &owner, &format!("hourly-{index}"), &[env])
                .await
                .0,
        );
    }

    let limited = api.create(&owner, &read_body("hourly-10", &[env])).await;
    assert_error(&limited, 429, "rate_limited");
    assert_eq!(token_count(&pool).await, 10);
    assert_eq!(token_audit_count(&pool, "access_token_created").await, 10);

    // A rotation is an issue operation and leaves the old token active.
    let rotation = api.rotate(&owner, ids[0]).await;
    assert_error(&rotation, 429, "rate_limited");
    assert!(!is_revoked(&pool, ids[0]).await);
    assert_eq!(token_audit_count(&pool, "access_token_rotated").await, 0);

    // Revoking is not limited, and a revoked token still counts for the hour.
    assert_eq!(api.revoke(&owner, ids[0]).await.status, 200);
    let still_limited = api.create(&owner, &read_body("hourly-11", &[env])).await;
    assert_error(&still_limited, 429, "rate_limited");

    // The limit is per owner.
    create_token(&api, &neighbour, "neighbour", &[env]).await;

    // The window rolls: tokens created more than an hour ago stop counting.
    sqlx::query(
        "UPDATE api_access_tokens SET created_at = now() - interval '2 hours' WHERE user_id = $1",
    )
    .bind(owner.id)
    .execute(&pool)
    .await
    .expect("age tokens past the rate window");
    create_token(&api, &owner, "hourly-after-window", &[env]).await;
}

#[sqlx::test]
async fn concurrent_creates_at_the_limit_never_exceed_twenty_live_tokens(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    seed_live_tokens(&pool, owner.id, 17).await;

    let replies = futures::future::join_all((0..6).map(|index| {
        let api = &api;
        let owner = &owner;
        let body = read_body(&format!("race-{index}"), &[env]);
        async move { api.create(owner, &body).await }
    }))
    .await;

    let created = replies.iter().filter(|reply| reply.status == 201).count();
    let conflicts: Vec<&Reply> = replies.iter().filter(|reply| reply.status == 409).collect();
    assert_eq!(created, 3, "exactly the free slots are issued");
    assert_eq!(conflicts.len(), 3, "the rest are rejected, never a 500");
    for reply in conflicts {
        assert_eq!(reply.body["error"], "conflict");
    }
    let live = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM api_access_tokens
         WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(owner.id)
    .fetch_one(&pool)
    .await
    .expect("count live tokens");
    assert_eq!(live, 20);
}

// ---------------------------------------------------------------------------
// List: pagination and filters
// ---------------------------------------------------------------------------

fn item_ids(reply: &Reply) -> Vec<String> {
    reply.body["items"]
        .as_array()
        .expect("list has items")
        .iter()
        .map(|item| item["id"].as_str().expect("item id").to_owned())
        .collect()
}

fn item_names(reply: &Reply) -> Vec<String> {
    let mut names: Vec<String> = reply.body["items"]
        .as_array()
        .expect("list has items")
        .iter()
        .map(|item| item["name"].as_str().expect("item name").to_owned())
        .collect();
    names.sort();
    names
}

#[sqlx::test]
async fn list_paginates_newest_first_with_has_more_and_next_offset(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let mut created = Vec::new();
    for index in 0..5 {
        created.push(
            create_token(&api, &owner, &format!("page-{index}"), &[env])
                .await
                .0,
        );
    }
    let newest_first: Vec<String> = created.iter().rev().map(Uuid::to_string).collect();

    let first = api.list(&owner, "limit=2").await;
    assert_eq!(first.status, 200);
    assert_eq!(first.cache_control.as_deref(), Some("no-store"));
    assert_eq!(item_ids(&first), newest_first[0..2]);
    assert_eq!(first.body["limit"], 2);
    assert_eq!(first.body["offset"], 0);
    assert_eq!(first.body["has_more"], true);
    assert_eq!(first.body["next_offset"], 2);

    let second = api.list(&owner, "limit=2&offset=2").await;
    assert_eq!(item_ids(&second), newest_first[2..4]);
    assert_eq!(second.body["has_more"], true);
    assert_eq!(second.body["next_offset"], 4);

    let last = api.list(&owner, "limit=2&offset=4").await;
    assert_eq!(item_ids(&last), newest_first[4..5]);
    assert_eq!(last.body["has_more"], false);
    assert!(last.body["next_offset"].is_null());

    let exact = api.list(&owner, "limit=5").await;
    assert_eq!(item_ids(&exact), newest_first);
    assert_eq!(exact.body["has_more"], false);

    let default = api.list(&owner, "").await;
    assert_eq!(default.body["limit"], 25);
    assert_eq!(default.body["offset"], 0);
    assert_eq!(item_ids(&default), newest_first);

    assert_eq!(api.list(&owner, "limit=1000").await.body["limit"], 100);
    assert_eq!(api.list(&owner, "limit=0").await.body["limit"], 1);
    assert_eq!(api.list(&owner, "offset=-5").await.body["offset"], 0);
    let beyond = api.list(&owner, "offset=99").await;
    assert!(item_ids(&beyond).is_empty());
    assert_eq!(beyond.body["has_more"], false);

    assert_field_error(&api.list(&owner, "limit=abc").await, "limit");
    assert_field_error(&api.list(&owner, "offset=x").await, "offset");
}

#[sqlx::test]
async fn list_filters_by_status_and_case_insensitive_name_query(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    for name in ["alpha-prod", "Alpha-dev", "beta"] {
        create_token(&api, &owner, name, &[env]).await;
    }
    let soon = api
        .create(&owner, &create_body("gamma-soon", &[env], &READ_SCOPES, 7))
        .await;
    issued(&soon);
    let (expired_id, _) = create_token(&api, &owner, "delta-expired", &[env]).await;
    let (revoked_id, _) = create_token(&api, &owner, "epsilon-revoked", &[env]).await;
    expire(&pool, expired_id).await;
    assert_eq!(api.revoke(&owner, revoked_id).await.status, 200);

    let all = api.list(&owner, "").await;
    assert_eq!(item_names(&all).len(), 6);
    for item in all.body["items"].as_array().expect("items") {
        let expected = match item["name"].as_str().expect("name") {
            "delta-expired" => "expired",
            "epsilon-revoked" => "revoked",
            "gamma-soon" => "expiring_soon",
            _ => "active",
        };
        assert_eq!(item["status"], expected, "status of {}", item["name"]);
    }
    assert_eq!(item_names(&api.list(&owner, "status=all").await).len(), 6);
    assert_eq!(
        item_names(&api.list(&owner, "status=active").await),
        ["Alpha-dev", "alpha-prod", "beta", "gamma-soon"]
    );
    assert_eq!(
        item_names(&api.list(&owner, "status=expired").await),
        ["delta-expired"]
    );
    assert_eq!(
        item_names(&api.list(&owner, "status=revoked").await),
        ["epsilon-revoked"]
    );
    assert_eq!(
        item_names(&api.list(&owner, "q=ALPHA").await),
        ["Alpha-dev", "alpha-prod"]
    );
    assert_eq!(
        item_names(&api.list(&owner, "q=%20beta%20").await),
        ["beta"]
    );
    assert!(item_names(&api.list(&owner, "q=alpha&status=revoked").await).is_empty());
    assert_eq!(
        item_names(&api.list(&owner, "q=ALPHA&status=active").await),
        ["Alpha-dev", "alpha-prod"]
    );
    // The query is a literal substring, not a pattern.
    assert!(item_names(&api.list(&owner, "q=%25").await).is_empty());
    assert!(item_names(&api.list(&owner, "q=_").await).is_empty());

    assert_field_error(&api.list(&owner, "status=bogus").await, "status");
    assert_field_error(
        &api.list(&owner, &format!("q={}", "q".repeat(101))).await,
        "q",
    );
}

#[sqlx::test]
async fn list_rejects_control_characters_in_the_name_query_with_400(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    create_token(&api, &owner, "alpha", &[env]).await;

    // A NUL byte used to reach PostgreSQL and fail as a 500.
    for query in ["q=%00", "q=a%00b", "q=%07", "q=a%09b", "q=%7F"] {
        let reply = api.list(&owner, query).await;
        assert_field_error(&reply, "q");
        assert_ne!(reply.status, 500, "{query}");
        assert_ne!(reply.body["error"], "internal_error", "{query}");
    }
    assert_eq!(item_names(&api.list(&owner, "q=alph").await), ["alpha"]);
}

// ---------------------------------------------------------------------------
// Revoke
// ---------------------------------------------------------------------------

async fn lifetime_seconds(pool: &PgPool, id: Uuid) -> f64 {
    sqlx::query_scalar::<_, f64>(
        "SELECT EXTRACT(EPOCH FROM (expires_at - created_at))::float8
         FROM api_access_tokens WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("read token lifetime")
}

async fn admitted(pool: &PgPool, secret: &str) -> bool {
    validate_presented_token(pool, secret, Utc::now(), &settings(MAX_AGE))
        .await
        .is_ok()
}

#[sqlx::test]
async fn revoke_is_immediate_idempotent_and_writes_exactly_one_audit_event(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (id, secret) = create_token(&api, &owner, "revoke-me", &[env]).await;
    assert!(admitted(&pool, &secret).await);

    let first = api.revoke(&owner, id).await;
    assert_eq!(first.status, 200, "{}", first.text);
    assert_eq!(first.cache_control.as_deref(), Some("no-store"));
    assert_eq!(first.body["id"], id.to_string());
    assert_eq!(first.body["status"], "revoked");
    assert!(first.body["revoked_at"].is_string());
    assert!(first.body.get("token").is_none());
    // Revocation takes effect on the very next admission check.
    assert!(!admitted(&pool, &secret).await);

    let again = api.revoke(&owner, id).await;
    assert_eq!(again.status, 200);
    assert_eq!(again.body["status"], "revoked");
    assert_eq!(again.body["revoked_at"], first.body["revoked_at"]);

    assert_eq!(token_audit_count(&pool, "access_token_revoked").await, 1);
    let (target, metadata): (String, Value) = sqlx::query_as(
        "SELECT target, metadata FROM admin_audit_events WHERE action = 'access_token_revoked'",
    )
    .fetch_one(&pool)
    .await
    .expect("read revoked audit event");
    assert_eq!(target, id.simple().to_string());
    assert_eq!(metadata["name"], "revoke-me");
    assert_secret_absent(&pool, &secret).await;

    let detail = api.detail(&owner, id).await;
    assert_eq!(detail.body["status"], "revoked");
    assert_eq!(
        detail.body["recent_events"][0]["action"],
        "access_token_revoked"
    );
    assert_eq!(
        detail.body["recent_events"][1]["action"],
        "access_token_created"
    );
    assert_error(
        &api.revoke(&owner, Uuid::new_v4()).await,
        404,
        "resource_not_found",
    );
}

#[sqlx::test]
async fn expired_token_reports_expired_status_is_rejected_and_can_be_revoked(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (id, secret) = create_token(&api, &owner, "short-lived", &[env]).await;
    assert!(admitted(&pool, &secret).await);

    expire(&pool, id).await;
    assert!(!admitted(&pool, &secret).await);
    let detail = api.detail(&owner, id).await;
    assert_eq!(detail.body["status"], "expired");
    assert_eq!(
        item_names(&api.list(&owner, "status=expired").await),
        ["short-lived"]
    );
    assert!(item_names(&api.list(&owner, "status=active").await).is_empty());

    let revoked = api.revoke(&owner, id).await;
    assert_eq!(revoked.status, 200);
    assert_eq!(revoked.body["status"], "revoked");
    assert_eq!(token_audit_count(&pool, "access_token_revoked").await, 1);
}

#[sqlx::test]
async fn revoke_takes_the_owner_row_lock_before_the_token_row_lock(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (id, _) = create_token(&api, &owner, "lock-order", &[env]).await;

    // A transaction that holds the user row `FOR UPDATE`, as a user deletion
    // does, must make revoke wait. A revoke that locked only the token row
    // would not wait here, and could deadlock with that deletion.
    let mut holder = pool.begin().await.expect("begin lock holder");
    sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(owner.id)
        .fetch_one(&mut *holder)
        .await
        .expect("lock owner row");
    let revoke = service::revoke(&pool, owner.id, id);
    tokio::pin!(revoke);
    let early = tokio::time::timeout(std::time::Duration::from_millis(500), &mut revoke).await;
    assert!(early.is_err(), "revoke must wait for the owner row lock");
    assert!(!is_revoked(&pool, id).await);

    holder.commit().await.expect("release owner row lock");
    let revoked = revoke.await.expect("revoke completes after the release");
    assert_eq!(revoked.status, "revoked");
    assert!(is_revoked(&pool, id).await);
    assert_eq!(token_audit_count(&pool, "access_token_revoked").await, 1);
}

// ---------------------------------------------------------------------------
// Rotate
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn rotate_replaces_the_token_with_the_same_grant_and_revokes_the_old_one(pool: PgPool) {
    let api = start(&pool).await;
    let env_a = environment(&pool, "rotate-a").await;
    let env_b = environment(&pool, "rotate-b").await;
    let owner = member(&pool, AuthRole::Operator, &[env_a, env_b]).await;
    let created = api
        .create(
            &owner,
            &create_body("rotating", &[env_a, env_b], &["cves:read", "poams:read"], 7),
        )
        .await;
    let (old_id, old_secret) = issued(&created);

    let rotated = api.rotate(&owner, old_id).await;
    assert_eq!(rotated.status, 201, "{}", rotated.text);
    assert_eq!(rotated.cache_control.as_deref(), Some("no-store"));
    let (new_id, new_secret) = issued(&rotated);
    assert_ne!(new_id, old_id);
    assert_ne!(new_secret, old_secret);
    assert_eq!(rotated.body["replaced_token_id"], old_id.to_string());
    assert_eq!(rotated.body["name"], "rotating");
    assert_eq!(rotated.body["audience"], "mcp");
    assert_eq!(rotated.body["scopes"], created.body["scopes"]);
    assert_eq!(rotated.body["environments"], created.body["environments"]);
    assert_eq!(rotated.body["status"], "expiring_soon");
    assert!((lifetime_seconds(&pool, new_id).await - 7.0 * 86_400.0).abs() < 1.0);

    // Old token revoked, new token active, one live token with the name.
    let old = api.detail(&owner, old_id).await;
    assert_eq!(old.body["status"], "revoked");
    assert!(is_revoked(&pool, old_id).await);
    assert!(!is_revoked(&pool, new_id).await);
    let live = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM api_access_tokens
         WHERE user_id = $1 AND revoked_at IS NULL AND name = 'rotating'",
    )
    .bind(owner.id)
    .fetch_one(&pool)
    .await
    .expect("count live tokens");
    assert_eq!(live, 1);
    assert!(!admitted(&pool, &old_secret).await);
    let validated = validate_presented_token(&pool, &new_secret, Utc::now(), &settings(MAX_AGE))
        .await
        .expect("replacement is admitted");
    assert_eq!(validated.grant.token_id(), new_id);
    assert_eq!(validated.owner_user_id, owner.id);

    // The replacement is stored as a hash only and the old token stays dead.
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT secret_hash FROM api_access_tokens WHERE id = $1")
            .bind(new_id)
            .fetch_one(&pool)
            .await
            .expect("read replacement hash");
    assert_eq!(stored, Sha256::digest(new_secret.as_bytes()).to_vec());
    assert_secret_absent(&pool, &new_secret).await;
    assert_secret_absent(&pool, &old_secret).await;
    let again = api.rotate(&owner, old_id).await;
    assert_error(&again, 409, "conflict");

    // One rotation event, targeted at the replacement and naming the old id.
    assert_eq!(token_audit_count(&pool, "access_token_created").await, 1);
    assert_eq!(token_audit_count(&pool, "access_token_rotated").await, 1);
    assert_eq!(token_audit_count(&pool, "access_token_revoked").await, 0);
    let (target, metadata): (String, Value) = sqlx::query_as(
        "SELECT target, metadata FROM admin_audit_events WHERE action = 'access_token_rotated'",
    )
    .fetch_one(&pool)
    .await
    .expect("read rotated audit event");
    assert_eq!(target, new_id.simple().to_string());
    assert_eq!(metadata["replaced_token_id"], old_id.simple().to_string());
    assert_eq!(
        metadata["replacement_token_id"],
        new_id.simple().to_string()
    );
    let old_actions: Vec<&str> = old.body["recent_events"]
        .as_array()
        .expect("recent events")
        .iter()
        .filter_map(|event| event["action"].as_str())
        .collect();
    assert!(old_actions.contains(&"access_token_rotated"));
    assert!(old_actions.contains(&"access_token_created"));

    // A replacement can itself be rotated.
    let chained = api.rotate(&owner, new_id).await;
    assert_eq!(chained.status, 201, "{}", chained.text);
    assert_eq!(chained.body["replaced_token_id"], new_id.to_string());
}

#[sqlx::test]
async fn rotate_clamps_the_replacement_lifetime_to_the_current_maximum(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (id, _) = create_token(&api, &owner, "long-lived", &[env]).await;
    assert!((lifetime_seconds(&pool, id).await - 30.0 * 86_400.0).abs() < 1.0);

    let strict = start_with(
        &pool,
        ServerConfig {
            access_tokens_default_lifetime_days: 10,
            access_tokens_max_lifetime_days: 10,
            ..ServerConfig::default()
        },
    )
    .await;
    let too_long = strict.create(&owner, &read_body("too-long", &[env])).await;
    assert_field_error(&too_long, "expires_in_days");
    let rotated = strict.rotate(&owner, id).await;
    let (new_id, _) = issued(&rotated);
    assert!((lifetime_seconds(&pool, new_id).await - 10.0 * 86_400.0).abs() < 1.0);
}

#[sqlx::test]
async fn rotate_rejects_expired_revoked_and_unknown_tokens_without_changes(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (expired_id, _) = create_token(&api, &owner, "will-expire", &[env]).await;
    let (revoked_id, _) = create_token(&api, &owner, "will-revoke", &[env]).await;
    expire(&pool, expired_id).await;
    assert_eq!(api.revoke(&owner, revoked_id).await.status, 200);

    for id in [expired_id, revoked_id] {
        assert_error(&api.rotate(&owner, id).await, 409, "conflict");
    }
    assert_error(
        &api.rotate(&owner, Uuid::new_v4()).await,
        404,
        "resource_not_found",
    );
    assert_eq!(token_count(&pool).await, 2);
    assert!(!is_revoked(&pool, expired_id).await);
    assert_eq!(token_audit_count(&pool, "access_token_rotated").await, 0);
}

#[sqlx::test]
async fn rotate_after_the_owner_lost_an_environment_conflicts_and_keeps_the_old_token(
    pool: PgPool,
) {
    let api = start(&pool).await;
    let env_a = environment(&pool, "lost-a").await;
    let env_b = environment(&pool, "lost-b").await;
    let owner = member(&pool, AuthRole::Operator, &[env_a, env_b]).await;
    let created = api
        .create(
            &owner,
            &create_body("grant-bound", &[env_a, env_b], &READ_SCOPES, 30),
        )
        .await;
    let (id, secret) = issued(&created);

    clear_user_environment_memberships(&pool, owner.id)
        .await
        .expect("clear memberships");
    assign(&pool, owner.id, env_a).await;

    let blocked = api.rotate(&owner, id).await;
    assert_error(&blocked, 409, "conflict");
    assert!(blocked.body["details"]["fields"]["environment_ids"].is_string());
    assert_eq!(token_count(&pool).await, 1);
    assert!(!is_revoked(&pool, id).await);
    assert_eq!(token_audit_count(&pool, "access_token_rotated").await, 0);

    // The old grant is immutable, but the effective view shows the loss and
    // the M0 boundary only exposes the environment the owner still has.
    let detail = api.detail(&owner, id).await;
    assert_eq!(detail.body["status"], "active");
    let availability: Vec<(String, bool)> = detail.body["effective"]["environments"]
        .as_array()
        .expect("effective environments")
        .iter()
        .map(|item| {
            (
                item["id"].as_str().expect("environment id").to_owned(),
                item["available"].as_bool().expect("availability"),
            )
        })
        .collect();
    assert_eq!(availability.len(), 2);
    for (environment_id, available) in availability {
        assert_eq!(available, environment_id == env_a.to_string());
    }
    let validated = validate_presented_token(&pool, &secret, Utc::now(), &settings(MAX_AGE))
        .await
        .expect("grant is still admitted");
    let readable = CredentialIdentity::Restricted(validated.grant).reading_environments(
        validated.owner_is_admin,
        validated.owner_environment_ids,
        &["cves:read"],
        Utc::now(),
    );
    assert_eq!(readable, Some(vec![env_a]));

    // A deactivated environment blocks rotation the same way.
    deactivate_environment(&pool, env_a).await;
    assert_error(&api.rotate(&owner, id).await, 409, "conflict");
    assert!(!is_revoked(&pool, id).await);
    assert_eq!(token_count(&pool).await, 1);

    // Revocation still works for a token whose grant is no longer permitted.
    assert_eq!(api.revoke(&owner, id).await.status, 200);
}

#[sqlx::test]
async fn concurrent_rotations_of_one_token_have_one_winner_and_one_active_replacement(
    pool: PgPool,
) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (id, _) = create_token(&api, &owner, "race-rotate", &[env]).await;

    let replies = futures::future::join_all((0..4).map(|_| {
        let api = &api;
        let owner = &owner;
        async move { api.rotate(owner, id).await }
    }))
    .await;

    let winners: Vec<&Reply> = replies.iter().filter(|reply| reply.status == 201).collect();
    let losers: Vec<&Reply> = replies.iter().filter(|reply| reply.status == 409).collect();
    assert_eq!(winners.len(), 1, "exactly one rotation succeeds");
    assert_eq!(losers.len(), 3, "the others conflict, never a 500");
    for loser in losers {
        assert_eq!(loser.body["error"], "conflict");
    }
    let winner_id = id_of(&winners[0].body);
    assert_eq!(winners[0].body["replaced_token_id"], id.to_string());

    let active: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM api_access_tokens WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(owner.id)
    .fetch_all(&pool)
    .await
    .expect("read active tokens");
    assert_eq!(active, vec![winner_id]);
    assert!(is_revoked(&pool, id).await);
    assert_eq!(token_count(&pool).await, 2);
    assert_eq!(token_audit_count(&pool, "access_token_rotated").await, 1);
}

async fn delete_token_environments(pool: &PgPool, token: Uuid) {
    sqlx::query("DELETE FROM api_access_token_environments WHERE token_id = $1")
        .bind(token)
        .execute(pool)
        .await
        .expect("delete token environment rows");
}

#[sqlx::test]
async fn rotate_of_a_token_with_no_remaining_environments_conflicts_and_changes_nothing(
    pool: PgPool,
) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (id, _) = create_token(&api, &owner, "no-environments", &[env]).await;

    // Environment rows deleted: the junction rows cascade away.
    delete_token_environments(&pool, id).await;

    let reply = api.rotate(&owner, id).await;
    assert_error(&reply, 409, "conflict");
    assert!(
        reply.body["message"]
            .as_str()
            .is_some_and(|message| message.contains("no remaining environments")),
        "unexpected message: {}",
        reply.text
    );
    assert_eq!(token_count(&pool).await, 1);
    assert!(!is_revoked(&pool, id).await);
    assert_eq!(token_audit_count(&pool, "access_token_rotated").await, 0);
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM api_access_token_environments").await,
        0
    );

    // Revocation still works for such a token.
    assert_eq!(api.revoke(&owner, id).await.status, 200);
}

#[sqlx::test]
async fn recent_events_include_target_and_replaced_token_events_newest_first(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (first_id, _) = create_token(&api, &owner, "event-order", &[env]).await;
    let (second_id, _) = issued(&api.rotate(&owner, first_id).await);
    assert_eq!(api.revoke(&owner, second_id).await.status, 200);
    // An unrelated token must not leak events into either list.
    create_token(&api, &owner, "event-other", &[env]).await;

    let actions = |reply: &Reply| -> Vec<String> {
        reply.body["recent_events"]
            .as_array()
            .expect("recent events")
            .iter()
            .map(|event| event["action"].as_str().expect("action").to_owned())
            .collect()
    };
    // The first token has its creation (target) and the rotation that names it
    // as `replaced_token_id`.
    let first = api.detail(&owner, first_id).await;
    assert_eq!(
        actions(&first),
        ["access_token_rotated", "access_token_created"]
    );
    // The replacement has the rotation (target) and its later revocation.
    let second = api.detail(&owner, second_id).await;
    assert_eq!(
        actions(&second),
        ["access_token_revoked", "access_token_rotated"]
    );
}

/// Runs `request` while another transaction holds an uncommitted live token
/// named `name` for `owner`, then commits that transaction.
///
/// The service name check cannot see the uncommitted row, so a request for the
/// same name passes the check and then waits at the unique index insert. The
/// commit makes that insert fail with a unique violation. The helper polls
/// `pg_stat_activity` for the waiting insert and falls back to committing
/// after five seconds, in which case the name check sees the row instead.
async fn with_competing_name_insert<T>(
    pool: &PgPool,
    owner: Uuid,
    name: &str,
    request: impl std::future::Future<Output = T>,
) -> T {
    let mut competing = pool.begin().await.expect("begin competing transaction");
    sqlx::query(
        "INSERT INTO api_access_tokens
             (user_id, name, audience, secret_hash, display_hint, created_at, expires_at)
         VALUES ($1, $2, 'mcp', sha256(convert_to(gen_random_uuid()::text, 'UTF8')),
                 'cfat_v1_race', now() - interval '2 hours', now() + interval '30 days')",
    )
    .bind(owner)
    .bind(name)
    .execute(&mut *competing)
    .await
    .expect("insert competing token");
    let release = async {
        for _ in 0..50 {
            let waiting = count(
                pool,
                "SELECT COUNT(*) FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock'
                   AND query LIKE 'INSERT INTO api_access_tokens%'",
            )
            .await;
            if waiting > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        competing
            .commit()
            .await
            .expect("commit competing transaction");
    };
    let (reply, ()) = futures::future::join(request, release).await;
    reply
}

#[sqlx::test]
async fn create_losing_a_name_race_at_the_unique_index_is_a_conflict_not_a_500(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;

    let reply = with_competing_name_insert(
        &pool,
        owner.id,
        "race-name",
        api.create(&owner, &read_body("Race-Name", &[env])),
    )
    .await;

    assert_error(&reply, 409, "conflict");
    assert!(
        reply.body["details"]["fields"]["name"].is_string(),
        "expected details.fields.name: {}",
        reply.text
    );
    // Only the competing token exists; the failed create left nothing behind.
    assert_eq!(token_count(&pool).await, 1);
    assert_eq!(token_audit_count(&pool, "access_token_created").await, 0);
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM api_access_token_scopes").await,
        0
    );
}

// ---------------------------------------------------------------------------
// Server-side validation of a presented token
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn a_token_with_no_environments_is_not_admitted(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (id, secret) = create_token(&api, &owner, "empty-grant", &[env]).await;
    assert!(admitted(&pool, &secret).await);

    delete_token_environments(&pool, id).await;
    let rejected = validate_presented_token(&pool, &secret, Utc::now(), &settings(MAX_AGE))
        .await
        .expect_err("a token without environments fails closed");
    let unknown = validate_presented_token(&pool, "garbage", Utc::now(), &settings(MAX_AGE))
        .await
        .expect_err("garbage is rejected");
    assert_eq!(rejected, unknown);
}

#[tokio::test]
async fn a_database_failure_during_admission_is_rejected_like_any_other_failure() {
    // No database listens on port 1, so the lookup fails with an internal
    // error. The caller still sees only the one non-disclosing rejection.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy("postgres://nobody@127.0.0.1:1/none")
        .expect("build a lazy pool");
    let token = generate_access_token(Uuid::new_v4());
    let rejected =
        validate_presented_token(&pool, token.token.expose(), Utc::now(), &settings(MAX_AGE))
            .await
            .expect_err("a database failure fails closed");
    assert_eq!(rejected.to_string(), "access token rejected");
}

#[sqlx::test]
async fn validate_presented_token_admits_a_good_token_and_reports_the_bounded_grant(pool: PgPool) {
    let api = start(&pool).await;
    let env_a = environment(&pool, "valid-a").await;
    let env_b = environment(&pool, "valid-b").await;
    let owner = member(&pool, AuthRole::Operator, &[env_a, env_b]).await;
    let created = api
        .create(
            &owner,
            &create_body("valid", &[env_a], &["cves:read", "poams:read"], 30),
        )
        .await;
    let (id, secret) = issued(&created);

    let validated = validate_presented_token(&pool, &secret, Utc::now(), &settings(MAX_AGE))
        .await
        .expect("good token is admitted");
    assert_eq!(validated.owner_user_id, owner.id);
    assert_eq!(validated.grant.token_id(), id);
    assert!(!validated.owner_is_admin);
    let mut owner_environments = validated.owner_environment_ids.clone();
    owner_environments.sort();
    let mut expected = vec![env_a, env_b];
    expected.sort();
    assert_eq!(owner_environments, expected);

    // The grant, not the owner's wider membership, bounds what is readable.
    let credential = CredentialIdentity::Restricted(validated.grant.clone());
    let both = ["cves:read", "poams:read"];
    assert_eq!(
        credential.reading_environments(
            validated.owner_is_admin,
            validated.owner_environment_ids.clone(),
            &both,
            Utc::now(),
        ),
        Some(vec![env_a])
    );
    assert_eq!(
        credential.reading_environments(
            false,
            validated.owner_environment_ids.clone(),
            &["provenance:read"],
            Utc::now(),
        ),
        None
    );
    let after_expiry = Utc::now() + chrono::Duration::days(31);
    assert_eq!(
        credential.reading_environments(false, vec![env_a], &both, after_expiry),
        None
    );
    assert!(
        validate_presented_token(&pool, &secret, after_expiry, &settings(MAX_AGE))
            .await
            .is_err()
    );
}

#[sqlx::test]
async fn validate_presented_token_rejects_every_bad_token_with_one_error(pool: PgPool) {
    let api = start(&pool).await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (good_id, good) = create_token(&api, &owner, "good", &[env]).await;
    let (expired_id, expired) = create_token(&api, &owner, "expired", &[env]).await;
    let (revoked_id, revoked) = create_token(&api, &owner, "revoked", &[env]).await;
    expire(&pool, expired_id).await;
    assert_eq!(api.revoke(&owner, revoked_id).await.status, 200);

    let (disabled_owner, disabled_env) = operator_with_environment(&pool).await;
    let (_, disabled) = create_token(&api, &disabled_owner, "disabled", &[disabled_env]).await;
    sqlx::query("UPDATE users SET is_active = FALSE WHERE id = $1")
        .bind(disabled_owner.id)
        .execute(&pool)
        .await
        .expect("disable owner");

    let (roleless_owner, roleless_env) = operator_with_environment(&pool).await;
    let (_, roleless) = create_token(&api, &roleless_owner, "roleless", &[roleless_env]).await;
    crystal_forge::queries::auth_identity::clear_user_role_assignments(&pool, roleless_owner.id)
        .await
        .expect("remove owner roles");

    let wrong_secret = generate_access_token(good_id).token.expose().to_owned();
    let unknown_id = generate_access_token(Uuid::new_v4())
        .token
        .expose()
        .to_owned();
    let cases: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("garbage", "garbage".to_owned()),
        ("prefix only", TOKEN_PREFIX.to_owned()),
        ("trailing space", format!("{good} ")),
        ("leading space", format!(" {good}")),
        ("uppercase", good.to_uppercase()),
        ("other version", good.replacen("cfat_v1_", "cfat_v2_", 1)),
        ("truncated", good[..good.len() - 1].to_owned()),
        ("bearer prefix", format!("Bearer {good}")),
        ("oversized", "x".repeat(10_000)),
        ("unknown id", unknown_id),
        ("wrong secret", wrong_secret),
        ("expired", expired),
        ("revoked", revoked),
        ("disabled owner", disabled),
        ("owner without roles", roleless),
    ];

    let mut errors: Vec<(&str, TokenRejected)> = Vec::new();
    for (label, presented) in &cases {
        match validate_presented_token(&pool, presented, Utc::now(), &settings(MAX_AGE)).await {
            Ok(_) => panic!("{label} token was admitted"),
            Err(error) => errors.push((*label, error)),
        }
    }
    let reference = errors[0].1;
    for (label, error) in &errors {
        assert_eq!(*error, reference, "{label} is distinguishable");
        assert_eq!(error.to_string(), reference.to_string(), "{label}");
        assert_eq!(format!("{error:?}"), format!("{reference:?}"), "{label}");
    }
    assert_eq!(reference.to_string(), "access token rejected");

    // The good token is untouched by every rejection above, and an expiry
    // seen only through the pinned clock is rejected the same way.
    assert!(admitted(&pool, &good).await);
    let late = validate_presented_token(
        &pool,
        &good,
        Utc::now() + chrono::Duration::days(31),
        &settings(MAX_AGE),
    )
    .await
    .expect_err("expired by clock");
    assert_eq!(late, reference);
}

#[sqlx::test]
async fn admin_owned_token_for_one_environment_cannot_read_another_environment(pool: PgPool) {
    let api = start(&pool).await;
    let env_a = environment(&pool, "grant-a").await;
    let env_b = environment(&pool, "grant-b").await;
    let admin_user = actor(&pool, AuthRole::Admin).await;
    let created = api
        .create(
            &admin_user,
            &create_body("admin-scoped", &[env_a], &["cves:read", "poams:read"], 30),
        )
        .await;
    let (_, secret) = issued(&created);

    let validated = validate_presented_token(&pool, &secret, Utc::now(), &settings(MAX_AGE))
        .await
        .expect("admin token is admitted");
    assert!(validated.owner_is_admin);
    assert!(validated.owner_environment_ids.is_empty());
    let credential = CredentialIdentity::Restricted(validated.grant);
    let scopes = ["cves:read", "poams:read"];

    // Admin bypass never widens the credential's environments.
    for owner_environments in [vec![], vec![env_b], vec![env_a, env_b]] {
        let readable = credential
            .reading_environments(true, owner_environments, &scopes, Utc::now())
            .expect("granted scopes are readable");
        assert_eq!(readable, vec![env_a]);
        assert!(!readable.contains(&env_b));
    }
    for missing in [["provenance:read"], ["poams:write"]] {
        assert_eq!(
            credential.reading_environments(true, vec![env_a], &missing, Utc::now()),
            None
        );
    }
}

// ---------------------------------------------------------------------------
// OIDC owners
// ---------------------------------------------------------------------------

/// Creates an OIDC-owned Operator whose entitlements were reconciled just now.
/// Returns the session actor, the identity id and the granted environment.
async fn oidc_operator(pool: &PgPool) -> (Actor, Uuid, Uuid) {
    let suffix = Uuid::new_v4().simple().to_string();
    let env = environment(pool, "oidc-env").await;
    let env_name: String = sqlx::query_scalar("SELECT name FROM environments WHERE id = $1")
        .bind(env)
        .fetch_one(pool)
        .await
        .expect("read environment name");
    let group = format!("at-operators-{suffix}");
    sqlx::query(
        "INSERT INTO oidc_group_mappings (group_name, role, environments)
         VALUES ($1, 'operator', ARRAY[$2]::text[])",
    )
    .bind(&group)
    .bind(&env_name)
    .execute(pool)
    .await
    .expect("insert group mapping");
    let user = insert_user(
        pool,
        &format!("at-oidc-{suffix}@example.invalid"),
        Some("Access Token Oidc"),
    )
    .await
    .expect("insert OIDC user");
    let identity: Uuid = sqlx::query_scalar(
        "INSERT INTO external_identities (user_id, provider_key, subject)
         VALUES ($1, 'oidc', $2) RETURNING id",
    )
    .bind(user.id)
    .bind(&suffix)
    .fetch_one(pool)
    .await
    .expect("insert external identity");
    reconcile_oidc(pool, user.id, identity, &group).await;
    let session = open_session(pool, user.id, "oidc").await;
    (
        Actor {
            id: user.id,
            session,
        },
        identity,
        env,
    )
}

async fn reconcile_oidc(pool: &PgPool, user: Uuid, identity: Uuid, group: &str) {
    let outcome = reconcile(
        pool,
        user,
        identity,
        reserve_entitlement_generation(pool)
            .await
            .expect("reserve generation"),
        Ok(vec![group.to_owned()]),
        &json!({ "groups": [group] }),
    )
    .await
    .expect("reconcile OIDC entitlements");
    assert_eq!(outcome, ReconciliationOutcome::Reconciled);
}

/// Returns the single group mapping that `oidc_operator` created.
async fn oidc_group(pool: &PgPool) -> String {
    sqlx::query_scalar(
        "SELECT group_name FROM oidc_group_mappings WHERE group_name LIKE 'at-operators-%'",
    )
    .fetch_one(pool)
    .await
    .expect("read OIDC group mapping")
}

async fn make_entitlements_stale(pool: &PgPool, identity: Uuid) {
    sqlx::query(
        "UPDATE external_identities
         SET last_successful_entitlement_sync_at = now() - interval '2 days'
         WHERE id = $1",
    )
    .bind(identity)
    .execute(pool)
    .await
    .expect("age entitlements");
}

fn conflict_code(error: AccessTokenError) -> String {
    match error {
        AccessTokenError::Conflict {
            details: Some(details),
            ..
        } => details["code"].as_str().expect("conflict code").to_owned(),
        other => panic!("expected an eligibility conflict, got {other:?}"),
    }
}

#[sqlx::test]
async fn oidc_owner_creates_while_fresh_and_is_blocked_over_http_when_stale(pool: PgPool) {
    // The handler reads the freshness bound from the environment; the
    // default is 24 hours, so a two-day-old sync is stale.
    let api = start(&pool).await;
    let (owner, identity, env) = oidc_operator(&pool).await;
    let policy = api.policy(&owner).await;
    assert_eq!(policy.status, 200, "{}", policy.text);
    assert!(policy.body["create_blocked"].is_null());

    let (id, _) = create_token(&api, &owner, "oidc-agent", &[env]).await;

    make_entitlements_stale(&pool, identity).await;
    let blocked_policy = api.policy(&owner).await;
    assert_eq!(blocked_policy.body["enabled"], true);
    assert_eq!(
        blocked_policy.body["create_blocked"]["code"],
        "entitlements_stale"
    );
    let blocked = api.create(&owner, &read_body("oidc-second", &[env])).await;
    assert_error(&blocked, 409, "conflict");
    assert_eq!(blocked.body["details"]["code"], "entitlements_stale");
    assert_error(&api.rotate(&owner, id).await, 409, "conflict");
    assert_eq!(token_count(&pool).await, 1);
    assert!(!is_revoked(&pool, id).await);

    // Reads and revocation stay available while issuing is blocked.
    assert_eq!(api.detail(&owner, id).await.status, 200);
    assert_eq!(api.revoke(&owner, id).await.status, 200);
}

#[sqlx::test]
async fn oidc_owner_eligibility_follows_freshness_and_ownership_at_service_level(pool: PgPool) {
    let (owner, identity, env) = oidc_operator(&pool).await;
    let request = service_request("svc-oidc", &[env], &READ_SCOPES, 30);

    let first = service::create(&pool, owner.id, &request, &settings(MAX_AGE))
        .await
        .expect("fresh OIDC owner may create");
    let first_id = first.metadata.id;

    // A zero bound denies every OIDC owner, as an unreadable bound must.
    let zero = service::create(&pool, owner.id, &request, &settings(0))
        .await
        .expect_err("zero bound denies");
    assert_eq!(conflict_code(zero), "entitlements_stale");

    make_entitlements_stale(&pool, identity).await;
    let stale = service::create(&pool, owner.id, &request, &settings(MAX_AGE))
        .await
        .expect_err("stale owner is blocked");
    assert_eq!(conflict_code(stale), "entitlements_stale");
    let stale_rotate = service::rotate(&pool, owner.id, first_id, &settings(MAX_AGE))
        .await
        .expect_err("stale owner cannot rotate");
    assert_eq!(conflict_code(stale_rotate), "entitlements_stale");
    let policy = service::policy(&pool, owner.id, &settings(MAX_AGE))
        .await
        .expect("policy is readable");
    assert_eq!(
        policy
            .create_blocked
            .as_ref()
            .map(|blocked| blocked.code.as_str()),
        Some("entitlements_stale")
    );
    assert_eq!(token_count(&pool).await, 1);
    assert!(!is_revoked(&pool, first_id).await);

    // Fresh evidence unblocks issuing.
    let group = oidc_group(&pool).await;
    reconcile_oidc(&pool, owner.id, identity, &group).await;
    service::rotate(&pool, owner.id, first_id, &settings(MAX_AGE))
        .await
        .expect("fresh owner may rotate");

    // Unresolved ownership blocks with its own code.
    sqlx::query(
        "UPDATE external_identities SET entitlement_sync_state = 'legacy_unresolved' WHERE id = $1",
    )
    .bind(identity)
    .execute(&pool)
    .await
    .expect("mark ownership unresolved");
    let unresolved = service::create(&pool, owner.id, &request, &settings(MAX_AGE))
        .await
        .expect_err("unresolved owner is blocked");
    assert_eq!(conflict_code(unresolved), "ownership_unresolved");
}

#[sqlx::test]
async fn stale_or_unresolved_oidc_owner_tokens_are_not_admitted(pool: PgPool) {
    let (owner, identity, env) = oidc_operator(&pool).await;
    let request = service_request("admit-oidc", &[env], &READ_SCOPES, 30);
    let issued = service::create(&pool, owner.id, &request, &settings(MAX_AGE))
        .await
        .expect("fresh OIDC owner may create");
    let secret = issued.secret.expose().to_owned();
    assert!(admitted(&pool, &secret).await);

    let zero = validate_presented_token(&pool, &secret, Utc::now(), &settings(0))
        .await
        .expect_err("zero bound denies");
    make_entitlements_stale(&pool, identity).await;
    let stale = validate_presented_token(&pool, &secret, Utc::now(), &settings(MAX_AGE))
        .await
        .expect_err("stale owner is not admitted");
    assert_eq!(stale, zero);

    let group = oidc_group(&pool).await;
    reconcile_oidc(&pool, owner.id, identity, &group).await;
    assert!(admitted(&pool, &secret).await);

    sqlx::query(
        "UPDATE external_identities SET entitlement_sync_state = 'legacy_unresolved' WHERE id = $1",
    )
    .bind(identity)
    .execute(&pool)
    .await
    .expect("mark ownership unresolved");
    let unresolved = validate_presented_token(&pool, &secret, Utc::now(), &settings(MAX_AGE))
        .await
        .expect_err("unresolved owner is not admitted");
    assert_eq!(unresolved, zero);
}

// ---------------------------------------------------------------------------
// Audit log, policy and the disabled switch
// ---------------------------------------------------------------------------

fn audit_items(reply: &Reply) -> &Vec<Value> {
    reply.body["items"].as_array().expect("audit items")
}

#[sqlx::test]
async fn token_audit_events_are_visible_to_an_admin_in_the_audit_log(pool: PgPool) {
    let api = start(&pool).await;
    let (operator, env) = operator_with_environment(&pool).await;
    let admin_user = actor(&pool, AuthRole::Admin).await;
    let (first_id, first_secret) = create_token(&api, &operator, "audited", &[env]).await;
    let (second_id, second_secret) = issued(&api.rotate(&operator, first_id).await);
    assert_eq!(api.revoke(&operator, second_id).await.status, 200);

    let log = api
        .call(
            Method::GET,
            "/api/v1/admin/audit-events?per_page=200",
            &admin_user,
            None,
        )
        .await;
    assert_eq!(log.status, 200, "{}", log.text);
    for (action, target) in [
        ("access_token_created", first_id),
        ("access_token_rotated", second_id),
        ("access_token_revoked", second_id),
    ] {
        let found = audit_items(&log).iter().any(|item| {
            item["action"] == action
                && item["target"] == target.simple().to_string()
                && item["actor"] == operator.id.to_string()
        });
        assert!(found, "audit log lacks {action} for {target}");
    }
    assert!(!log.text.contains(secret_part(&first_secret)));
    assert!(!log.text.contains(secret_part(&second_secret)));

    let created_only = api
        .call(
            Method::GET,
            "/api/v1/admin/audit-events?action=access_token_created&per_page=200",
            &admin_user,
            None,
        )
        .await;
    assert_eq!(created_only.status, 200, "{}", created_only.text);
    assert_eq!(audit_items(&created_only).len(), 1);
    assert_eq!(
        audit_items(&created_only)[0]["action"],
        "access_token_created"
    );

    // The log is an Admin surface; token owners do not get it.
    let denied = api
        .call(Method::GET, "/api/v1/admin/audit-events", &operator, None)
        .await;
    assert_eq!(denied.status, 403);
}

#[sqlx::test]
async fn disabled_server_blocks_issue_rotate_and_admission_but_keeps_read_policy_and_revoke(
    pool: PgPool,
) {
    let enabled = start(&pool).await;
    let disabled = start_with(
        &pool,
        ServerConfig {
            access_tokens_enable: false,
            ..ServerConfig::default()
        },
    )
    .await;
    let (owner, env) = operator_with_environment(&pool).await;
    let (id, secret) = create_token(&enabled, &owner, "pre-existing", &[env]).await;

    // The kill switch stops admission, not only issuing. The rejection is the
    // same non-disclosing error as for an unknown token.
    let on = settings(MAX_AGE);
    let off = AccessTokenSettings {
        enabled: false,
        ..on.clone()
    };
    validate_presented_token(&pool, &secret, Utc::now(), &on)
        .await
        .expect("enabled server admits the good token");
    let rejected = validate_presented_token(&pool, &secret, Utc::now(), &off)
        .await
        .expect_err("disabled server does not admit a good token");
    let unknown = validate_presented_token(&pool, "garbage", Utc::now(), &on)
        .await
        .expect_err("garbage is rejected");
    assert_eq!(rejected, unknown);

    assert_error(
        &disabled.create(&owner, &read_body("blocked", &[env])).await,
        403,
        "access_tokens_disabled",
    );
    assert_error(
        &disabled.rotate(&owner, id).await,
        403,
        "access_tokens_disabled",
    );
    assert_eq!(token_count(&pool).await, 1);
    assert!(!is_revoked(&pool, id).await);

    // List and detail are read-only, so an owner can still see what to revoke.
    let listed = disabled.list(&owner, "").await;
    assert_eq!(listed.status, 200, "{}", listed.text);
    assert_eq!(listed.cache_control.as_deref(), Some("no-store"));
    assert_eq!(item_names(&listed), ["pre-existing"]);
    let detail = disabled.detail(&owner, id).await;
    assert_eq!(detail.status, 200, "{}", detail.text);
    assert_eq!(detail.body["id"], id.to_string());
    assert_eq!(detail.body["status"], "active");

    let policy = disabled.policy(&owner).await;
    assert_eq!(policy.status, 200, "{}", policy.text);
    assert_eq!(policy.body["enabled"], false);
    assert!(policy.body["create_blocked"].is_null());

    // Nothing changed the token, so enabling the server admits it again.
    validate_presented_token(&pool, &secret, Utc::now(), &on)
        .await
        .expect("re-enabled server admits the token again");

    // An owner can always revoke a credential, even when issuing is off.
    let revoked = disabled.revoke(&owner, id).await;
    assert_eq!(revoked.status, 200, "{}", revoked.text);
    assert_eq!(revoked.body["status"], "revoked");
    assert!(!admitted(&pool, &secret).await);
}

fn policy_environment_ids(reply: &Reply) -> Vec<String> {
    let mut ids: Vec<String> = reply.body["environments"]
        .as_array()
        .expect("policy environments")
        .iter()
        .map(|item| item["id"].as_str().expect("environment id").to_owned())
        .collect();
    ids.sort();
    ids
}

fn policy_scope<'a>(reply: &'a Reply, scope: &str) -> &'a Value {
    reply.body["scopes"]
        .as_array()
        .expect("policy scopes")
        .iter()
        .find(|item| item["scope"] == scope)
        .unwrap_or_else(|| panic!("policy lacks scope {scope}"))
}

#[sqlx::test]
async fn policy_reports_limits_selectable_environments_and_role_scopes(pool: PgPool) {
    let api = start(&pool).await;
    let env_a = environment(&pool, "policy-a").await;
    let env_b = environment(&pool, "policy-b").await;
    let inactive = environment(&pool, "policy-off").await;
    let viewer = member(&pool, AuthRole::Viewer, &[env_a, inactive]).await;
    let operator = member(&pool, AuthRole::Operator, &[env_a, env_b]).await;
    let admin_user = actor(&pool, AuthRole::Admin).await;
    deactivate_environment(&pool, inactive).await;

    let viewer_policy = api.policy(&viewer).await;
    assert_eq!(viewer_policy.status, 200, "{}", viewer_policy.text);
    assert_eq!(viewer_policy.cache_control.as_deref(), Some("no-store"));
    let body = &viewer_policy.body;
    assert_eq!(body["enabled"], true);
    assert_eq!(body["mcp"]["enabled"], false);
    assert!(body["mcp"]["url"].is_null());
    assert_eq!(body["audience"], "mcp");
    assert_eq!(body["default_lifetime_days"], 30);
    assert_eq!(body["max_lifetime_days"], 90);
    assert_eq!(body["lifetime_options_days"], json!([7, 30, 90]));
    assert_eq!(body["max_active_tokens"], 20);
    assert!(body["create_blocked"].is_null());
    assert_eq!(body["scopes"].as_array().map(Vec::len), Some(13));

    // Viewer: only assigned active environments and the read scopes.
    assert_eq!(policy_environment_ids(&viewer_policy), [env_a.to_string()]);
    for scope in READ_SCOPES {
        let entry = policy_scope(&viewer_policy, scope);
        assert_eq!(entry["available"], true, "{scope}");
        assert_eq!(entry["default_selected"], true, "{scope}");
        assert!(entry["unavailable_reason"].is_null());
    }
    for scope in [
        "scans:request",
        "poams:write",
        "poams:verify",
        "poams:close",
    ] {
        let entry = policy_scope(&viewer_policy, scope);
        assert_eq!(entry["available"], false, "{scope}");
        assert_eq!(entry["default_selected"], false, "{scope}");
        assert!(entry["unavailable_reason"].is_string(), "{scope}");
    }

    // Operator: its own environments; the write scopes are released but not
    // preselected, and poams:close is not released.
    let operator_policy = api.policy(&operator).await;
    let mut expected = vec![env_a.to_string(), env_b.to_string()];
    expected.sort();
    assert_eq!(policy_environment_ids(&operator_policy), expected);
    for scope in ["scans:request", "poams:write", "poams:verify"] {
        let entry = policy_scope(&operator_policy, scope);
        assert_eq!(entry["available"], true, "{scope}");
        assert_eq!(entry["default_selected"], false, "{scope}");
        assert!(entry["unavailable_reason"].is_null(), "{scope}");
    }
    let close = policy_scope(&operator_policy, "poams:close");
    assert_eq!(close["available"], false);
    assert_eq!(close["unavailable_reason"], "Not supported yet");
    for scope in ["scans:request", "poams:write", "poams:verify"] {
        assert_eq!(
            policy_scope(&viewer_policy, scope)["unavailable_reason"],
            "Requires Operator",
            "{scope}"
        );
    }

    // Admin: every active environment (including the seeded defaults),
    // without any membership, and never an inactive one.
    let admin_policy = api.policy(&admin_user).await;
    let admin_ids = policy_environment_ids(&admin_policy);
    for id in &expected {
        assert!(
            admin_ids.contains(id),
            "admin lacks active environment {id}"
        );
    }
    assert!(!admin_ids.contains(&inactive.to_string()));

    // Compliance scopes (TASK-534.6, released by TASK-534.7 and TASK-534.8).
    // The read scopes and compliance:preview are selectable by every role and
    // never preselected. The authoring scopes report the role reason to
    // non-Admins; an Admin can select every authoring scope.
    for policy in [&viewer_policy, &operator_policy, &admin_policy] {
        for scope in COMPLIANCE_READ_SCOPES
            .into_iter()
            .chain(["compliance:preview"])
        {
            let entry = policy_scope(policy, scope);
            assert_eq!(entry["available"], true, "{scope}");
            assert_eq!(entry["default_selected"], false, "{scope}");
            assert!(entry["unavailable_reason"].is_null(), "{scope}");
        }
    }
    for scope in AUTHORING_SCOPES {
        for policy in [&viewer_policy, &operator_policy] {
            let entry = policy_scope(policy, scope);
            assert_eq!(entry["available"], false, "{scope}");
            assert_eq!(entry["default_selected"], false, "{scope}");
            assert_eq!(entry["unavailable_reason"], "Requires Admin", "{scope}");
        }
    }
    for scope in AUTHORING_SCOPES {
        let entry = policy_scope(&admin_policy, scope);
        assert_eq!(entry["available"], true, "{scope}");
        assert_eq!(entry["default_selected"], false, "{scope}");
        assert!(entry["unavailable_reason"].is_null(), "{scope}");
    }
    assert_eq!(
        policy_scope(&operator_policy, "policies:read")["label"],
        "Read security policies"
    );

    // Local owners report no OIDC ownership and no entitlement window.
    assert_eq!(body["oidc_owned"], false);
    assert!(body["entitlement_max_age_hours"].is_null());
    assert!(
        body.get("entitlement_max_age_hours").is_some(),
        "entitlement_max_age_hours is always present"
    );
}

async fn set_environment_color(pool: &PgPool, environment: Uuid, color: &str) {
    sqlx::query("UPDATE environments SET color_hex = $2 WHERE id = $1")
        .bind(environment)
        .bind(color)
        .execute(pool)
        .await
        .expect("set environment color");
}

/// Finds the entry with `id` in a JSON `environments` array.
fn environment_entry(environments: &Value, id: Uuid) -> &Value {
    environments
        .as_array()
        .expect("environments array")
        .iter()
        .find(|item| item["id"] == id.to_string())
        .unwrap_or_else(|| panic!("environments lack {id}"))
}

/// Asserts the migration default, a valid custom color and two malformed
/// stored values for the environments in `environments`.
fn assert_environment_colors(environments: &Value, ids: [Uuid; 4]) {
    let [default, custom, word, non_hex] = ids;
    assert_eq!(environment_entry(environments, default)["color"], "#6B7280");
    assert_eq!(environment_entry(environments, custom)["color"], "#f87171");
    for malformed in [word, non_hex] {
        let entry = environment_entry(environments, malformed);
        assert!(entry.get("color").is_some(), "color key is always present");
        assert!(entry["color"].is_null(), "malformed color leaked: {entry}");
    }
}

#[sqlx::test]
async fn environment_colors_are_validated_in_policy_and_token_metadata(pool: PgPool) {
    let api = start(&pool).await;
    // Created without a color, so the migration default applies.
    let default = environment(&pool, "color-default").await;
    let custom = environment(&pool, "color-custom").await;
    let word = environment(&pool, "color-word").await;
    let non_hex = environment(&pool, "color-nonhex").await;
    set_environment_color(&pool, custom, "#f87171").await;
    set_environment_color(&pool, word, "red").await;
    set_environment_color(&pool, non_hex, "#GGGGGG").await;
    let ids = [default, custom, word, non_hex];
    let owner = member(&pool, AuthRole::Operator, &ids).await;

    let policy = api.policy(&owner).await;
    assert_eq!(policy.status, 200, "{}", policy.text);
    assert_environment_colors(&policy.body["environments"], ids);

    let created = api.create(&owner, &read_body("colors", &ids)).await;
    let (id, _) = issued(&created);
    assert_environment_colors(&created.body["environments"], ids);

    let detail = api.detail(&owner, id).await;
    assert_eq!(detail.status, 200, "{}", detail.text);
    assert_environment_colors(&detail.body["environments"], ids);

    let listed = api.list(&owner, "").await;
    assert_eq!(listed.status, 200, "{}", listed.text);
    assert_environment_colors(&listed.body["items"][0]["environments"], ids);

    let rotated = api.rotate(&owner, id).await;
    assert_eq!(rotated.status, 201, "{}", rotated.text);
    assert_environment_colors(&rotated.body["environments"], ids);
}

#[sqlx::test]
async fn policy_reports_oidc_ownership_and_rounded_up_entitlement_max_age(pool: PgPool) {
    let api = start(&pool).await;

    let (local, _) = operator_with_environment(&pool).await;
    let local_policy = api.policy(&local).await;
    assert_eq!(local_policy.status, 200, "{}", local_policy.text);
    assert_eq!(local_policy.body["oidc_owned"], false);
    assert!(local_policy.body["entitlement_max_age_hours"].is_null());

    // The handler uses the 86400-second default, which is 24 hours.
    let (oidc, identity, _) = oidc_operator(&pool).await;
    let oidc_policy = api.policy(&oidc).await;
    assert_eq!(oidc_policy.status, 200, "{}", oidc_policy.text);
    assert_eq!(oidc_policy.body["oidc_owned"], true);
    assert_eq!(oidc_policy.body["entitlement_max_age_hours"], 24);

    // The window is reported even when the owner's entitlements are stale.
    make_entitlements_stale(&pool, identity).await;
    let stale_policy = api.policy(&oidc).await;
    assert_eq!(stale_policy.body["oidc_owned"], true);
    assert_eq!(stale_policy.body["entitlement_max_age_hours"], 24);
    assert_eq!(
        stale_policy.body["create_blocked"]["code"],
        "entitlements_stale"
    );

    // Service level: whole hours are exact, partial hours round up, a local
    // owner never gets a window, and disabling tokens keeps ownership visible.
    let hours = |policy: &crystal_forge::api::models::AccessTokenPolicyDto| {
        (policy.oidc_owned, policy.entitlement_max_age_hours)
    };
    let exact = service::policy(&pool, oidc.id, &settings(MAX_AGE))
        .await
        .expect("OIDC policy");
    assert_eq!(hours(&exact), (true, Some(24)));
    let partial = service::policy(&pool, oidc.id, &settings(MAX_AGE + 1))
        .await
        .expect("OIDC policy");
    assert_eq!(hours(&partial), (true, Some(25)));
    let mut disabled = settings(MAX_AGE);
    disabled.enabled = false;
    let off = service::policy(&pool, oidc.id, &disabled)
        .await
        .expect("OIDC policy while disabled");
    assert_eq!(hours(&off), (true, Some(24)));
    assert!(off.create_blocked.is_none());
    let local_service = service::policy(&pool, local.id, &settings(MAX_AGE + 1))
        .await
        .expect("local policy");
    assert_eq!(hours(&local_service), (false, None));
}
