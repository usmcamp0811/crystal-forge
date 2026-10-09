//! Proves Basic read authority, scoped transport projection, and persistence.

use super::*;
use crate::models::cache_destination::nix_public_key_fixture;
use axum::body::Body;

fn fixture() -> CreateCacheDestination {
    CreateCacheDestination {
        name: "basic-read-fixture".into(),
        cache_type: "Niks3".into(),
        push_to: Some("https://read.example.com/cache".into()),
        niks3_server_url: Some("https://write.example.com".into()),
        niks3_write_auth_mode: Some("token".into()),
        niks3_auth_token: Some("synthetic-write-marker".into()),
        niks3_public_keys: vec![nix_public_key_fixture("fixture")],
        niks3_read_auth_mode: Some("basic".into()),
        niks3_read_basic_username: Some("synthetic üser \"quoted\"".into()),
        niks3_read_basic_password: Some(format!("synthetic-{}", uuid::Uuid::new_v4())),
        ..Default::default()
    }
}

fn request(body: &str) -> axum::extract::Request {
    axum::http::Request::builder()
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn snapshot(pool: &PgPool, id: i32) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_build_object('destination',to_jsonb(cd),'scope',(SELECT COALESCE(jsonb_agg(to_jsonb(cde) ORDER BY environment_id),'[]'::jsonb) FROM cache_destination_environments cde WHERE cache_destination_id=cd.id),'jobs',(SELECT count(*) FROM cache_push_jobs)) FROM cache_destinations cd WHERE id=$1")
        .bind(id).fetch_one(pool).await.unwrap()
}

#[test]
fn niks3_basic_http_projection_and_plane_scope_do_not_cross_credentials() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let create = fixture();
    create.validate().unwrap();
    let endpoint = Url::parse("https://read.example.com/cache/nix-cache-info").unwrap();
    let client = reqwest::Client::new();
    let read = niks3_read_request(&client, endpoint, &create)
        .unwrap()
        .build()
        .unwrap();
    let auth = &read.headers()[reqwest::header::AUTHORIZATION];
    assert!(auth.is_sensitive());
    let expected = format!(
        "Basic {}",
        STANDARD.encode(format!(
            "{}:{}",
            create.niks3_read_basic_username.as_deref().unwrap(),
            create.niks3_read_basic_password.as_deref().unwrap()
        ))
    );
    assert!(auth.to_str().unwrap() == expected);
    assert!(!auth.to_str().unwrap().contains("synthetic-write-marker"));
    let write = niks3_metadata_request(
        &client,
        &niks3_base_url(create.niks3_server_url.as_deref().unwrap(), false).unwrap(),
    )
    .unwrap()
    .build()
    .unwrap();
    assert!(!write.headers().contains_key(reqwest::header::AUTHORIZATION));
    let write_only: ScopedProbe<CreateCacheDestination> = serde_json::from_str(r#"{"cache_type":"Niks3","niks3_server_url":"https://write.example.com","probe_scope":"write"}"#).unwrap();
    assert_eq!(write_only.probe_scope, Niks3ProbeScope::Write);
    write_only
        .settings
        .validate_probe(write_only.probe_scope)
        .unwrap();
    assert!(write_only.settings.validate().is_err());
    assert!(
        serde_json::from_str::<ScopedProbe<CreateCacheDestination>>(
            r#"{"cache_type":"Niks3","probe_scope":"unknown"}"#
        )
        .is_err()
    );
    let all: ScopedProbe<UpdateCacheDestination> = serde_json::from_str("{}").unwrap();
    assert_eq!(all.probe_scope, Niks3ProbeScope::All);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn niks3_basic_get_save_test_cancel_modes_and_authority_preserve_secrets(pool: PgPool) {
    let admin = super::tests::admin_headers(&pool).await;
    let mut create = fixture();
    let scope: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES ('basic-read-scope') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    create.environment_ids = Some(vec![scope]);
    let current = cache_destinations::create_cache_destination(&pool, &create)
        .await
        .unwrap();
    let before = snapshot(&pool, current.id).await;
    assert!(crate::security::cache_secrets::is_encrypted(
        before["destination"]["niks3_read_basic_password"]
            .as_str()
            .unwrap()
    ));
    for response in [
        get_cache_destination(State(pool.clone()), admin.clone(), Path(current.id))
            .await
            .into_response(),
        list_cache_destinations(
            State(pool.clone()),
            admin.clone(),
            Query(ListCacheDestinationsQuery {
                enabled_only: false,
            }),
        )
        .await
        .into_response(),
    ] {
        let json = json(response).await;
        let row = if let Some(rows) = json.as_array() {
            rows.iter().find(|row| row["id"] == current.id).unwrap()
        } else {
            &json
        };
        assert_eq!(row["niks3_read_basic_configured"], true);
        assert!(row.get("niks3_read_basic_username").is_none());
        assert!(row.get("niks3_read_basic_password").is_none());
        assert!(
            !json
                .to_string()
                .contains(create.niks3_read_basic_password.as_deref().unwrap())
        );
    }
    assert!(snapshot(&pool, current.id).await == before);
    for patch in [
        serde_json::json!({"probe_scope":"read"}),
        serde_json::json!({"probe_scope":"write"}),
        serde_json::json!({"push_to":"https://read.example.com/changed-path","probe_scope":"read"}),
    ] {
        let response = test_stored_with_scoped_probe(
            &pool,
            &admin,
            current.id,
            request(&patch.to_string()),
            |effective, scope| {
                assert!(effective.niks3_read_basic_password == current.niks3_read_basic_password);
                assert!(matches!(
                    scope,
                    Niks3ProbeScope::Read | Niks3ProbeScope::Write
                ));
                std::future::ready(Ok(CacheCredentialTestResult {
                    ok: true,
                    status_code: Some(200),
                    message: "Scoped synthetic probe successful".into(),
                    tested_url: None,
                    niks3: None,
                }))
            },
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(snapshot(&pool, current.id).await == before);
    }
    // Cancel sends no server request and cannot persist the local draft.
    drop(UpdateCacheDestination {
        name: Some("discarded draft".into()),
        ..Default::default()
    });
    assert!(snapshot(&pool, current.id).await == before);
    for patch in [
        serde_json::json!({"push_to":"https://other.example.com/cache"}),
        serde_json::json!({"push_to":"https://read.example.com:8443/cache"}),
        serde_json::json!({"niks3_read_basic_username":"partial"}),
        serde_json::json!({"niks3_read_basic_username":"partial","niks3_read_basic_password":""}),
    ] {
        let called = std::sync::atomic::AtomicBool::new(false);
        let response = test_stored_with_scoped_probe(
            &pool,
            &admin,
            current.id,
            request(&patch.to_string()),
            |_, _| {
                called.store(true, std::sync::atomic::Ordering::SeqCst);
                std::future::ready(Ok(CacheCredentialTestResult {
                    ok: true,
                    status_code: None,
                    message: "must not run".into(),
                    tested_url: None,
                    niks3: None,
                }))
            },
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
        let update: UpdateCacheDestination = serde_json::from_value(patch).unwrap();
        assert!(
            cache_destinations::update_cache_destination(&pool, current.id, &update)
                .await
                .is_err()
        );
        assert!(snapshot(&pool, current.id).await == before);
    }
    cache_destinations::update_cache_destination(
        &pool,
        current.id,
        &UpdateCacheDestination {
            name: Some("renamed-basic-fixture".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    let after_save = snapshot(&pool, current.id).await;
    assert!(
        after_save["destination"]["niks3_read_basic_password"]
            == before["destination"]["niks3_read_basic_password"]
    );
    assert!(after_save["scope"] == before["scope"]);
    let changed = UpdateCacheDestination {
        push_to: Some("https://other.example.com/cache".into()),
        niks3_read_basic_username: Some("explicit replacement".into()),
        niks3_read_basic_password: Some("synthetic replacement".into()),
        ..Default::default()
    };
    let saved = cache_destinations::update_cache_destination(&pool, current.id, &changed)
        .await
        .unwrap()
        .unwrap();
    assert!(saved.niks3_read_basic_password.as_deref() == Some("synthetic replacement"));
    let cleared = cache_destinations::update_cache_destination(
        &pool,
        current.id,
        &UpdateCacheDestination {
            niks3_read_auth_mode: Some("none".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        cleared.niks3_read_basic_username.is_none() && cleared.niks3_read_basic_password.is_none()
    );
    let stored = snapshot(&pool, current.id).await;
    assert!(stored["destination"]["niks3_read_basic_password"].is_null());
    // Whitespace is a meaningful Basic password, not the legacy empty-secret
    // sentinel. It must still receive an authenticated encrypted envelope.
    let mut spaces = fixture();
    spaces.name = "basic-space-password".into();
    spaces.niks3_read_basic_password = Some("  ".into());
    let spaced = cache_destinations::create_cache_destination(&pool, &spaces)
        .await
        .unwrap();
    let raw = snapshot(&pool, spaced.id).await;
    assert!(crate::security::cache_secrets::is_encrypted(
        raw["destination"]["niks3_read_basic_password"]
            .as_str()
            .unwrap()
    ));
    assert!(spaced.niks3_read_basic_password.as_deref() == Some("  "));
}
