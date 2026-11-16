//! HTTP authorization boundary of `GET /api/v1/systems/:id/flake-position`.
//!
//! The endpoint returns branch history and the hostnames of other systems, so
//! its environment scoping must hold at the HTTP layer, not only in the query:
//! an unauthenticated caller is rejected, a system in an environment the
//! caller is not a member of is indistinguishable from a missing system, and
//! peer hostnames follow the same membership rule.

use std::sync::Arc;

use axum::{Router, routing::get};
use chrono::Utc;
use crystal_forge::auth::session::{SESSION_COOKIE_NAME, hash_token};
use crystal_forge::handlers::{agent_request::CFState, api::systems};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::queries::{
    auth_identity::{create_user_session, sync_user_role},
    users::insert_user,
};
use crystal_forge::{
    config::ServerConfig, queue::QueueNotifier, server::jobs::BackgroundJobRegistry,
};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

async fn session(pool: &PgPool, role: AuthRole, member_of: Option<Uuid>) -> String {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("flake-position-{suffix}@example.invalid"),
        Some("Flake position test"),
    )
    .await
    .expect("insert user");
    sync_user_role(pool, user.id, role)
        .await
        .expect("assign role");
    if let Some(environment) = member_of {
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
        )
        .bind(user.id)
        .bind(environment)
        .execute(pool)
        .await
        .expect("membership");
    }
    let token = format!("flake-position-session-{suffix}");
    create_user_session(
        pool,
        user.id,
        hash_token(&token),
        Utc::now() + chrono::Duration::hours(1),
        Some("flake-position-test".into()),
        Some("127.0.0.1".into()),
        "local".into(),
    )
    .await
    .expect("create session");
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
            "/api/v1/systems/:id/flake-position",
            get(systems::get_system_flake_position),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{address}")
}

struct World {
    env_a: Uuid,
    subject_a: Uuid,
    subject_b: Uuid,
}

/// Three commits. `subject-a` and `peer-a` (environment A) and `peer-b` plus
/// `subject-b` (environment B) all run commit index 1.
async fn world(pool: &PgPool) -> World {
    let flake: i32 = sqlx::query_scalar(
        "INSERT INTO flakes(name,repo_url,branch) VALUES('http-lane','https://example.invalid/http-lane','main') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("flake");
    let mut commits = Vec::new();
    for index in 0..3_i32 {
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp,message,author) \
             VALUES($1,$2,now() - make_interval(hours => $3),$4,'dev') RETURNING id",
        )
        .bind(flake)
        .bind(format!("{}{index}", "b".repeat(39)))
        .bind(index)
        .bind(format!("commit {index}"))
        .fetch_one(pool)
        .await
        .expect("commit");
        commits.push(id);
    }
    let env_a: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES('http-a') RETURNING id")
            .fetch_one(pool)
            .await
            .expect("environment a");
    let env_b: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES('http-b') RETURNING id")
            .fetch_one(pool)
            .await
            .expect("environment b");
    let mut ids = std::collections::HashMap::new();
    for (host, env) in [
        ("subject-a", env_a),
        ("peer-a", env_a),
        ("peer-b", env_b),
        ("subject-b", env_b),
    ] {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname,public_key,derivation,flake_id,system_configuration_name,environment_id) \
             VALUES($1,'test-key','test-derivation',$2,$1,$3) RETURNING id",
        )
        .bind(host)
        .bind(flake)
        .bind(env)
        .fetch_one(pool)
        .await
        .expect("system");
        for (index, commit) in commits.iter().enumerate() {
            sqlx::query(
                "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path,status_id,completed_at,store_path) \
                 VALUES($1,'nixos',$2,$3,10,now(),$4)",
            )
            .bind(commit)
            .bind(host)
            .bind(format!("/nix/store/{host}-{index}.drv"))
            .bind(format!("/nix/store/{host}-{index}"))
            .execute(pool)
            .await
            .expect("derivation");
        }
        sqlx::query(
            "INSERT INTO system_states(hostname,store_path,change_reason) VALUES($1,$2,'startup')",
        )
        .bind(host)
        .bind(format!("/nix/store/{host}-1"))
        .execute(pool)
        .await
        .expect("state");
        ids.insert(host, id);
    }
    World {
        env_a,
        subject_a: ids["subject-a"],
        subject_b: ids["subject-b"],
    }
}

async fn get_position(base: &str, id: Uuid, token: Option<&str>) -> (StatusCode, Value) {
    let mut request = Client::new().get(format!("{base}/api/v1/systems/{id}/flake-position"));
    if let Some(token) = token {
        request = request.header("cookie", format!("{SESSION_COOKIE_NAME}={token}"));
    }
    let response = request.send().await.expect("request");
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

fn peers_at(body: &Value, position: usize) -> Vec<String> {
    body["commits"][position]["peers"]
        .as_array()
        .expect("peers array")
        .iter()
        .map(|peer| peer.as_str().expect("peer hostname").to_string())
        .collect()
}

#[sqlx::test]
async fn flake_position_enforces_authentication_and_environment_scope(pool: PgPool) {
    let world = world(&pool).await;
    let base = server(pool.clone()).await;
    let viewer_a = session(&pool, AuthRole::Viewer, Some(world.env_a)).await;
    let admin = session(&pool, AuthRole::Admin, None).await;

    // Unauthenticated callers are rejected before any data is read.
    let (status, _) = get_position(&base, world.subject_a, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A member reads a system in its own environment. Peers exclude hosts in
    // environments the caller is not a member of.
    let (status, body) = get_position(&base, world.subject_a, Some(&viewer_a)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["running_position"], 1);
    assert_eq!(peers_at(&body, 1), vec!["peer-a".to_string()]);

    // An existing system outside the caller's environments is hidden: the
    // response matches a missing system, so existence is not revealed.
    let (status, hidden) = get_position(&base, world.subject_b, Some(&viewer_a)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, missing) = get_position(&base, Uuid::new_v4(), Some(&viewer_a)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(hidden, missing, "hidden and missing systems look identical");

    // Admin is not environment-scoped.
    let (status, body) = get_position(&base, world.subject_a, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        peers_at(&body, 1),
        vec![
            "peer-a".to_string(),
            "peer-b".to_string(),
            "subject-b".to_string()
        ]
    );
}
