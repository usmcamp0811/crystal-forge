//! CSRF regressions for mutation APIs introduced or materially changed by TASK-433.

use std::sync::Arc;

use axum::{
    Router,
    routing::{post, put},
};
use chrono::Utc;
use crystal_forge::auth::session::{
    CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
};
use crystal_forge::handlers::{
    agent_request::CFState,
    api::{deployment_policies, framework_requirements, systems},
};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::queries::{
    auth_identity::{create_user_session, sync_user_role},
    users::insert_user,
};
use crystal_forge::{
    config::ServerConfig, queue::QueueNotifier, server::jobs::BackgroundJobRegistry,
};
use reqwest::{Client, Method, Response};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const CSRF: &str = "task433-csrf";

async fn session(pool: &PgPool, role: AuthRole) -> String {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("task433-csrf-{suffix}@example.invalid"),
        Some("TASK-433 CSRF Test"),
    )
    .await
    .expect("insert CSRF test user");
    sync_user_role(pool, user.id, role)
        .await
        .expect("assign CSRF test role");

    let token = format!("task433-session-{suffix}");
    create_user_session(
        pool,
        user.id,
        hash_token(&token),
        Utc::now() + chrono::Duration::hours(1),
        Some("task433-csrf-test".into()),
        Some("127.0.0.1".into()),
        "local".into(),
    )
    .await
    .expect("create CSRF test session");
    token
}

async fn server(pool: PgPool) -> String {
    let state = CFState::new(
        pool,
        ServerConfig::default(),
        Arc::new(QueueNotifier::new()),
        BackgroundJobRegistry::new(),
    );
    let app = Router::new()
        .route(
            "/api/v1/deployment-policies",
            post(deployment_policies::create_deployment_policy),
        )
        .route(
            "/api/v1/deployment-policies/bulk-delete",
            post(deployment_policies::bulk_delete_deployment_policies),
        )
        .route(
            "/api/v1/deployment-policies/:id",
            put(deployment_policies::update_deployment_policy)
                .delete(deployment_policies::delete_deployment_policy),
        )
        .route(
            "/api/v1/policy-versions/:pv_id/requirement-mappings",
            post(framework_requirements::create_policy_requirement_mapping),
        )
        .route(
            "/api/v1/policy-versions/:pv_id/requirement-mappings/:m_id",
            put(framework_requirements::update_policy_requirement_mapping)
                .delete(framework_requirements::delete_policy_requirement_mapping),
        )
        .route("/api/v1/systems/:id/deploy", post(systems::deploy_system))
        .route(
            "/api/v1/systems/:id/rollback",
            post(systems::rollback_system),
        )
        .route(
            "/api/v1/systems/:id/rollback-generation",
            post(systems::rollback_system_generation),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind CSRF test server");
    let address = listener.local_addr().expect("read CSRF test address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve CSRF test API")
    });
    format!("http://{address}")
}

fn request(
    client: &Client,
    method: Method,
    url: String,
    session: &str,
    csrf_cookie: Option<&str>,
    csrf_header: Option<&str>,
    body: Value,
) -> reqwest::RequestBuilder {
    let cookie = match csrf_cookie {
        Some(csrf) => format!("{SESSION_COOKIE_NAME}={session}; {CSRF_COOKIE_NAME}={csrf}"),
        None => format!("{SESSION_COOKIE_NAME}={session}"),
    };
    let builder = client
        .request(method, url)
        .header("cookie", cookie)
        .json(&body);
    match csrf_header {
        Some(csrf) => builder.header(CSRF_HEADER_NAME.as_str(), csrf),
        None => builder,
    }
}

async fn assert_csrf_rejected(response: Response) {
    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    let body = response
        .json::<Value>()
        .await
        .expect("CSRF rejection must be structured JSON");
    assert_eq!(body["error"], "csrf_validation_failed");
}

async fn mutation_counts(pool: &PgPool) -> (i64, i64, i64) {
    let policies = sqlx::query_scalar("SELECT COUNT(*) FROM deployment_policies")
        .fetch_one(pool)
        .await
        .expect("count deployment policies");
    let mappings = sqlx::query_scalar("SELECT COUNT(*) FROM policy_requirement_mappings")
        .fetch_one(pool)
        .await
        .expect("count requirement mappings");
    let targeted_systems =
        sqlx::query_scalar("SELECT COUNT(*) FROM systems WHERE desired_target IS NOT NULL")
            .fetch_one(pool)
            .await
            .expect("count targeted systems");
    (policies, mappings, targeted_systems)
}

#[sqlx::test]
async fn task433_mutation_apis_reject_roles_without_matching_csrf(pool: PgPool) {
    let base = server(pool.clone()).await;
    let client = Client::new();
    let operator = session(&pool, AuthRole::Operator).await;
    let admin = session(&pool, AuthRole::Admin).await;
    let id = Uuid::new_v4();
    let mapping_id = Uuid::new_v4();
    let before = mutation_counts(&pool).await;

    let cases = [
        (
            Method::POST,
            format!("{base}/api/v1/deployment-policies"),
            operator.as_str(),
            None,
            None,
            json!({"name":"blocked-create","policy_type":"custom_check","config":{"mode":"all","rules":[]}}),
        ),
        (
            Method::PUT,
            format!("{base}/api/v1/deployment-policies/{id}"),
            operator.as_str(),
            Some(CSRF),
            Some("wrong"),
            json!({"name":"blocked-update"}),
        ),
        (
            Method::POST,
            format!("{base}/api/v1/deployment-policies/bulk-delete"),
            admin.as_str(),
            None,
            None,
            json!({"policy_ids":[id]}),
        ),
        (
            Method::POST,
            format!("{base}/api/v1/policy-versions/{id}/requirement-mappings"),
            operator.as_str(),
            Some(CSRF),
            Some("wrong"),
            json!({"requirement_version_id":mapping_id,"relationship":"implements","coverage":"full"}),
        ),
        (
            Method::PUT,
            format!("{base}/api/v1/policy-versions/{id}/requirement-mappings/{mapping_id}"),
            operator.as_str(),
            None,
            None,
            json!({"relationship":"supports","coverage":"partial"}),
        ),
        (
            Method::DELETE,
            format!("{base}/api/v1/policy-versions/{id}/requirement-mappings/{mapping_id}"),
            operator.as_str(),
            Some(CSRF),
            Some("wrong"),
            json!(null),
        ),
        (
            Method::POST,
            format!("{base}/api/v1/systems/{id}/deploy"),
            operator.as_str(),
            None,
            None,
            json!({"commit_sha":"a1b2c3d"}),
        ),
        (
            Method::POST,
            format!("{base}/api/v1/systems/{id}/rollback"),
            operator.as_str(),
            Some(CSRF),
            Some("wrong"),
            json!({"target_commit":"a1b2c3d"}),
        ),
        (
            Method::POST,
            format!("{base}/api/v1/systems/{id}/rollback-generation"),
            operator.as_str(),
            None,
            None,
            json!({"store_path":"/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-system"}),
        ),
    ];

    for (method, url, token, cookie, header, body) in cases {
        let response = request(&client, method, url, token, cookie, header, body)
            .send()
            .await
            .expect("send scoped mutation request");
        assert_csrf_rejected(response).await;
    }

    assert_eq!(mutation_counts(&pool).await, before);
}

#[sqlx::test]
async fn single_policy_delete_rejects_missing_and_mismatched_csrf_without_deleting(pool: PgPool) {
    let base = server(pool.clone()).await;
    let client = Client::new();
    let admin = session(&pool, AuthRole::Admin).await;
    let policy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
         VALUES ($1, 'custom_check', '{\"mode\":\"all\",\"rules\":[]}', FALSE) \
         RETURNING id",
    )
    .bind(format!("csrf-delete-{}", Uuid::new_v4()))
    .fetch_one(&pool)
    .await
    .expect("insert deletable CSRF fixture policy");

    for (cookie, header) in [(None, None), (Some(CSRF), Some("wrong"))] {
        let response = request(
            &client,
            Method::DELETE,
            format!("{base}/api/v1/deployment-policies/{policy_id}"),
            &admin,
            cookie,
            header,
            json!(null),
        )
        .send()
        .await
        .expect("send single-policy DELETE without matching CSRF");
        assert_csrf_rejected(response).await;

        let remains: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM deployment_policies WHERE id = $1)")
                .bind(policy_id)
                .fetch_one(&pool)
                .await
                .expect("check rejected DELETE preserved policy");
        assert!(remains, "CSRF rejection must occur before policy deletion");
    }
}

#[sqlx::test]
async fn matching_csrf_preserves_existing_role_and_resource_checks(pool: PgPool) {
    let base = server(pool.clone()).await;
    let client = Client::new();
    let viewer = session(&pool, AuthRole::Viewer).await;
    let operator = session(&pool, AuthRole::Operator).await;
    let admin = session(&pool, AuthRole::Admin).await;
    let id = Uuid::new_v4();

    let viewer_response = request(
        &client,
        Method::POST,
        format!("{base}/api/v1/deployment-policies"),
        &viewer,
        Some(CSRF),
        Some(CSRF),
        json!({"name":"viewer-create","policy_type":"custom_check","config":{"mode":"all","rules":[]}}),
    )
    .send()
    .await
    .expect("send viewer policy request");
    assert_eq!(viewer_response.status(), reqwest::StatusCode::FORBIDDEN);
    assert_ne!(
        viewer_response
            .json::<Value>()
            .await
            .expect("viewer rejection JSON")["error"],
        "csrf_validation_failed"
    );

    let operator_response = request(
        &client,
        Method::POST,
        format!("{base}/api/v1/systems/{id}/deploy"),
        &operator,
        Some(CSRF),
        Some(CSRF),
        json!({"commit_sha":"a1b2c3d"}),
    )
    .send()
    .await
    .expect("send operator deployment request");
    assert_eq!(operator_response.status(), reqwest::StatusCode::NOT_FOUND);

    let admin_response = request(
        &client,
        Method::POST,
        format!("{base}/api/v1/deployment-policies/bulk-delete"),
        &admin,
        Some(CSRF),
        Some(CSRF),
        json!({"policy_ids":[]}),
    )
    .send()
    .await
    .expect("send admin bulk-delete request");
    assert_eq!(admin_response.status(), reqwest::StatusCode::OK);
}
