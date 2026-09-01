//! Environment-scope authorization regressions for TASK-433 assignment APIs.

use axum::{
    Router,
    routing::{get, post},
};
use chrono::Utc;
use crystal_forge::auth::session::{SESSION_COOKIE_NAME, hash_token};
use crystal_forge::handlers::api::compliance;
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::queries::{
    auth_identity::{create_user_session, sync_user_role},
    users::insert_user,
};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

async fn role_session(pool: &PgPool, role: AuthRole) -> (Uuid, String) {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("t433-{suffix}@x.invalid"),
        Some("TASK-433 Assignment Scope"),
    )
    .await
    .expect("insert assignment-scope user");
    sync_user_role(pool, user.id, role)
        .await
        .expect("assign role");
    let token = format!("task433-assignment-scope-{suffix}");
    create_user_session(
        pool,
        user.id,
        hash_token(&token),
        Utc::now() + chrono::Duration::hours(1),
        Some("task433-assignment-scope-test".into()),
        Some("127.0.0.1".into()),
        "local".into(),
    )
    .await
    .expect("create assignment-scope session");
    (user.id, token)
}

async fn server(pool: PgPool) -> String {
    let app = Router::new()
        .route(
            "/api/v1/compliance/assignments/:id",
            get(compliance::get_assignment),
        )
        .route(
            "/api/v1/compliance/assignments/:id/effective-policies",
            get(compliance::get_assignment_effective_policies),
        )
        .route(
            "/api/v1/compliance/assignments/preview",
            post(compliance::preview_assignment),
        )
        .route(
            "/api/v1/environments/:id/compliance-assignments",
            get(compliance::list_environment_assignments),
        )
        .route(
            "/api/v1/systems/:id/compliance-assignments",
            get(compliance::list_system_assignments),
        )
        .route(
            "/api/v1/systems/:id/effective-policies",
            get(compliance::get_system_effective_policies),
        )
        .with_state(pool);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind assignment-scope server");
    let address = listener.local_addr().expect("read server address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve assignment-scope API")
    });
    format!("http://{address}")
}

fn authenticated(request: RequestBuilder, token: &str) -> RequestBuilder {
    request.header("cookie", format!("{SESSION_COOKIE_NAME}={token}"))
}

async fn environment(pool: &PgPool, label: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(format!("task433-scope-{label}-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .expect("insert environment")
}

async fn system(pool: &PgPool, environment_id: Uuid, label: &str) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    sqlx::query_scalar(
        "INSERT INTO systems(hostname,environment_id,public_key,derivation) \
         VALUES($1,$2,'task433-scope-key',$3) RETURNING id",
    )
    .bind(format!("task433-scope-{label}-{suffix}"))
    .bind(environment_id)
    .bind(format!("/nix/store/{suffix}-task433-scope-system"))
    .fetch_one(pool)
    .await
    .expect("insert system")
}

async fn empty_bundle(pool: &PgPool, label: &str) -> (Uuid, Uuid) {
    let bundle_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundles(name,framework,version,layer,owner) \
         VALUES($1,'NIST','1.0','fleet','TASK-433') RETURNING id",
    )
    .bind(format!("task433-scope-{label}-{}", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .expect("insert bundle");
    let bundle_version_id =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id=$1")
            .bind(bundle_id)
            .fetch_one(pool)
            .await
            .expect("load generated bundle version");
    let mut tx = pool.begin().await.expect("begin fixture publication");
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id=NULL WHERE id=$1")
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("clear bundle draft pointer");
    sqlx::query(
        "UPDATE compliance_bundle_versions SET publication_state='accepted', \
         trust_state='trusted',published_at=CURRENT_TIMESTAMP,semantic_digest='task433-scope' \
         WHERE id=$1",
    )
    .bind(bundle_version_id)
    .execute(&mut *tx)
    .await
    .expect("accept bundle version");
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id=$1 WHERE id=$2")
        .bind(bundle_version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("set bundle published pointer");
    tx.commit().await.expect("commit fixture publication");
    (bundle_id, bundle_version_id)
}

async fn assignment(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    environment_id: Uuid,
) -> Uuid {
    let assignment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_assignments( \
             bundle_id,bundle_version_id,scope_type,environment_id,enforcement_mode, \
             assignment_overlay_digest) \
         VALUES($1,$2,'environment',$3,'report_only','task433-scope') \
         RETURNING id",
    )
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(environment_id)
    .fetch_one(pool)
    .await
    .expect("insert assignment lineage");
    let version_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundle_assignment_versions( \
             assignment_id,version_number,bundle_version_id,enforcement_mode,assignment_overlay_digest) \
         VALUES($1,1,$2,'report_only','task433-scope') RETURNING id",
    )
    .bind(assignment_id)
    .bind(bundle_version_id)
    .fetch_one(pool)
    .await
    .expect("insert assignment version");
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id=$2 WHERE id=$1")
        .bind(assignment_id)
        .bind(version_id)
        .execute(pool)
        .await
        .expect("set current assignment version");
    assignment_id
}

async fn assert_status(request: RequestBuilder, expected: StatusCode) {
    let response = request.send().await.expect("send assignment-scope request");
    let status = response.status();
    let body = response.text().await.expect("read assignment-scope body");
    assert_eq!(status, expected, "unexpected response body: {body}");
}

#[sqlx::test]
async fn assignment_reads_and_preview_enforce_environment_membership_without_oracles(pool: PgPool) {
    let environment_a = environment(&pool, "a").await;
    let environment_b = environment(&pool, "b").await;
    let system_a = system(&pool, environment_a, "a").await;
    let system_b = system(&pool, environment_b, "b").await;
    let (bundle_id, bundle_version_id) = empty_bundle(&pool, "visibility").await;
    let assignment_a = assignment(&pool, bundle_id, bundle_version_id, environment_a).await;
    let assignment_b = assignment(&pool, bundle_id, bundle_version_id, environment_b).await;
    let (viewer_id, viewer) = role_session(&pool, AuthRole::Viewer).await;
    let (_admin_id, admin) = role_session(&pool, AuthRole::Admin).await;
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(viewer_id)
        .bind(environment_a)
        .execute(&pool)
        .await
        .expect("grant environment A membership");

    let base = server(pool).await;
    let client = Client::new();
    let read_urls = |assignment_id, environment_id, system_id| {
        [
            format!("{base}/api/v1/compliance/assignments/{assignment_id}"),
            format!("{base}/api/v1/environments/{environment_id}/compliance-assignments"),
            format!("{base}/api/v1/systems/{system_id}/compliance-assignments"),
            format!("{base}/api/v1/systems/{system_id}/effective-policies"),
            format!("{base}/api/v1/compliance/assignments/{assignment_id}/effective-policies"),
        ]
    };

    for url in read_urls(assignment_a, environment_a, system_a) {
        assert_status(authenticated(client.get(url), &viewer), StatusCode::OK).await;
    }
    assert_status(
        authenticated(
            client
                .post(format!("{base}/api/v1/compliance/assignments/preview"))
                .json(&json!({
                    "bundle_version_id":bundle_version_id,
                    "scope_type":"environment",
                    "scope_id":environment_a
                })),
            &viewer,
        ),
        StatusCode::OK,
    )
    .await;

    for url in read_urls(assignment_b, environment_b, system_b) {
        assert_status(
            authenticated(client.get(url), &viewer),
            StatusCode::NOT_FOUND,
        )
        .await;
    }
    assert_status(
        authenticated(
            client
                .post(format!("{base}/api/v1/compliance/assignments/preview"))
                .json(&json!({
                    "bundle_version_id":bundle_version_id,
                    "scope_type":"environment",
                    "scope_id":environment_b
                })),
            &viewer,
        ),
        StatusCode::NOT_FOUND,
    )
    .await;

    for url in read_urls(assignment_b, environment_b, system_b) {
        assert_status(authenticated(client.get(url), &admin), StatusCode::OK).await;
    }
    assert_status(
        authenticated(
            client
                .post(format!("{base}/api/v1/compliance/assignments/preview"))
                .json(&json!({
                    "bundle_version_id":bundle_version_id,
                    "scope_type":"environment",
                    "scope_id":environment_b
                })),
            &admin,
        ),
        StatusCode::OK,
    )
    .await;
}
