//! Disposable-database and wire-contract regressions for operator authority.

use super::*;
use crate::models::auth_identity::AuthRole;

async fn mutation_headers(pool: &PgPool, role: AuthRole, suffix: &str) -> HeaderMap {
    use crate::auth::session::{
        CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
    };
    let user =
        crate::queries::users::insert_user(pool, &format!("{suffix}@lifecycle.invalid"), None)
            .await
            .unwrap();
    crate::queries::auth_identity::sync_user_role(pool, user.id, role)
        .await
        .unwrap();
    let token = format!("lifecycle-session-{suffix}");
    crate::queries::auth_identity::create_user_session(
        pool,
        user.id,
        hash_token(&token),
        chrono::Utc::now() + chrono::Duration::hours(1),
        None,
        None,
        "local".into(),
    )
    .await
    .unwrap();
    let csrf = format!("lifecycle-csrf-{suffix}");
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        format!("{SESSION_COOKIE_NAME}={token}; {CSRF_COOKIE_NAME}={csrf}")
            .parse()
            .unwrap(),
    );
    headers.insert(CSRF_HEADER_NAME.clone(), csrf.parse().unwrap());
    headers
}

#[test]
fn lifecycle_request_requires_explicit_nullable_boolean_and_rejects_payloads() {
    for value in [
        serde_json::json!(true),
        serde_json::json!(false),
        serde_json::Value::Null,
    ] {
        let request: SetEvaluationLifecycleRequest =
            serde_json::from_value(serde_json::json!({"greenfield": value})).unwrap();
        assert_eq!(serde_json::to_value(request.greenfield).unwrap(), value);
    }
    for value in [
        serde_json::json!({}),
        serde_json::json!({"greenfield": "false"}),
        serde_json::json!({"greenfield": false, "actor": "admin"}),
        serde_json::json!({"greenfield": {"expression": "secret"}}),
    ] {
        assert!(serde_json::from_value::<SetEvaluationLifecycleRequest>(value).is_err());
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn lifecycle_mutation_scopes_authority_audits_and_cleans_up(pool: PgPool) {
    let env: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES ('lifecycle-scope') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let foreign: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES ('lifecycle-foreign') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let actor = crate::queries::users::insert_user(&pool, "lifecycle@example.invalid", None)
        .await
        .unwrap()
        .id;
    sqlx::query(
        "INSERT INTO user_environment_memberships(user_id, environment_id) VALUES ($1, $2)",
    )
    .bind(actor)
    .bind(env)
    .execute(&pool)
    .await
    .unwrap();
    let system: Uuid = sqlx::query_scalar("INSERT INTO systems(hostname,environment_id,public_key,derivation) VALUES ('lifecycle', $1, 'test', '/nix/store/test') RETURNING id")
        .bind(env).fetch_one(&pool).await.unwrap();
    assert!(
        !set_classification(&pool, actor, Role::Viewer, system, Some(true))
            .await
            .unwrap()
    );
    assert!(
        !set_classification(&pool, actor, Role::Operator, Uuid::new_v4(), Some(true))
            .await
            .unwrap()
    );
    for value in [Some(true), Some(false), None] {
        assert!(
            set_classification(&pool, actor, Role::Operator, system, value)
                .await
                .unwrap()
        );
        let row: (Option<bool>, Option<Uuid>, bool) = sqlx::query_as(
            "SELECT greenfield, classified_by, classified_at IS NOT NULL FROM system_evaluation_lifecycle WHERE system_id=$1")
            .bind(system).fetch_one(&pool).await.unwrap();
        assert_eq!(row, (value, Some(actor), true));
    }
    let events: Vec<(Option<Uuid>, Option<String>, serde_json::Value)> = sqlx::query_as(
        "SELECT actor_user_id,actor_identifier,metadata FROM admin_audit_events WHERE action='system_evaluation_lifecycle_updated' ORDER BY created_at")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(events.len(), 3);
    for (event, value) in events.iter().zip([Some(true), Some(false), None]) {
        assert_eq!(event.0, Some(actor));
        assert_eq!(event.1.as_deref(), Some(actor.to_string().as_str()));
        assert_eq!(
            event.2,
            serde_json::json!({"system_id":system,"greenfield":value})
        );
    }
    // Current moves and revocation remove operator authority. Admin bypass is
    // exactly the existing Role environment policy, including unscoped systems.
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(system)
        .bind(foreign)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        !set_classification(&pool, actor, Role::Operator, system, Some(true))
            .await
            .unwrap()
    );
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(system)
        .bind(env)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1")
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        !set_classification(&pool, actor, Role::Operator, system, Some(true))
            .await
            .unwrap()
    );
    sqlx::query("UPDATE systems SET environment_id=NULL WHERE id=$1")
        .bind(system)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        !set_classification(&pool, actor, Role::Operator, system, Some(true))
            .await
            .unwrap()
    );
    assert!(
        set_classification(&pool, actor, Role::Admin, system, Some(false))
            .await
            .unwrap()
    );
    sqlx::query("DELETE FROM systems WHERE id=$1")
        .bind(system)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM system_evaluation_lifecycle")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn lifecycle_handler_enforces_auth_csrf_and_nondisclosing_scope(pool: PgPool) {
    let system: Uuid = sqlx::query_scalar("INSERT INTO systems(hostname,public_key,derivation) VALUES ('lifecycle-handler', 'test', '/nix/store/test') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    let call = |headers, id| {
        put_evaluation_lifecycle(
            State(pool.clone()),
            headers,
            Path(id),
            Json(SetEvaluationLifecycleRequest {
                greenfield: Some(true),
            }),
        )
    };
    assert_eq!(
        call(HeaderMap::new(), system).await.status(),
        StatusCode::FORBIDDEN
    );
    let viewer = mutation_headers(&pool, AuthRole::Viewer, "lifecycle-viewer").await;
    assert_eq!(call(viewer, system).await.status(), StatusCode::FORBIDDEN);
    let operator = mutation_headers(&pool, AuthRole::Operator, "lifecycle-operator").await;
    assert_eq!(
        call(operator.clone(), system).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(operator.clone(), Uuid::new_v4()).await.status(),
        StatusCode::NOT_FOUND
    );
    let mut admin = mutation_headers(&pool, AuthRole::Admin, "lifecycle-admin").await;
    admin.remove(crate::auth::session::CSRF_HEADER_NAME.clone());
    assert_eq!(call(admin, system).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM system_evaluation_lifecycle")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let actor: Uuid = sqlx::query_scalar(
        "SELECT id FROM users WHERE email='lifecycle-operator@lifecycle.invalid'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let env: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES('handler-member') RETURNING id")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(system)
        .bind(env)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(actor)
        .bind(env)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(call(operator, system).await.status(), StatusCode::OK);
    let recorded: (Option<bool>, Option<Uuid>) = sqlx::query_as(
        "SELECT greenfield,classified_by FROM system_evaluation_lifecycle WHERE system_id=$1",
    )
    .bind(system)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recorded, (Some(true), Some(actor)));
    assert_eq!(sqlx::query_scalar::<_, Option<Uuid>>("SELECT actor_user_id FROM admin_audit_events WHERE action='system_evaluation_lifecycle_updated'")
        .fetch_one(&pool).await.unwrap(), Some(actor));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
async fn lifecycle_audit_failure_rolls_back_classification(pool: PgPool) {
    let actor = crate::queries::users::insert_user(&pool, "rollback@lifecycle.invalid", None)
        .await
        .unwrap()
        .id;
    let system: Uuid = sqlx::query_scalar("INSERT INTO systems(hostname,public_key,derivation) VALUES ('lifecycle-rollback', 'test', '/nix/store/test') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    assert!(
        set_classification(&pool, actor, Role::Admin, system, Some(false))
            .await
            .unwrap()
    );
    sqlx::query("CREATE FUNCTION reject_lifecycle_test_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test audit failure'; END $$")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_lifecycle_test_audit BEFORE INSERT ON admin_audit_events FOR EACH ROW EXECUTE FUNCTION reject_lifecycle_test_audit()")
        .execute(&pool).await.unwrap();
    assert!(
        set_classification(&pool, actor, Role::Admin, system, Some(true))
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, Option<bool>>(
            "SELECT greenfield FROM system_evaluation_lifecycle WHERE system_id=$1"
        )
        .bind(system)
        .fetch_one(&pool)
        .await
        .unwrap(),
        Some(false)
    );
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM admin_audit_events WHERE action='system_evaluation_lifecycle_updated'")
        .fetch_one(&pool).await.unwrap(), 1);
}
