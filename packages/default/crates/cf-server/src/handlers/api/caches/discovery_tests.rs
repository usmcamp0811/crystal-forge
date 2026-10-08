//! Proves discovery transport selection and unlocked, non-mutating stored merges.

use super::*;
use crate::models::cache_destination::nix_public_key_fixture;
use crate::security::cache_secrets::{TEST_CERTIFICATE, is_encrypted};
use axum::body::Body;
use tower::ServiceExt;

fn request(body: &str) -> axum::extract::Request {
    axum::http::Request::builder()
        .method("POST")
        .uri("/api/v1/caches/1/niks3/discover")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn identity() -> (String, String) {
    // Runtime-only synthetic private keys are never committed, printed, or sent
    // to a provider. Existing OpenSSL tooling generates an actual matching pair.
    let directory = tempfile::tempdir().unwrap();
    let cert = directory.path().join("client.pem");
    let key = directory.path().join("client.key");
    let status = std::process::Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "ed25519",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=discovery-runtime-fixture",
        ])
        .arg("-keyout")
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "runtime certificate generation failed");
    (
        std::fs::read_to_string(cert).unwrap(),
        std::fs::read_to_string(key).unwrap(),
    )
}

fn public_discovery() -> Niks3Discovery {
    Niks3Discovery {
        server_url: "https://write.example.com/".into(),
        substituter_url: "https://read.example.com/".into(),
        public_keys: vec![nix_public_key_fixture("fixture")],
        oidc_audience: None,
    }
}

#[test]
fn niks3_discovery_projects_only_write_mtls_and_public_mode() {
    let public: Niks3DiscoverRequest =
        serde_json::from_str(r#"{"server_url":"https://write.example.com/proxy"}"#).unwrap();
    let transport = niks3_write_transport(
        public.niks3_write_auth_mode.as_deref(),
        public.niks3_write_client_cert.as_deref(),
        public.niks3_write_client_key.as_deref(),
        public.niks3_write_ca_cert.as_deref(),
    )
    .unwrap();
    assert!(transport.cert.is_none() && transport.key.is_none() && transport.ca.is_none());
    let server = niks3_base_url(&public.server_url, false).unwrap();
    let outgoing = niks3_metadata_request(&reqwest::Client::new(), &server)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        outgoing.url().as_str(),
        "https://write.example.com/proxy/api/cache-config"
    );
    assert!(
        !outgoing
            .headers()
            .contains_key(reqwest::header::AUTHORIZATION)
    );
    assert_eq!(outgoing.method(), reqwest::Method::GET);
    let (cert, key) = identity();
    let chain = format!("{cert}\n{TEST_CERTIFICATE}");
    let ca = format!("{TEST_CERTIFICATE}\n{TEST_CERTIFICATE}");
    let transport =
        niks3_write_transport(Some("mtls"), Some(&chain), Some(&key), Some(&ca)).unwrap();
    assert!(transport.cert == Some(chain.as_str()));
    assert!(transport.key == Some(key.as_str()));
    assert!(transport.ca == Some(ca.as_str()));
    assert!(niks3_write_transport(Some("mtls"), Some(&cert), Some(&key), None).is_ok());
    // Discovery is public metadata, even behind transport-level mTLS. It cannot
    // establish authorization for the upload API or token validity.
    assert!(
        Niks3ConnectionTestResult::default()
            .write_auth_valid
            .is_none()
    );
}

#[tokio::test]
async fn niks3_discovery_rejects_invalid_bundles_and_queries_before_network() {
    let (cert, key) = identity();
    for bad in [
        "".to_owned(),
        "arbitrary text".into(),
        format!("{cert}\n# comment\n{TEST_CERTIFICATE}"),
        format!("{cert}\n{key}"),
        "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----".into(),
    ] {
        for (client_cert, ca) in [
            (Some(bad.as_str()), None),
            (Some(cert.as_str()), Some(bad.as_str())),
        ] {
            let request = Niks3DiscoverRequest {
                server_url: "https://must-not-resolve.invalid".into(),
                niks3_write_auth_mode: Some("mtls".into()),
                niks3_write_client_cert: client_cert.map(str::to_string),
                niks3_write_client_key: Some(key.clone()),
                niks3_write_ca_cert: ca.map(str::to_string),
            };
            let error = discover_niks3(&request, false).await.unwrap_err();
            assert!(error.contains("certificate-only PEM"));
            assert!(!error.contains(&bad) || bad.is_empty());
            assert!(!error.contains("BEGIN"));
        }
    }
    for (mode, client_cert, client_key, ca) in [
        (Some("mtls"), Some(cert.as_str()), None, None),
        (Some("mtls"), None, Some(key.as_str()), None),
        (Some("token"), Some(cert.as_str()), Some(key.as_str()), None),
        (Some("token"), None, None, Some(TEST_CERTIFICATE)),
        (Some("unsupported"), None, None, None),
    ] {
        assert!(niks3_write_transport(mode, client_cert, client_key, ca).is_err());
    }
    assert!(
        niks3_write_transport(
            Some("mtls"),
            Some(&cert),
            Some("malformed-private-key"),
            None
        )
        .is_err()
    );
    for raw in [
        "https://cache.example.com?token=synthetic",
        "https://cache.example.com?X-Amz-Signature=synthetic",
        "https://user:synthetic@cache.example.com",
        "https://127.0.0.1",
        "http://cache.example.com",
    ] {
        let request = Niks3DiscoverRequest {
            server_url: raw.into(),
            niks3_write_auth_mode: None,
            niks3_write_client_cert: None,
            niks3_write_client_key: None,
            niks3_write_ca_cert: None,
        };
        let error = discover_niks3(&request, false).await.unwrap_err();
        assert!(!error.contains("synthetic"));
        assert!(!error.contains(raw));
    }
}

#[tokio::test]
async fn niks3_discovery_admin_first_and_public_response_redact_tls_material() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://127.0.0.1:5433/unused_discovery_fixture")
        .unwrap();
    for stored in [false, true] {
        let response = if stored {
            discover_stored_niks3_cache(
                State(pool.clone()),
                State(ServerConfig::default()),
                HeaderMap::new(),
                Path(i32::MAX),
                request("malformed-secret-json"),
            )
            .await
        } else {
            discover_niks3_cache(
                State(pool.clone()),
                State(ServerConfig::default()),
                HeaderMap::new(),
                request("malformed-secret-json"),
            )
            .await
        };
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(
            !json(response)
                .await
                .to_string()
                .contains("malformed-secret-json")
        );
    }
    let response = serde_json::to_value(public_discovery()).unwrap();
    assert_eq!(response.as_object().unwrap().len(), 4);
    for field in [
        "niks3_write_client_cert",
        "niks3_write_client_key",
        "niks3_write_ca_cert",
        "niks3_auth_token",
    ] {
        assert!(response.get(field).is_none());
    }
    let mut stages = Niks3ConnectionTestResult::default();
    let error = validate_niks3_discovery(br#"{"substituter_url":"https://read.example.com?X-Amz-Signature=synthetic","public_keys":["fixture:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="]}"#, false, &mut stages).await.unwrap_err();
    assert!(!error.contains("synthetic"));
    assert!(stages.write_auth_valid.is_none());
}

async fn snapshot(pool: &PgPool, id: i32) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_build_object('destination',to_jsonb(cd),'scope',(SELECT COALESCE(jsonb_agg(to_jsonb(cde) ORDER BY environment_id),'[]'::jsonb) FROM cache_destination_environments cde WHERE cde.cache_destination_id=cd.id),'jobs',(SELECT count(*) FROM cache_push_jobs)) FROM cache_destinations cd WHERE id=$1")
        .bind(id).fetch_one(pool).await.unwrap()
}

async fn fixture(pool: &PgPool) -> CacheDestination {
    let (cert, key) = identity();
    let (read_cert, read_key) = identity();
    let scope: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES ('discovery-scope') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let create = CreateCacheDestination {
        name: "stored-discovery".into(),
        cache_type: "Niks3".into(),
        push_to: Some("https://read.example.com".into()),
        niks3_server_url: Some("https://write.example.com".into()),
        niks3_public_keys: vec![nix_public_key_fixture("fixture")],
        niks3_write_auth_mode: Some("mtls".into()),
        niks3_write_client_cert: Some(cert),
        niks3_write_client_key: Some(key),
        niks3_write_ca_cert: Some(format!("{TEST_CERTIFICATE}\n{TEST_CERTIFICATE}")),
        niks3_read_auth_mode: Some("mtls".into()),
        niks3_read_client_cert: Some(read_cert),
        niks3_read_client_key: Some(read_key),
        environment_ids: Some(vec![scope]),
        ..Default::default()
    };
    cache_destinations::create_cache_destination(pool, &create)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn niks3_stored_discovery_retains_replaces_clears_modes_without_persistence(pool: PgPool) {
    let admin = super::tests::admin_headers(&pool).await;
    let current = fixture(&pool).await;
    let before = snapshot(&pool, current.id).await;
    assert!(is_encrypted(
        before["destination"]["niks3_write_client_key"]
            .as_str()
            .unwrap()
    ));
    let (replacement_cert, replacement_key) = identity();
    for patch in [
        serde_json::json!({}),
        serde_json::json!({"niks3_write_mtls_configured":false,"niks3_write_token_configured":false}),
        serde_json::json!({"niks3_write_client_cert":replacement_cert,"niks3_write_client_key":replacement_key,"environment_ids":[]}),
        serde_json::json!({"clear_niks3_write_ca_cert":true}),
        serde_json::json!({"niks3_read_auth_mode":"none"}),
        serde_json::json!({"niks3_write_auth_mode":"token","niks3_auth_token":"synthetic-replacement-token"}),
    ] {
        let update: UpdateCacheDestination = serde_json::from_value(patch.clone()).unwrap();
        let expected = cache_destinations::effective_update(&current, &update).unwrap();
        let probe_pool = &pool;
        let response = stored_niks3_discovery_with(
            &pool,
            &admin,
            current.id,
            request(&patch.to_string()),
            |request| async move {
                assert!(request.niks3_write_auth_mode == expected.niks3_write_auth_mode);
                assert!(request.niks3_write_client_cert == expected.niks3_write_client_cert);
                assert!(request.niks3_write_client_key == expected.niks3_write_client_key);
                assert!(request.niks3_write_ca_cert == expected.niks3_write_ca_cert);
                assert!(request.niks3_write_client_key != expected.niks3_read_client_key);
                let mut tx = probe_pool.begin().await.unwrap();
                sqlx::query("SELECT id FROM cache_destinations WHERE id=$1 FOR UPDATE NOWAIT")
                    .bind(current.id)
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap();
                tx.rollback().await.unwrap();
                Ok(public_discovery())
            },
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = json(response).await;
        assert_eq!(result.as_object().unwrap().len(), 4);
        assert!(!result.to_string().contains("BEGIN"));
        assert!(!result.to_string().contains("synthetic"));
        assert!(
            snapshot(&pool, current.id).await == before,
            "discovery changed raw database state"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
async fn niks3_stored_discovery_auth_invalid_json_type_tls_and_csrf_fail_without_mutation(
    pool: PgPool,
) {
    let admin = super::tests::admin_headers(&pool).await;
    let current = fixture(&pool).await;
    let before = snapshot(&pool, current.id).await;
    let router = axum::Router::new().route(
        "/api/v1/caches/:id/niks3/discover",
        axum::routing::post({
            let pool = pool.clone();
            move |headers: HeaderMap, Path(id): Path<i32>, request: axum::extract::Request| {
                discover_stored_niks3_cache(
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
            i32::MAX,
            "malformed-secret-json",
            StatusCode::FORBIDDEN,
        ),
        (true, i32::MAX, "{}", StatusCode::NOT_FOUND),
        (
            true,
            current.id,
            "malformed-secret-json",
            StatusCode::BAD_REQUEST,
        ),
        (
            true,
            current.id,
            r#"{"niks3_write_client_key":123}"#,
            StatusCode::BAD_REQUEST,
        ),
        (
            true,
            current.id,
            r#"{"cache_type":"Attic"}"#,
            StatusCode::BAD_REQUEST,
        ),
        (
            true,
            current.id,
            r#"{"clear_niks3_write_client_key":true}"#,
            StatusCode::BAD_REQUEST,
        ),
        (
            true,
            current.id,
            r#"{"niks3_write_client_cert":"arbitrary text"}"#,
            StatusCode::BAD_REQUEST,
        ),
        (
            true,
            current.id,
            r#"{"niks3_write_client_key":"malformed-private-key"}"#,
            StatusCode::BAD_REQUEST,
        ),
        (
            true,
            current.id,
            r#"{"niks3_server_url":"https://write.example.com?token=synthetic"}"#,
            StatusCode::BAD_REQUEST,
        ),
        (
            true,
            current.id,
            r#"{"niks3_server_url":"https://127.0.0.1"}"#,
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let mut req = request(body);
        *req.uri_mut() = format!("/api/v1/caches/{id}/niks3/discover")
            .parse()
            .unwrap();
        if authorized {
            req.headers_mut().extend(admin.clone());
        }
        let response = router.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), status);
        let error = json(response).await;
        assert!(error["details"].is_null());
        assert!(!error.to_string().contains("synthetic"));
        assert!(!error.to_string().contains("malformed-secret-json"));
        assert!(!error.to_string().contains("malformed-private-key"));
        assert!(snapshot(&pool, current.id).await == before);
    }
    let mut req = request("{}");
    *req.uri_mut() = format!("/api/v1/caches/{}/niks3/discover", current.id)
        .parse()
        .unwrap();
    req.headers_mut().extend(admin.clone());
    req.headers_mut()
        .insert("x-csrf-token", "unmatched".parse().unwrap());
    let response = router.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(json(response).await["error"], "csrf_validation_failed");
    assert!(snapshot(&pool, current.id).await == before);
    // A valid non-Niks3 stored row cannot be converted into a discovery target.
    let id: i32 = sqlx::query_scalar("INSERT INTO cache_destinations(name,cache_type,push_to) VALUES ('not-niks3','Nix','https://cache.example.com') RETURNING id").fetch_one(&pool).await.unwrap();
    let non_niks3_before = snapshot(&pool, id).await;
    let response = discover_stored_niks3_cache(
        State(pool.clone()),
        State(ServerConfig::default()),
        admin.clone(),
        Path(id),
        request("{}"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(snapshot(&pool, id).await == non_niks3_before);
    let user: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username='cache-key-admin'")
            .fetch_one(&pool)
            .await
            .unwrap();
    crate::queries::auth_identity::sync_user_role(
        &pool,
        user,
        crate::models::auth_identity::AuthRole::Viewer,
    )
    .await
    .unwrap();
    for id in [current.id, i32::MAX] {
        let response = discover_stored_niks3_cache(
            State(pool.clone()),
            State(ServerConfig::default()),
            admin.clone(),
            Path(id),
            request("malformed-secret-json"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(
            !json(response)
                .await
                .to_string()
                .contains("malformed-secret-json")
        );
    }
    assert!(snapshot(&pool, current.id).await == before);
}
