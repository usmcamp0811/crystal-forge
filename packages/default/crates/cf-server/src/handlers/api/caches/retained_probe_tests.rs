//! Exercises retained-credential probes without persistent or global injection.

use super::*;
use crate::models::cache_destination::nix_public_key_fixture;
use crate::security::cache_secrets::{TEST_CERTIFICATE, is_encrypted};
use axum::{body::Body, http::Request, routing::post};
use tower::ServiceExt;

fn request(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/caches/1/test-credentials")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn fixture(ty: &str) -> CreateCacheDestination {
    let mut create = CreateCacheDestination {
        name: format!("probe-{ty}"),
        cache_type: ty.into(),
        push_to: Some("https://cache.example.invalid/cache".into()),
        ..Default::default()
    };
    match ty {
        "Attic" => {
            create.attic_token = Some("fixture-attic-marker".into());
            create.attic_cache_name = Some("fixture-cache".into());
            create.attic_public_key = Some(nix_public_key_fixture("fixture"));
        }
        "S3" => {
            create.push_to = Some("s3://fixture-bucket".into());
            create.s3_endpoint_url = Some("https://objects.example.invalid".into());
            create.s3_region = Some("fixture-region".into());
            create.s3_access_key_id = Some("fixture-access-marker".into());
            create.s3_secret_access_key = Some("fixture-secret-marker".into());
            create.s3_session_token = Some("fixture-session-marker".into());
        }
        "Niks3" => {
            create.niks3_server_url = Some("https://write.example.invalid".into());
            create.niks3_public_keys = vec![nix_public_key_fixture("fixture")];
            create.niks3_write_auth_mode = Some("token".into());
            create.niks3_auth_token = Some("fixture-write-marker".into());
            create.niks3_read_auth_mode = Some("mtls".into());
            create.niks3_read_client_cert = Some(TEST_CERTIFICATE.into());
            create.niks3_read_client_key = Some("fixture-read-key-marker".into());
        }
        _ => {}
    }
    create
}

async fn snapshot(pool: &PgPool, id: i32) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_build_object('destination', to_jsonb(cd), 'scope', (SELECT COALESCE(jsonb_agg(to_jsonb(cde) ORDER BY environment_id), '[]'::jsonb) FROM cache_destination_environments cde WHERE cde.cache_destination_id = cd.id), 'jobs', (SELECT count(*) FROM cache_push_jobs)) FROM cache_destinations cd WHERE id = $1")
        .bind(id).fetch_one(pool).await.unwrap()
}

async fn json_response(response: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn stored_probe_preserves_ciphertext_scope_and_timestamps(pool: PgPool) {
    let headers = super::tests::admin_headers(&pool).await;
    let scope: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO environments (name) VALUES ('probe-scope') RETURNING id")
            .fetch_one(&pool)
            .await
            .unwrap();
    for ty in ["Attic", "S3", "Niks3"] {
        let mut create = fixture(ty);
        create.environment_ids = Some(vec![scope]);
        let destination = cache_destinations::create_cache_destination(&pool, &create)
            .await
            .unwrap();
        let id = destination.id;
        let before = snapshot(&pool, id).await;
        let secret_fields = match ty {
            "Attic" => vec!["attic_token"],
            "S3" => vec![
                "s3_access_key_id",
                "s3_secret_access_key",
                "s3_session_token",
            ],
            _ => vec!["niks3_auth_token", "niks3_read_client_key"],
        };
        for field in secret_fields {
            assert!(is_encrypted(before["destination"][field].as_str().unwrap()));
        }
        let replacement = match ty {
            "Attic" => {
                serde_json::json!({"attic_token":"replacement-attic-marker", "environment_ids": []})
            }
            "S3" => {
                serde_json::json!({"s3_access_key_id":"replacement-access-marker", "s3_secret_access_key":"replacement-secret-marker", "s3_session_token":"replacement-session-marker", "environment_ids": []})
            }
            _ => {
                serde_json::json!({"niks3_write_auth_mode":"mtls", "niks3_write_client_cert": TEST_CERTIFICATE, "niks3_write_client_key":"replacement-write-key-marker", "niks3_read_auth_mode":"none", "environment_ids": []})
            }
        };
        for patch in [serde_json::json!({}), replacement] {
            let expected: UpdateCacheDestination = serde_json::from_value(patch.clone()).unwrap();
            let expected = cache_destinations::effective_update(&destination, &expected).unwrap();
            let probe_pool = pool.clone();
            let response = test_stored_with_probe(
                &pool,
                &headers,
                id,
                request(&patch.to_string()),
                |effective| async move {
                    assert_eq!(
                        serde_json::to_value(&effective).unwrap(),
                        serde_json::to_value(&expected).unwrap()
                    );
                    // A different connection can acquire the write lock during the
                    // probe. The handler cannot hold a publication snapshot lock.
                    let mut tx = probe_pool.begin().await.unwrap();
                    sqlx::query(
                        "SELECT id FROM cache_destinations WHERE id = $1 FOR UPDATE NOWAIT",
                    )
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap();
                    tx.rollback().await.unwrap();
                    if ty == "S3" {
                        let now = chrono::Utc::now();
                        let (_, sent) = s3_probe::signed_bucket_request(&effective, now).unwrap();
                        let (_, expected_headers) =
                            s3_probe::signed_bucket_request(&expected, now).unwrap();
                        assert_eq!(sent, expected_headers);
                        assert!(
                            sent[reqwest::header::AUTHORIZATION]
                                .to_str()
                                .unwrap()
                                .contains(effective.s3_access_key_id.as_deref().unwrap())
                        );
                    }
                    if ty == "Niks3" && effective.niks3_write_auth_mode.as_deref() == Some("mtls") {
                        assert!(effective.niks3_auth_token.is_none());
                        assert!(effective.niks3_read_client_key.is_none());
                        assert!(effective.niks3_read_client_cert.is_none());
                    }
                    Ok(CacheCredentialTestResult {
                        ok: true,
                        status_code: Some(200),
                        message: "Read-only fixture probe successful".into(),
                        tested_url: None,
                        niks3: None,
                    })
                },
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let body = json_response(response).await.to_string();
            assert!(!body.contains("marker"));
            assert!(!body.contains("configuration"));
            assert_eq!(snapshot(&pool, id).await, before);
        }
        let response = get_cache_destination(State(pool.clone()), headers.clone(), Path(id))
            .await
            .into_response();
        let dto = json_response(response).await;
        assert_eq!(dto["attic_token_configured"], ty == "Attic");
        assert_eq!(dto["s3_credentials_configured"], ty == "S3");
        assert_eq!(dto["s3_session_token_configured"], ty == "S3");
        assert!(!dto.to_string().contains("marker"));
        assert_eq!(snapshot(&pool, id).await, before);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn stored_probe_admin_json_missing_id_conversion_and_ssrf(pool: PgPool) {
    let admin = super::tests::admin_headers(&pool).await;
    let destination = cache_destinations::create_cache_destination(&pool, &fixture("Attic"))
        .await
        .unwrap();
    // Deliberately populate historical inactive fields. Conversions must never
    // turn those old fields into active credential selection.
    sqlx::query("UPDATE cache_destinations SET s3_access_key_id=$2, s3_secret_access_key=$3, s3_session_token=$4, s3_region='fixture-region', s3_endpoint_url='https://objects.example.invalid' WHERE id=$1")
        .bind(destination.id).bind("inactive-access-marker").bind("inactive-secret-marker").bind("inactive-session-marker").execute(&pool).await.unwrap();
    let before = snapshot(&pool, destination.id).await;
    let router = axum::Router::new().route(
        "/api/v1/caches/:id/test-credentials",
        post({
            let pool = pool.clone();
            move |headers: HeaderMap, Path(id): Path<i32>, request: axum::extract::Request| {
                test_stored_cache_destination_credentials(
                    State(pool.clone()),
                    State(ServerConfig::default()),
                    headers,
                    Path(id),
                    request,
                )
            }
        }),
    );
    for (authorized, id, body, status) in [
        (
            false,
            destination.id,
            "not-json-fixture-secret",
            StatusCode::FORBIDDEN,
        ),
        (
            false,
            i32::MAX,
            "not-json-fixture-secret",
            StatusCode::FORBIDDEN,
        ),
        (true, i32::MAX, "{}", StatusCode::NOT_FOUND),
        (
            true,
            destination.id,
            r#"{"attic_token":123456}"#,
            StatusCode::BAD_REQUEST,
        ),
        (
            true,
            destination.id,
            r#"{"cache_type":"S3","push_to":"s3://fixture-bucket"}"#,
            StatusCode::BAD_REQUEST,
        ),
        (
            true,
            destination.id,
            r#"{"push_to":"https://127.0.0.1"}"#,
            StatusCode::BAD_REQUEST,
        ),
        (
            true,
            destination.id,
            r#"{"push_to":"https://user:fixture-secret@cache.example"}"#,
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let mut req = request(body);
        *req.uri_mut() = format!("/api/v1/caches/{id}/test-credentials")
            .parse()
            .unwrap();
        if authorized {
            req.headers_mut().extend(admin.clone());
        }
        let response = router.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), status);
        assert!(
            !json_response(response)
                .await
                .to_string()
                .contains("fixture-secret")
        );
    }
    assert_eq!(snapshot(&pool, destination.id).await, before);
    use crate::models::auth_identity::AuthRole;
    use crate::queries::auth_identity::sync_user_role;
    let user: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username='cache-key-admin'")
            .fetch_one(&pool)
            .await
            .unwrap();
    sync_user_role(&pool, user, AuthRole::Viewer).await.unwrap();
    for id in [destination.id, i32::MAX] {
        let mut req = request("not-json-fixture-secret");
        *req.uri_mut() = format!("/api/v1/caches/{id}/test-credentials")
            .parse()
            .unwrap();
        req.headers_mut().extend(admin.clone());
        let response = router.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(
            !json_response(response)
                .await
                .to_string()
                .contains("fixture-secret")
        );
    }
    sync_user_role(&pool, user, AuthRole::Admin).await.unwrap();
    let mut req = request("{}");
    *req.uri_mut() = format!("/api/v1/caches/{}/test-credentials", destination.id)
        .parse()
        .unwrap();
    req.headers_mut().extend(admin.clone());
    req.headers_mut()
        .insert("x-csrf-token", "unmatched-fixture-csrf".parse().unwrap());
    let response = router.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_response(response).await["error"],
        "csrf_validation_failed"
    );
    assert_eq!(snapshot(&pool, destination.id).await, before);
    let patch: UpdateCacheDestination = serde_json::from_value(serde_json::json!({
        "cache_type":"S3", "push_to":"s3://fixture-bucket",
        "s3_access_key_id":"explicit-access-marker", "s3_secret_access_key":"explicit-secret-marker"
    }))
    .unwrap();
    let loaded = cache_destinations::get_cache_destination(&pool, destination.id)
        .await
        .unwrap()
        .unwrap();
    let effective = cache_destinations::effective_update(&loaded, &patch).unwrap();
    assert!(effective.s3_session_token.is_none());
    assert!(effective.attic_token.is_none());
    let saved = cache_destinations::update_cache_destination(&pool, destination.id, &patch)
        .await
        .unwrap()
        .unwrap();
    assert!(saved.s3_session_token.is_none());
    assert!(saved.attic_token.is_none());
    assert_eq!(saved.s3_access_key_id, effective.s3_access_key_id);
    assert_eq!(saved.s3_secret_access_key, effective.s3_secret_access_key);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn add_probe_validates_without_writes_and_niks3_requires_token(pool: PgPool) {
    let admin = super::tests::admin_headers(&pool).await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM cache_destinations")
        .fetch_one(&pool)
        .await
        .unwrap();
    for ty in ["Attic", "S3", "Niks3"] {
        let mut create = fixture(ty);
        if ty == "Niks3" {
            create.niks3_auth_token = None;
        } else if ty == "S3" {
            create.s3_secret_access_key = None;
        } else {
            create.attic_token = None;
        }
        let response = test_cache_destination_credentials(
            State(pool.clone()),
            State(ServerConfig::default()),
            admin.clone(),
            request(&serde_json::to_string(&create).unwrap()),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(!json_response(response).await.to_string().contains("marker"));
    }
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM cache_destinations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let create = fixture("Niks3");
    create.validate().unwrap();
    assert!(
        Niks3ConnectionTestResult::default()
            .write_auth_valid
            .is_none()
    );
}

#[test]
fn active_type_metadata_and_serialization_never_reveal_inactive_credentials() {
    for ty in ["Nix", "Http", "Attic", "S3"] {
        let destination = redact_cache_secrets(CacheDestination {
            cache_type: ty.into(),
            attic_token: Some("fixture-attic-marker".into()),
            s3_access_key_id: Some("fixture-access-marker".into()),
            s3_secret_access_key: Some("fixture-secret-marker".into()),
            s3_session_token: Some("fixture-session-marker".into()),
            niks3_auth_token: Some("inactive-write-marker".into()),
            niks3_write_client_cert: Some(TEST_CERTIFICATE.into()),
            niks3_write_client_key: Some("inactive-write-key-marker".into()),
            niks3_read_client_cert: Some(TEST_CERTIFICATE.into()),
            niks3_read_client_key: Some("inactive-read-key-marker".into()),
            ..Default::default()
        });
        assert_eq!(destination.attic_token_configured, ty == "Attic");
        assert_eq!(destination.s3_credentials_configured, ty == "S3");
        assert_eq!(destination.s3_session_token_configured, ty == "S3");
        assert!(!destination.niks3_write_token_configured);
        assert!(!destination.niks3_write_mtls_configured);
        assert!(!destination.niks3_read_mtls_configured);
        assert!(
            !serde_json::to_string(&destination)
                .unwrap()
                .contains("marker")
        );
        assert!(!format!("{destination:?}").contains("marker"));
    }
}

#[test]
fn effective_update_never_borrows_inactive_credentials_on_type_conversion() {
    let source = CacheDestination {
        name: "conversion".into(),
        cache_type: "Nix".into(),
        push_to: Some("https://cache.example.invalid".into()),
        attic_cache_name: Some("fixture".into()),
        attic_public_key: Some(nix_public_key_fixture("fixture")),
        attic_token: Some("inactive-attic-marker".into()),
        s3_region: Some("fixture-region".into()),
        s3_endpoint_url: Some("https://objects.example.invalid".into()),
        s3_access_key_id: Some("inactive-access-marker".into()),
        s3_secret_access_key: Some("inactive-secret-marker".into()),
        s3_session_token: Some("inactive-session-marker".into()),
        niks3_server_url: Some("https://write.example.invalid".into()),
        niks3_public_keys: vec![nix_public_key_fixture("fixture")],
        niks3_write_auth_mode: Some("token".into()),
        niks3_auth_token: Some("inactive-write-marker".into()),
        niks3_read_auth_mode: Some("mtls".into()),
        niks3_read_client_cert: Some(TEST_CERTIFICATE.into()),
        niks3_read_client_key: Some("inactive-read-key-marker".into()),
        ..Default::default()
    };
    for ty in ["Attic", "S3", "Niks3"] {
        let patch = UpdateCacheDestination {
            cache_type: Some(ty.into()),
            ..Default::default()
        };
        assert!(cache_destinations::effective_update(&source, &patch).is_err());
    }
    let patch = UpdateCacheDestination {
        cache_type: Some("Niks3".into()),
        niks3_write_auth_mode: Some("token".into()),
        niks3_auth_token: Some("explicit-write-marker".into()),
        niks3_read_auth_mode: Some("none".into()),
        ..Default::default()
    };
    let effective = cache_destinations::effective_update(&source, &patch).unwrap();
    assert!(effective.niks3_read_client_key.is_none());
    assert!(effective.attic_token.is_none());
    assert!(effective.s3_access_key_id.is_none());
    assert_eq!(
        effective.niks3_auth_token.as_deref(),
        Some("explicit-write-marker")
    );
}

#[test]
fn bearer_auth_is_exclusive_to_active_attic() {
    let client = reqwest::Client::new();
    for ty in ["S3", "Nix", "Http", "Niks3", "Attic"] {
        let create = CreateCacheDestination {
            cache_type: ty.into(),
            attic_token: Some("inactive-attic-marker".into()),
            ..Default::default()
        };
        let request = legacy_probe_request(
            &client,
            Url::parse("https://cache.example.invalid").unwrap(),
            &create,
        )
        .build()
        .unwrap();
        assert_eq!(
            request
                .headers()
                .contains_key(reqwest::header::AUTHORIZATION),
            ty == "Attic"
        );
    }
}

fn basic_fixture_url(host: &str) -> (String, String, String) {
    // Build the URI at runtime. No committed credential tuple or provider-shaped
    // access marker is needed to prove Basic header construction.
    let username = ["synthetic", "basic", "user"].join("-");
    let password = format!("synthetic-{}", uuid::Uuid::new_v4());
    let mut url = Url::parse(&format!("https://{host}/cache?priority=30")).unwrap();
    url.set_username(&username).unwrap();
    url.set_password(Some(&password)).unwrap();
    (url.into(), username, password)
}

fn assert_basic_request(effective: &CreateCacheDestination, username: &str, password: &str) {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let request = legacy_probe_request(
        &reqwest::Client::new(),
        Url::parse(effective.push_to.as_deref().unwrap()).unwrap(),
        effective,
    )
    .build()
    .unwrap();
    assert!(request.url().username().is_empty());
    assert!(request.url().password().is_none());
    let auth = &request.headers()[reqwest::header::AUTHORIZATION];
    assert!(auth.is_sensitive());
    assert_eq!(
        auth.to_str().unwrap(),
        format!(
            "Basic {}",
            STANDARD.encode(format!("{username}:{password}"))
        )
    );
}

fn successful_probe() -> CacheCredentialTestResult {
    CacheCredentialTestResult {
        ok: true,
        status_code: Some(200),
        message: "Read-only fixture successful".into(),
        tested_url: None,
        niks3: None,
    }
}

fn native_attic_config(is_public: bool) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "public_key": nix_public_key_fixture("native"),
        "is_public": is_public,
        "store_dir": "/nix/store",
        "priority": 41
    }))
    .unwrap()
}

#[test]
fn attic_canonical_api_and_model_read_roots_share_named_cache() {
    for raw in [
        "https://cache.example.com",
        "https://cache.example.com/",
        "https://cache.example.com/team",
        "https://cache.example.com/team/nix-cache-info",
    ] {
        let urls = cf_config::attic_urls::resolve_attic_urls(raw, "team").unwrap();
        assert_eq!(
            urls.cache_config_url.as_str(),
            "https://cache.example.com/_api/v1/cache-config/team"
        );
        let destination = CacheDestination {
            cache_type: "Attic".into(),
            push_to: Some(raw.into()),
            attic_cache_name: Some("team".into()),
            attic_public_key: Some(nix_public_key_fixture("native")),
            ..Default::default()
        };
        let (read, _, auth) = destination.read_config().unwrap();
        assert_eq!(read, "https://cache.example.com/team");
        assert_eq!(auth, cf_protocol::cache::CacheReadAuth::None);
    }
    for (raw, name, expected_read, expected_api) in [
        (
            "attic://cache.example.com/ignored/prefix",
            "local:team",
            "https://cache.example.com/team",
            "https://cache.example.com/_api/v1/cache-config/team",
        ),
        (
            "https://cache.example.com/proxy/",
            "local:team",
            "https://cache.example.com/proxy/team",
            "https://cache.example.com/proxy/_api/v1/cache-config/team",
        ),
        (
            "https://cache.example.com/proxy/%74eam/nix-cache-info?priority=30",
            "team",
            "https://cache.example.com/proxy/team?priority=30",
            "https://cache.example.com/proxy/_api/v1/cache-config/team?priority=30",
        ),
    ] {
        let urls = cf_config::attic_urls::resolve_attic_urls(raw, name).unwrap();
        assert_eq!(urls.cache_config_url.as_str(), expected_api);
        let destination = CacheDestination {
            cache_type: "Attic".into(),
            push_to: Some(raw.into()),
            attic_cache_name: Some(name.into()),
            ..Default::default()
        };
        assert_eq!(destination.read_config().unwrap().0, expected_read);
    }
    for name in [None, Some(""), Some("../other")] {
        let destination = CacheDestination {
            cache_type: "Attic".into(),
            push_to: Some("https://cache.example.com".into()),
            attic_cache_name: name.map(str::to_string),
            ..Default::default()
        };
        assert!(destination.read_config().is_err());
    }
}

#[test]
fn attic_native_metadata_and_status_matrix_never_claims_root_or_write_success() {
    for is_public in [false, true] {
        let json = serde_json::to_value(attic_probe::classify_response(
            200,
            &native_attic_config(is_public),
        ))
        .unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["probe_kind"], "attic_cache_config");
        assert_eq!(json["stage"], "complete");
        assert_eq!(json["cache_access_valid"], true);
        assert_eq!(
            json["token_auth_valid"],
            if is_public {
                serde_json::Value::Null
            } else {
                true.into()
            }
        );
        assert!(json["write_auth_valid"].is_null());
        assert_eq!(
            json["message"],
            "Cache access verified. Write authorization: Untested."
        );
        assert!(json["tested_url"].is_null());
        assert!(json.get("public_key").is_none());
    }
    for body in [
        b"<html>root is healthy</html>".as_slice(),
        b"{}",
        b"[]",
        b"null",
    ] {
        let json = serde_json::to_value(attic_probe::classify_response(200, body)).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["stage"], "response");
    }
    for (field, value) in [
        ("public_key", serde_json::json!("invalid-key")),
        ("is_public", serde_json::json!("true")),
        ("store_dir", serde_json::json!("/other/store")),
        ("priority", serde_json::json!(2147483648_i64)),
    ] {
        for missing in [false, true] {
            let mut config: serde_json::Value =
                serde_json::from_slice(&native_attic_config(false)).unwrap();
            if missing {
                config.as_object_mut().unwrap().remove(field);
            } else {
                config[field] = value.clone();
            }
            let result = attic_probe::classify_response(200, &serde_json::to_vec(&config).unwrap());
            assert!(!result.ok);
        }
    }
    for (status, body, stage) in [
        (
            401,
            b"sensitive upstream detail".as_slice(),
            "authentication",
        ),
        (403, b"{}", "authentication"),
        (
            404,
            br#"{"code":404,"error":"NoSuchCache","message":"sensitive upstream detail"}"#,
            "cache_not_found",
        ),
        (404, br#"{"error":"NoSuchCache"}"#, "response"),
        (404, br#"{"code":404,"error":"NotFound"}"#, "response"),
        (404, b"<html>not found</html>", "response"),
        (302, b"{}", "response"),
        (500, b"sensitive upstream detail", "response"),
    ] {
        let json = serde_json::to_value(attic_probe::classify_response(status, body)).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["stage"], stage);
        assert!(json["token_auth_valid"].is_null());
        assert!(json["write_auth_valid"].is_null());
        assert!(!json.to_string().contains("sensitive upstream detail"));
    }
}

#[tokio::test]
async fn attic_target_policy_and_credential_queries_fail_before_network() {
    for raw in [
        "http://cache.example.com",
        "https://127.0.0.1",
        "https://[::1]",
        "https://localhost",
        "https://user:synthetic@cache.example.com",
        "https://cache.example.com?token=synthetic",
    ] {
        let mut create = fixture("Attic");
        create.push_to = Some(raw.into());
        let error = run_cache_destination_test(&create, false)
            .await
            .unwrap_err();
        assert!(!error.contains(raw));
        let response = attic_probe::policy_error_response(&error);
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let json = json_response(response).await;
        assert_eq!(json["stage"], "target_policy");
        assert!(json["write_auth_valid"].is_null());
        assert!(json["cache_access_valid"].is_null());
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn attic_named_cache_retained_replacement_results_and_policy_preserve_raw_state(
    pool: PgPool,
) {
    let admin = super::tests::admin_headers(&pool).await;
    let destination = scoped_fixture(
        &pool,
        "Attic",
        "native-api-scope",
        "https://cache.example.com",
    )
    .await;
    let before = snapshot(&pool, destination.id).await;
    for replacement in [false, true] {
        let patch = if replacement {
            r#"{"attic_token":"replacement-attic-marker"}"#
        } else {
            "{}"
        };
        let probe_pool = &pool;
        let id = destination.id;
        let response = test_stored_with_probe(
            &pool,
            &admin,
            destination.id,
            request(patch),
            |effective| async move {
                let urls = cf_config::attic_urls::resolve_attic_urls(
                    effective.push_to.as_deref().unwrap(),
                    effective.attic_cache_name.as_deref().unwrap(),
                )
                .unwrap();
                assert_eq!(
                    urls.cache_config_url.as_str(),
                    "https://cache.example.com/_api/v1/cache-config/fixture-cache"
                );
                let request = legacy_probe_request(
                    &reqwest::Client::new(),
                    urls.cache_config_url,
                    &effective,
                )
                .build()
                .unwrap();
                let expected = if replacement {
                    "replacement-attic-marker"
                } else {
                    "fixture-attic-marker"
                };
                let header = &request.headers()[reqwest::header::AUTHORIZATION];
                assert!(header.is_sensitive());
                assert!(header.to_str().unwrap() == format!("Bearer {expected}"));
                // The probe holds no long database lock and performs no writes.
                let mut tx = probe_pool.begin().await.unwrap();
                sqlx::query("SELECT id FROM cache_destinations WHERE id=$1 FOR UPDATE NOWAIT")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap();
                tx.rollback().await.unwrap();
                Ok(attic_probe::classify_response(
                    200,
                    &native_attic_config(false),
                ))
            },
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let json = json_response(response).await;
        assert_eq!(json["stage"], "complete");
        assert_eq!(json["token_auth_valid"], true);
        assert!(!json.to_string().contains("marker"));
        assert!(snapshot(&pool, destination.id).await == before);
    }
    for (status, body) in [
        (401, b"private upstream detail".as_slice()),
        (404, br#"{"code":404,"error":"NoSuchCache"}"#),
        (200, b"{}"),
        (302, b"private upstream detail"),
    ] {
        let response =
            test_stored_with_probe(&pool, &admin, destination.id, request("{}"), |effective| {
                assert!(effective.attic_token.as_deref() == Some("fixture-attic-marker"));
                std::future::ready(Ok(attic_probe::classify_response(status, body)))
            })
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let json = json_response(response).await;
        assert_eq!(json["ok"], false);
        assert!(json["write_auth_valid"].is_null());
        assert!(!json.to_string().contains("private upstream detail"));
        assert!(snapshot(&pool, destination.id).await == before);
    }
    for raw in [
        "https://127.0.0.1",
        "https://cache.example.com?token=synthetic",
    ] {
        let patch = serde_json::json!({"push_to":raw});
        let response = test_stored_cache_destination_credentials(
            State(pool.clone()),
            State(ServerConfig::default()),
            admin.clone(),
            Path(destination.id),
            request(&patch.to_string()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let json = json_response(response).await;
        assert_eq!(json["stage"], "target_policy");
        assert!(json["write_auth_valid"].is_null());
        assert!(!json.to_string().contains("synthetic"));
        assert!(snapshot(&pool, destination.id).await == before);
    }
}

fn historical_attic_envelope(token: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use rand::RngCore;
    use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
    use sha2::{Digest, Sha256};

    // COMPATIBILITY: Construct the historical wire format independently of
    // encrypt_secret and the new create path. Use only the runtime fixture key;
    // never print the key, plaintext, or envelope, even on assertion failure.
    let raw_key = std::env::var("CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY").unwrap();
    let key_bytes = Sha256::digest(raw_key.as_bytes());
    let key = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &key_bytes).unwrap());
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let mut ciphertext = token.as_bytes().to_vec();
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::empty(),
        &mut ciphertext,
    )
    .unwrap();
    assert_eq!(ciphertext.len(), token.len() + 16);
    format!(
        "enc:v1:{}.{}",
        STANDARD.encode(nonce),
        STANDARD.encode(ciphertext)
    )
}

async fn insert_legacy_attic(pool: &PgPool, name: &str, token: Option<&str>) -> i32 {
    // Insert only historical columns. New create validation/encryption and
    // response-only configured flags cannot manufacture this fixture's state.
    let id = sqlx::query_scalar("INSERT INTO cache_destinations (name, cache_type, push_to, attic_token, attic_cache_name, attic_public_key) VALUES ($1, 'Attic', 'https://legacy.example.invalid/cache', $2, 'legacy-cache', $3) RETURNING id")
        .bind(name).bind(token).bind(nix_public_key_fixture("legacy"))
        .fetch_one(pool).await.unwrap();
    let scope: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO cache_destination_environments (cache_destination_id, environment_id) VALUES ($1, $2)")
        .bind(id).bind(scope).execute(pool).await.unwrap();
    id
}

async fn assert_legacy_attic_metadata(pool: &PgPool, admin: &HeaderMap, id: i32, configured: bool) {
    let response = get_cache_destination(State(pool.clone()), admin.clone(), Path(id))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let single = json_response(response).await;
    let response = list_cache_destinations(
        State(pool.clone()),
        admin.clone(),
        Query(ListCacheDestinationsQuery {
            enabled_only: false,
        }),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let list = json_response(response).await;
    let listed = list
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap();
    assert!(single == *listed, "single/list metadata differ");
    assert_eq!(single["attic_token_configured"], configured);
    assert!(
        single
            .get("attic_token")
            .is_none_or(serde_json::Value::is_null)
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn legacy_attic_plaintext_and_historical_ciphertext_retain_on_test_and_save(pool: PgPool) {
    let admin = super::tests::admin_headers(&pool).await;
    for encrypted in [false, true] {
        // JWT-shaped synthetic values exercise retention, not provider auth.
        // Native Bearer acceptance is proved by the owner's VM runtime JWTs.
        let token = format!("e30.e30.synthetic-{}", uuid::Uuid::new_v4());
        let stored = if encrypted {
            historical_attic_envelope(&token)
        } else {
            token.clone()
        };
        let id =
            insert_legacy_attic(&pool, &format!("legacy-attic-{encrypted}"), Some(&stored)).await;
        let before = snapshot(&pool, id).await;
        assert_legacy_attic_metadata(&pool, &admin, id, true).await;
        assert!(
            snapshot(&pool, id).await == before,
            "GET/list mutated raw state"
        );
        let response = test_stored_with_probe(&pool, &admin, id, request("{}"), |effective| {
            assert_eq!(effective.cache_type, "Attic");
            assert!(
                effective.attic_token.as_deref() == Some(token.as_str()),
                "retained token differs"
            );
            assert_eq!(
                effective.push_to.as_deref(),
                Some("https://legacy.example.invalid/cache")
            );
            std::future::ready(Ok(successful_probe()))
        })
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = json_response(response).await;
        assert_eq!(result["ok"], true);
        assert!(!result.to_string().contains(&token));
        assert!(!result.to_string().contains(&stored));
        assert!(
            snapshot(&pool, id).await == before,
            "Test mutated raw state"
        );
        // Cancel has no server route: discard an in-memory draft without PUT.
        let draft = UpdateCacheDestination {
            name: Some("discarded draft".into()),
            ..Default::default()
        };
        drop(draft);
        assert!(
            snapshot(&pool, id).await == before,
            "Cancel mutated raw state"
        );

        let response = update_cache_destination(
            State(pool.clone()),
            admin.clone(),
            Path(id),
            Json(UpdateCacheDestination {
                cache_type: Some("Attic".into()),
                attic_token: Some(String::new()),
                ..Default::default()
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let error = json_response(response).await;
        assert_eq!(error["error"], "validation_error");
        assert_eq!(error["message"], "Invalid effective cache configuration");
        assert!(error["details"].is_null());
        assert!(!error.to_string().contains(&token));
        assert!(!error.to_string().contains(&stored));
        assert!(
            snapshot(&pool, id).await == before,
            "blank Save mutated raw state"
        );

        let scope = before["scope"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| serde_json::from_value(row["environment_id"].clone()).unwrap())
            .collect();
        let name = format!("legacy-attic-{encrypted}-renamed");
        let response = update_cache_destination(
            State(pool.clone()),
            admin.clone(),
            Path(id),
            Json(UpdateCacheDestination {
                name: Some(name.clone()),
                cache_type: Some("Attic".into()),
                environment_ids: Some(scope),
                ..Default::default()
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let saved = json_response(response).await;
        assert_eq!(saved["attic_token_configured"], true);
        assert!(
            saved
                .get("attic_token")
                .is_none_or(serde_json::Value::is_null)
        );
        let after = snapshot(&pool, id).await;
        assert_eq!(after["destination"]["name"], name);
        let mut normalized = after.clone();
        for field in ["name", "updated_at"] {
            normalized["destination"][field] = before["destination"][field].clone();
        }
        // Successful assignment replacement may renew only created_at. Compare
        // all other members, including exact destination/environment identity.
        let old_scope = before["scope"].as_array().unwrap();
        let new_scope = normalized["scope"].as_array_mut().unwrap();
        assert_eq!(new_scope.len(), old_scope.len());
        for (new, old) in new_scope.iter_mut().zip(old_scope) {
            new["created_at"] = old["created_at"].clone();
        }
        assert!(
            normalized == before,
            "unrelated Save changed protected raw state"
        );
        let response = test_stored_with_probe(&pool, &admin, id, request("{}"), |effective| {
            assert!(effective.attic_token.as_deref() == Some(token.as_str()));
            std::future::ready(Ok(successful_probe()))
        })
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            snapshot(&pool, id).await == after,
            "post-Save Test mutated raw state"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn legacy_attic_null_and_empty_token_refuse_before_probe_without_mutation(pool: PgPool) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let admin = super::tests::admin_headers(&pool).await;
    for (name, token) in [("legacy-null", None), ("legacy-empty", Some(""))] {
        let id = insert_legacy_attic(&pool, name, token).await;
        let before = snapshot(&pool, id).await;
        assert_legacy_attic_metadata(&pool, &admin, id, false).await;
        let called = AtomicBool::new(false);
        let response = test_stored_with_probe(&pool, &admin, id, request("{}"), |_| {
            called.store(true, Ordering::SeqCst);
            std::future::ready(Ok(successful_probe()))
        })
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(!called.load(Ordering::SeqCst));
        let error = json_response(response).await;
        assert_eq!(error["error"], "invalid_cache_test_config");
        assert_eq!(error["message"], "Invalid effective cache configuration");
        assert!(error["details"].is_null());
        assert!(error.get("attic_token").is_none());
        assert!(
            snapshot(&pool, id).await == before,
            "missing-token route mutated raw state"
        );
    }
}

async fn scoped_fixture(pool: &PgPool, ty: &str, name: &str, raw: &str) -> CacheDestination {
    let scope: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap();
    let mut create = fixture(ty);
    create.name = name.into();
    create.push_to = Some(raw.into());
    create.environment_ids = Some(vec![scope]);
    cache_destinations::create_cache_destination(pool, &create)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn legacy_basic_probe_preserves_raw_url_and_sanitized_roundtrip(pool: PgPool) {
    let admin = super::tests::admin_headers(&pool).await;
    for ty in ["Nix", "Http"] {
        let (raw, username, password) = basic_fixture_url("basic.example.invalid");
        let destination = scoped_fixture(&pool, ty, &format!("basic-{ty}"), &raw).await;
        let id = destination.id;
        let before = snapshot(&pool, id).await;
        let dto = json_response(
            get_cache_destination(State(pool.clone()), admin.clone(), Path(id))
                .await
                .into_response(),
        )
        .await;
        assert_eq!(dto["http_basic_auth_configured"], true);
        assert_eq!(dto["legacy_query_credentials_configured"], false);
        assert!(!dto.to_string().contains(&username));
        assert!(!dto.to_string().contains(&password));
        let sanitized = dto["push_to"].as_str().unwrap();
        for patch in [
            serde_json::json!({}),
            serde_json::json!({"push_to": sanitized}),
        ] {
            let response = test_stored_with_probe(
                &pool,
                &admin,
                id,
                request(&patch.to_string()),
                |effective| {
                    assert_eq!(effective.push_to.as_deref(), Some(raw.as_str()));
                    assert_basic_request(&effective, &username, &password);
                    std::future::ready(Ok(successful_probe()))
                },
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let result = json_response(response).await;
            assert!(result["tested_url"].is_null());
            assert!(!result.to_string().contains(&password));
            assert_eq!(snapshot(&pool, id).await, before);
        }
        let update: UpdateCacheDestination = serde_json::from_value(serde_json::json!({
            "name": format!("basic-{ty}-renamed"), "push_to": sanitized
        }))
        .unwrap();
        let saved = cache_destinations::update_cache_destination(&pool, id, &update)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.push_to.as_deref(), Some(raw.as_str()));
        let after_save = snapshot(&pool, id).await;
        assert_eq!(
            after_save["destination"]["push_to"],
            before["destination"]["push_to"]
        );
        assert_eq!(after_save["scope"], before["scope"]);
        let response = test_stored_with_probe(&pool, &admin, id, request("{}"), |effective| {
            assert_basic_request(&effective, &username, &password);
            std::future::ready(Ok(successful_probe()))
        })
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(snapshot(&pool, id).await, after_save);
        let response = test_stored_with_probe(
            &pool,
            &admin,
            id,
            request(r#"{"push_to":"https://other.example.invalid/cache?priority=30"}"#),
            |effective| {
                let url = Url::parse(effective.push_to.as_deref().unwrap()).unwrap();
                assert!(url.username().is_empty());
                assert!(url.password().is_none());
                let outgoing = legacy_probe_request(&reqwest::Client::new(), url, &effective)
                    .build()
                    .unwrap();
                assert!(
                    !outgoing
                        .headers()
                        .contains_key(reqwest::header::AUTHORIZATION)
                );
                std::future::ready(Ok(successful_probe()))
            },
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(snapshot(&pool, id).await, after_save);
    }
}

async fn assert_query_refusal(pool: &PgPool, admin: &HeaderMap, id: i32, patch: serde_json::Value) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let called = AtomicBool::new(false);
    let before = snapshot(pool, id).await;
    let response = test_stored_with_probe(pool, admin, id, request(&patch.to_string()), |_| {
        called.store(true, Ordering::SeqCst);
        std::future::ready(Ok(successful_probe()))
    })
    .await;
    assert!(!called.load(Ordering::SeqCst));
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error = json_response(response).await;
    assert_eq!(error["error"], "legacy_query_credentials_unsupported");
    assert_eq!(error["message"], LEGACY_QUERY_CREDENTIALS_TEST_ERROR);
    assert_eq!(snapshot(pool, id).await, before);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn legacy_query_probe_refuses_without_mutation_or_replay(pool: PgPool) {
    let admin = super::tests::admin_headers(&pool).await;
    for ty in ["Http", "Nix", "Attic"] {
        let marker = format!("synthetic-query-{}", uuid::Uuid::new_v4());
        let mut url = Url::parse("https://query.example.invalid/cache?priority=30").unwrap();
        url.query_pairs_mut().append_pair("ToKeN", &marker);
        let raw = url.to_string();
        let destination = scoped_fixture(&pool, ty, &format!("query-{ty}"), &raw).await;
        let id = destination.id;
        let dto = json_response(
            get_cache_destination(State(pool.clone()), admin.clone(), Path(id))
                .await
                .into_response(),
        )
        .await;
        assert_eq!(dto["legacy_query_credentials_configured"], true);
        assert!(!dto.to_string().contains(&marker));
        let sanitized = dto["push_to"].as_str().unwrap();
        assert_query_refusal(&pool, &admin, id, serde_json::json!({})).await;
        assert_query_refusal(&pool, &admin, id, serde_json::json!({"push_to": sanitized})).await;
        let saved = cache_destinations::update_cache_destination(
            &pool,
            id,
            &serde_json::from_value(
                serde_json::json!({"name": format!("query-{ty}-renamed"), "push_to": sanitized}),
            )
            .unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(saved.push_to.as_deref(), Some(raw.as_str()));
        assert_query_refusal(&pool, &admin, id, serde_json::json!({})).await;
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn legacy_s3_presigned_endpoint_redacts_and_refuses_replay(pool: PgPool) {
    let admin = super::tests::admin_headers(&pool).await;
    let marker = format!("synthetic-query-{}", uuid::Uuid::new_v4());
    let raw = format!(
        "https://objects.example.invalid/?priority=30&%58-Amz-Credential={marker}&X-Amz-Security-Token={marker}&X-Amz-Signature={marker}&AWSAccessKeyId={marker}&Signature={marker}"
    );
    let mut create = fixture("S3");
    create.s3_endpoint_url = Some(raw.clone());
    let scope: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO environments (name) VALUES ('query-s3-scope') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    create.environment_ids = Some(vec![scope]);
    let destination = cache_destinations::create_cache_destination(&pool, &create)
        .await
        .unwrap();
    let id = destination.id;
    let before = snapshot(&pool, id).await;
    let dto = json_response(
        get_cache_destination(State(pool.clone()), admin.clone(), Path(id))
            .await
            .into_response(),
    )
    .await;
    assert_eq!(dto["legacy_query_credentials_configured"], true);
    assert_eq!(dto["s3_credentials_configured"], true);
    assert_eq!(
        dto["s3_endpoint_url"],
        "https://objects.example.invalid/?priority=30"
    );
    assert!(!dto.to_string().contains(&marker));
    assert_query_refusal(&pool, &admin, id, serde_json::json!({})).await;
    assert_query_refusal(
        &pool,
        &admin,
        id,
        serde_json::json!({"s3_endpoint_url": dto["s3_endpoint_url"]}),
    )
    .await;
    assert_eq!(snapshot(&pool, id).await, before);
    let response = test_cache_destination_credentials(
        State(pool.clone()),
        State(ServerConfig::default()),
        admin.clone(),
        request(&serde_json::to_string(&create).unwrap()),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_response(response).await["message"],
        LEGACY_QUERY_CREDENTIALS_TEST_ERROR
    );
    let saved = cache_destinations::update_cache_destination(&pool, id,
        &serde_json::from_value(serde_json::json!({"name": "query-s3-renamed", "s3_endpoint_url": dto["s3_endpoint_url"]})).unwrap()
    ).await.unwrap().unwrap();
    assert_eq!(saved.s3_endpoint_url.as_deref(), Some(raw.as_str()));
    let after_save = snapshot(&pool, id).await;
    for field in [
        "s3_access_key_id",
        "s3_secret_access_key",
        "s3_session_token",
    ] {
        assert_eq!(
            before["destination"][field],
            after_save["destination"][field]
        );
    }
    assert_query_refusal(&pool, &admin, id, serde_json::json!({})).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn legacy_uri_type_conversion_strips_inherited_auth(pool: PgPool) {
    let admin = super::tests::admin_headers(&pool).await;
    for (source, target) in [("Nix", "Http"), ("Http", "Nix")] {
        let (raw, _, password) = basic_fixture_url("conversion.example.invalid");
        let mut url = Url::parse(&raw).unwrap();
        url.query_pairs_mut()
            .append_pair("X-Amz-Signature", &password);
        let raw = url.to_string();
        let destination = scoped_fixture(&pool, source, &format!("uri-{source}"), &raw).await;
        let id = destination.id;
        let patch = serde_json::json!({"cache_type": target});
        let before = snapshot(&pool, id).await;
        let response = test_stored_with_probe(
            &pool,
            &admin,
            id,
            request(&patch.to_string()),
            |effective| {
                assert_eq!(effective.cache_type, target);
                let url = Url::parse(effective.push_to.as_deref().unwrap()).unwrap();
                assert!(url.username().is_empty());
                assert!(url.password().is_none());
                assert!(!cache_url_has_query_credentials(url.as_str()));
                assert_eq!(url.query(), Some("priority=30"));
                assert!(!effective.push_to.as_deref().unwrap().contains(&password));
                std::future::ready(Ok(successful_probe()))
            },
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(snapshot(&pool, id).await, before);
        let saved = cache_destinations::update_cache_destination(
            &pool,
            id,
            &serde_json::from_value(patch).unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            saved.push_to.as_deref(),
            Some(sanitize_cache_url_credentials(&raw).as_str())
        );
        assert!(!saved.http_basic_auth_configured);
        assert!(!saved.legacy_query_credentials_configured);
        let after_save = snapshot(&pool, id).await;
        assert_eq!(after_save["scope"], before["scope"]);
        let response = test_stored_with_probe(&pool, &admin, id, request("{}"), |effective| {
            assert!(!cache_url_has_query_credentials(
                effective.push_to.as_deref().unwrap()
            ));
            assert!(
                Url::parse(effective.push_to.as_deref().unwrap())
                    .unwrap()
                    .password()
                    .is_none()
            );
            std::future::ready(Ok(successful_probe()))
        })
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(snapshot(&pool, id).await, after_save);
    }
}

#[test]
fn every_url_field_redacts_decoded_aws_queries_and_invalid_urls_fail_closed() {
    for name in [
        "X-Amz-Credential",
        "x-AMZ-security-token",
        "X-Amz-Signature",
        "AWSAccessKeyId",
        "Signature",
        "%58%2dAmz%2dCredential",
        "%53ignature",
        "%74oken",
    ] {
        let marker = format!("synthetic-query-{}", uuid::Uuid::new_v4());
        let raw =
            format!("https://cache.example.invalid/cache?priority=30&{name}={marker}&priority=40");
        for ty in ["Nix", "Http", "Attic", "S3", "Niks3"] {
            let dto = redact_cache_secrets(CacheDestination {
                cache_type: ty.into(),
                push_to: Some(raw.clone()),
                s3_endpoint_url: Some(raw.clone()),
                niks3_server_url: Some(raw.clone()),
                ..Default::default()
            });
            assert!(dto.legacy_query_credentials_configured);
            assert!(!serde_json::to_string(&dto).unwrap().contains(&marker));
            for value in [dto.push_to, dto.s3_endpoint_url, dto.niks3_server_url] {
                assert_eq!(
                    Url::parse(value.as_deref().unwrap())
                        .unwrap()
                        .query_pairs()
                        .into_owned()
                        .collect::<Vec<_>>(),
                    vec![
                        ("priority".into(), "30".into()),
                        ("priority".into(), "40".into())
                    ]
                );
            }
        }
        let invalid = format!("not-a-url?{name}={marker}");
        assert_eq!(sanitize_cache_url_credentials(&invalid), "[REDACTED]");
        assert!(cache_url_has_query_credentials(&invalid));
        assert_eq!(
            reject_probe_query_credentials([Some(invalid.as_str())]),
            Err(LEGACY_QUERY_CREDENTIALS_TEST_ERROR)
        );
    }
}
