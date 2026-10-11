//! Proves CA compatibility, strict identities and non-mutating canonical reads.

use super::*;
use crate::security::cache_secrets::TEST_CERTIFICATE;
use base64::{Engine as _, engine::general_purpose::STANDARD};

fn trusted_only(cert: &str) -> String {
    let encoded = cert
        .strip_prefix("-----BEGIN CERTIFICATE-----")
        .unwrap()
        .split_once("-----END CERTIFICATE-----")
        .unwrap()
        .0;
    let mut der = STANDARD
        .decode(encoded.split_whitespace().collect::<String>())
        .unwrap();
    // OpenSSL AUX with an empty explicit trust list. This record contributes no
    // ordinary root to Go and must not be promoted during normalization.
    der.extend_from_slice(&[0x30, 0x02, 0x30, 0x00]);
    format!(
        "-----BEGIN TRUSTED CERTIFICATE-----\n{}\n-----END TRUSTED CERTIFICATE-----\n",
        STANDARD.encode(der)
    )
}

fn mixed_ca() -> String {
    format!(
        "# runtime synthetic CA annotation\n{TEST_CERTIFICATE}\n{}",
        trusted_only(TEST_CERTIFICATE)
    )
}

#[test]
fn ca_normalization_keeps_roots_separate_from_strict_client_chains() {
    let mixed = mixed_ca();
    let canonical = crate::security::cache_secrets::normalize_ca_bundle(&mixed).unwrap();
    assert_eq!(canonical, TEST_CERTIFICATE);
    let mut create = CreateCacheDestination {
        niks3_write_ca_cert: Some(mixed.clone()),
        niks3_read_ca_cert: Some(mixed),
        ..Default::default()
    };
    create.normalize_ca_fields().unwrap();
    assert_eq!(
        create.niks3_write_ca_cert.as_deref(),
        Some(TEST_CERTIFICATE)
    );
    assert_eq!(create.niks3_read_ca_cert.as_deref(), Some(TEST_CERTIFICATE));
    for invalid in [
        trusted_only(TEST_CERTIFICATE),
        format!("# client annotation\n{TEST_CERTIFICATE}"),
    ] {
        assert!(client_chain_diagnostic(Some(&invalid), None).is_err());
    }
    let mut inactive = CreateCacheDestination {
        name: "inactive-private-material".into(),
        cache_type: "Http".into(),
        push_to: Some("https://read.example/".into()),
        niks3_write_ca_cert: Some(format!(
            "{TEST_CERTIFICATE}\n-----BEGIN PRIVATE KEY-----\nAQID\n-----END PRIVATE KEY-----"
        )),
        ..Default::default()
    };
    assert!(inactive.normalize_ca_fields().is_err());
    assert!(inactive.validate().is_err());
    // A leading trim would make the first two inputs valid. A clean first root
    // must not hide a later indented delimiter either. Every backend projection
    // must preserve these original bytes until the shared parser rejects them.
    for raw in [
        format!(" {TEST_CERTIFICATE}"),
        format!("\t{TEST_CERTIFICATE}"),
        format!("{TEST_CERTIFICATE}\n {TEST_CERTIFICATE}"),
    ] {
        let error = crate::security::cache_secrets::normalize_ca_bundle(&raw).unwrap_err();
        assert_eq!(
            error.downcast_ref::<cf_config::ca_bundle::CaBundleError>(),
            Some(&cf_config::ca_bundle::CaBundleError::InvalidFraming)
        );
        for field in ["niks3_write_ca_cert", "niks3_read_ca_cert"] {
            let mut create: CreateCacheDestination = serde_json::from_value(serde_json::json!({
                "name":"raw-ca-guard", "cache_type":"Http", "push_to":"https://read.example/", (field): raw,
            })).unwrap();
            let before = serde_json::to_value(&create).unwrap();
            assert!(create.validate().is_err());
            let error = create.normalize_ca_fields().unwrap_err();
            assert_eq!(ca_diagnostic(&error).code, "ca_bundle_framing_invalid");
            assert_eq!(ca_diagnostic(&error).field, Some(field));
            assert_eq!(serde_json::to_value(create).unwrap(), before);
            let mut update: UpdateCacheDestination =
                serde_json::from_value(serde_json::json!({(field): raw})).unwrap();
            assert!(update.normalize_ca_fields().is_err());
            let current = CacheDestination {
                name: "raw-ca-guard".into(),
                cache_type: "Http".into(),
                push_to: Some("https://read.example/".into()),
                ..Default::default()
            };
            assert!(cache_destinations::effective_update_unvalidated(&current, &update).is_err());
            let mut stored = current;
            if field == "niks3_write_ca_cert" {
                stored.niks3_write_ca_cert = Some(raw.clone());
            } else {
                stored.niks3_read_ca_cert = Some(raw.clone());
            }
            assert!(stored.normalize_ca_fields().is_err());
        }
    }
}

#[tokio::test]
async fn ca_diagnostics_are_typed_static_and_field_scoped() {
    use cf_config::ca_bundle::CaBundleError;
    // This is the additive wire allowlist shared with the UI, including the
    // aggregate identity fallback and three precise local validation failures.
    for code in [
        "ca_bundle_framing_invalid",
        "ca_certificate_invalid",
        "ca_bundle_content_unsupported",
        "ca_bundle_trust_ambiguous",
        "ca_bundle_empty",
        "ca_bundle_too_large",
        "client_identity_invalid",
        "client_certificate_invalid",
        "client_private_key_invalid",
        "client_identity_mismatch",
        "tls_verification_failed",
        "dns_or_connection_failed",
        "http_response_failed",
        "invalid_niks3_metadata",
        "target_policy_rejected",
        "probe_timeout",
        "probe_failed",
    ] {
        assert_eq!(diagnostic(code).code, code);
    }
    for error in [
        CaBundleError::InvalidFraming,
        CaBundleError::InvalidX509,
        CaBundleError::UnsupportedContent,
        CaBundleError::AmbiguousTrust,
        CaBundleError::NoCertificates,
        CaBundleError::TooLarge,
    ] {
        let error = anyhow::Error::new(error).context("niks3_write_ca_cert");
        let hint = ca_diagnostic(&error);
        assert_ne!(hint.code, "probe_failed");
        assert_eq!(hint.field, Some("niks3_write_ca_cert"));
        let response = diagnostic_error(StatusCode::BAD_REQUEST, "invalid_cache_test_config", hint);
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["diagnostic_code"], hint.code);
        assert_eq!(body["field"], "niks3_write_ca_cert");
        assert!(body["details"].is_null());
    }
    let raw = "https://private.invalid/?token=PRIVATE_SENTINEL BEGIN PRIVATE KEY Authorization";
    let hint = static_probe_diagnostic(raw);
    assert_eq!(hint.code, "probe_failed");
    assert!(!hint.message.contains("PRIVATE_SENTINEL"));
    for error in [
        cf_config::ca_bundle::ClientIdentityError::InvalidCertificate,
        cf_config::ca_bundle::ClientIdentityError::InvalidPrivateKey,
        cf_config::ca_bundle::ClientIdentityError::KeyMismatch,
    ] {
        let typed = anyhow::Error::new(error).context("PRIVATE_SENTINEL");
        let hint = client_identity_diagnostic(&typed);
        assert_eq!(hint.code, error.code());
        assert!(!hint.message.contains("PRIVATE_SENTINEL"));
        let response = diagnostic_error(
            StatusCode::BAD_REQUEST,
            "invalid_niks3_discovery",
            discovery_diagnostic(hint.code),
        );
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["diagnostic_code"], error.code());
        assert!(!body.to_string().contains("PRIVATE_SENTINEL"));
    }
}

#[test]
fn transport_diagnostics_use_local_causes_not_url_or_body_text() {
    let spoof = std::io::Error::other(
        "https://private.invalid/?hint=invalid peer certificate: PRIVATE_SENTINEL",
    );
    assert_eq!(tls_verification_cause(Some(&spoof)), None);
    for kind in [std::io::ErrorKind::InvalidData, std::io::ErrorKind::Other] {
        for message in [
            "invalid peer certificate: UnknownIssuer",
            "invalid peer certificate: NotValidForName",
            "invalid peer certificate: Expired",
            "invalid certificate: BadEncoding",
        ] {
            let tls = std::io::Error::new(kind, message);
            assert_eq!(
                tls_verification_cause(Some(&tls)),
                Some("tls_verification_failed")
            );
        }
    }
    #[derive(Debug)]
    struct Wrapper(Box<dyn std::error::Error + Send + Sync>);
    impl std::fmt::Display for Wrapper {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("local transport wrapper")
        }
    }
    impl std::error::Error for Wrapper {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(self.0.as_ref())
        }
    }
    let nested = std::io::Error::other(Wrapper(Box::new(std::io::Error::other(
        "invalid peer certificate: UnknownIssuer",
    ))));
    assert_eq!(
        tls_verification_cause(Some(&nested)),
        Some("tls_verification_failed")
    );
    let nested_spoof = std::io::Error::other(Wrapper(Box::new(std::io::Error::other(
        "HTTP response body: invalid peer certificate: PRIVATE_SENTINEL",
    ))));
    assert_eq!(tls_verification_cause(Some(&nested_spoof)), None);
    for (alert, code) in [
        ("BadCertificate", "client_identity_invalid"),
        ("CertificateRequired", "client_identity_invalid"),
        ("UnknownCA", "client_identity_invalid"),
        ("HandshakeFailure", "probe_failed"),
    ] {
        let local = std::io::Error::other(format!("received fatal alert: {alert}"));
        assert_eq!(tls_verification_cause(Some(&local)), Some(code));
    }
    let excessive = (0..20).fold(
        Box::new(std::io::Error::other(
            "invalid peer certificate: UnknownIssuer",
        )) as Box<dyn std::error::Error + Send + Sync>,
        |inner, _| Box::new(Wrapper(inner)),
    );
    assert_eq!(
        tls_verification_cause(Some(&std::io::Error::other(excessive))),
        None
    );
    #[derive(Debug)]
    struct Cycle;
    impl std::fmt::Display for Cycle {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("unknown local cause")
        }
    }
    impl std::error::Error for Cycle {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(self)
        }
    }
    assert_eq!(tls_verification_cause(Some(&Cycle)), None);
    let unknown = std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "invalid TLS message PRIVATE_SENTINEL",
    );
    assert_eq!(tls_verification_cause(Some(&unknown)), None);
    for message in [
        "Failed to resolve cache test host",
        "Cache test DNS lookup timed out",
        "Refusing to test private, loopback, or non-routable IP",
    ] {
        assert_ne!(static_target_failure(message), "probe_failed");
    }
    assert_eq!(static_target_failure("PRIVATE_SENTINEL"), "probe_failed");
    for status in [301, 302, 401, 403, 404, 500] {
        let message =
            require_probe_success(reqwest::StatusCode::from_u16(status).unwrap()).unwrap_err();
        assert_eq!(
            static_probe_diagnostic(&message).code,
            "http_response_failed"
        );
    }
}

#[tokio::test]
async fn mismatched_write_identity_fails_before_dns_and_never_proves_auth() {
    let (cert, matching_key) = super::discovery_tests::identity();
    let (_, wrong_key) = super::discovery_tests::identity();
    let mut request = Niks3DiscoverRequest {
        server_url: "https://must-not-resolve.invalid/".into(),
        niks3_write_auth_mode: Some("mtls".into()),
        niks3_write_client_cert: Some(cert.clone()),
        niks3_write_client_key: Some(wrong_key),
        niks3_write_ca_cert: Some(mixed_ca()),
    };
    assert_eq!(
        discover_niks3(&request, false).await.unwrap_err(),
        "client_identity_mismatch"
    );
    request.niks3_write_client_key = Some("PRIVATE_SENTINEL".into());
    assert_eq!(
        discover_niks3(&request, false).await.unwrap_err(),
        "client_private_key_invalid"
    );
    request.niks3_write_client_cert = Some(trusted_only(TEST_CERTIFICATE));
    assert_eq!(
        discover_niks3(&request, false).await.unwrap_err(),
        "client_certificate_invalid"
    );
    let stages = Niks3ConnectionTestResult::default();
    assert!(stages.write_authn_valid.is_none());
    assert!(stages.write_authorization_valid.is_none());
    request.niks3_write_client_cert = Some(cert);
    request.niks3_write_client_key = Some(matching_key);
    for raw in [
        format!(" {TEST_CERTIFICATE}"),
        format!("{TEST_CERTIFICATE}\n\t{TEST_CERTIFICATE}"),
    ] {
        request.niks3_write_ca_cert = Some(raw);
        assert_eq!(
            discover_niks3(&request, false).await.unwrap_err(),
            "ca_bundle_framing_invalid"
        );
    }
}

#[tokio::test]
async fn custom_write_roots_skip_ambient_trust_and_validate_every_root() {
    assert!(system_ca_source(RootPolicy::ReplaceCustom).is_none());
    assert!(system_ca_source(RootPolicy::ExtendSystem).is_some());
    let url = Url::parse("https://must-not-resolve.invalid/").unwrap();
    let addresses = ["127.0.0.1:443".parse().unwrap()];
    let client = cache_test_client_pinned_policy(
        &url,
        &addresses,
        None,
        None,
        Some(&mixed_ca()),
        RootPolicy::ReplaceCustom,
    )
    .await;
    assert!(client.is_ok()); // Builds a pinned client only; no request is sent.
    assert_eq!(
        cache_test_client_pinned_policy(
            &url,
            &addresses,
            None,
            None,
            None,
            RootPolicy::ReplaceCustom
        )
        .await
        .unwrap_err(),
        "ca_bundle_empty"
    );
    assert!(
        cache_test_client_pinned_policy(
            &url,
            &addresses,
            None,
            None,
            Some(&format!(
                "{}\n-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----",
                mixed_ca()
            )),
            RootPolicy::ReplaceCustom
        )
        .await
        .is_err()
    );
}

async fn raw_snapshot(pool: &PgPool, id: i32) -> serde_json::Value {
    sqlx::query_scalar("SELECT to_jsonb(cd) FROM cache_destinations cd WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated test database creation privileges"]
async fn canonical_ca_get_test_cancel_and_ca_only_save_preserve_ciphertext(pool: PgPool) {
    let (cert, key) = super::discovery_tests::identity();
    let (read_cert, read_key) = super::discovery_tests::identity();
    let environment: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES ('canonical-ca-scope') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let create = CreateCacheDestination {
        name: "canonical-ca-fixture".into(),
        cache_type: "Niks3".into(),
        push_to: Some("https://read.example/".into()),
        niks3_server_url: Some("https://write.example/".into()),
        niks3_public_keys: vec![crate::models::cache_destination::nix_public_key_fixture(
            "fixture",
        )],
        niks3_write_auth_mode: Some("mtls".into()),
        niks3_read_auth_mode: Some("mtls".into()),
        niks3_write_client_cert: Some(cert),
        niks3_write_client_key: Some(key),
        niks3_write_ca_cert: Some(mixed_ca()),
        niks3_read_client_cert: Some(read_cert),
        niks3_read_client_key: Some(read_key),
        niks3_read_ca_cert: Some(mixed_ca()),
        environment_ids: Some(vec![environment]),
        ..Default::default()
    };
    let destination = cache_destinations::create_cache_destination(&pool, &create)
        .await
        .unwrap();
    assert_eq!(
        raw_snapshot(&pool, destination.id).await["niks3_write_ca_cert"],
        TEST_CERTIFICATE
    );
    // Direct SQL models a previously stored recognized bundle. GET/Test/Cancel
    // must canonicalize only their snapshots, not the raw row or ciphertext.
    sqlx::query("UPDATE cache_destinations SET niks3_write_ca_cert=$2 WHERE id=$1")
        .bind(destination.id)
        .bind(mixed_ca())
        .execute(&pool)
        .await
        .unwrap();
    let before = raw_snapshot(&pool, destination.id).await;
    let scope_before: serde_json::Value = sqlx::query_scalar("SELECT to_jsonb(cde) FROM cache_destination_environments cde WHERE cache_destination_id=$1")
        .bind(destination.id).fetch_one(&pool).await.unwrap();
    let loaded = cache_destinations::get_cache_destination(&pool, destination.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        loaded.niks3_write_ca_cert.as_deref(),
        Some(TEST_CERTIFICATE)
    );
    assert_eq!(
        loaded.niks3_write_auth().unwrap(),
        destination.niks3_write_auth().unwrap()
    );
    let headers = super::tests::admin_headers(&pool).await;
    let response =
        get_cache_destination(State(pool.clone()), headers.clone(), Path(destination.id))
            .await
            .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let public: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(public["niks3_write_ca_cert"], TEST_CERTIFICATE);
    assert!(public["niks3_write_client_key"].is_null());
    assert!(public["niks3_read_client_key"].is_null());
    assert!(!public.to_string().contains("TRUSTED CERTIFICATE"));
    assert!(!public.to_string().contains("annotation"));
    let listed = cache_destinations::list_cache_destinations(&pool, false)
        .await
        .unwrap();
    assert_eq!(
        listed
            .iter()
            .find(|row| row.id == destination.id)
            .unwrap()
            .niks3_write_ca_cert
            .as_deref(),
        Some(TEST_CERTIFICATE)
    );
    let request = axum::http::Request::builder()
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{}"))
        .unwrap();
    let response = test_stored_with_probe(
        &pool,
        &headers,
        destination.id,
        request,
        |effective| async move {
            assert_eq!(
                effective.niks3_write_ca_cert.as_deref(),
                Some(TEST_CERTIFICATE)
            );
            Ok(CacheCredentialTestResult {
                ok: true,
                status_code: Some(200),
                message: "synthetic metadata only".into(),
                tested_url: None,
                niks3: Some(CacheProbeDetails::Niks3(
                    Niks3ConnectionTestResult::default(),
                )),
            })
        },
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let request = axum::http::Request::builder()
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{}"))
        .unwrap();
    let response = stored_niks3_discovery_with(
        &pool,
        &headers,
        destination.id,
        request,
        |request| async move {
            assert_eq!(
                request.niks3_write_ca_cert.as_deref(),
                Some(TEST_CERTIFICATE)
            );
            Ok(Niks3Discovery {
                server_url: "https://write.example/".into(),
                substituter_url: "https://read.example/".into(),
                public_keys: vec![crate::models::cache_destination::nix_public_key_fixture(
                    "fixture",
                )],
                oidc_audience: None,
            })
        },
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(raw_snapshot(&pool, destination.id).await, before);
    assert_eq!(sqlx::query_scalar::<_, serde_json::Value>("SELECT to_jsonb(cde) FROM cache_destination_environments cde WHERE cache_destination_id=$1")
        .bind(destination.id).fetch_one(&pool).await.unwrap(), scope_before);
    // Cancel sends no request. Save is the only operation allowed to persist CA.
    let update = UpdateCacheDestination {
        niks3_write_ca_cert: Some(format!("{}{}", mixed_ca(), TEST_CERTIFICATE)),
        niks3_read_ca_cert: Some(format!("{}{}", mixed_ca(), TEST_CERTIFICATE)),
        ..Default::default()
    };
    cache_destinations::update_cache_destination(&pool, destination.id, &update)
        .await
        .unwrap();
    let mut after = raw_snapshot(&pool, destination.id).await;
    assert_eq!(
        after["niks3_write_ca_cert"],
        format!("{TEST_CERTIFICATE}{TEST_CERTIFICATE}")
    );
    assert_eq!(
        after["niks3_write_client_key"],
        before["niks3_write_client_key"]
    );
    assert_eq!(
        after["niks3_read_client_key"],
        before["niks3_read_client_key"]
    );
    let mut protected = before;
    for field in ["updated_at", "niks3_write_ca_cert", "niks3_read_ca_cert"] {
        after.as_object_mut().unwrap().remove(field);
        protected.as_object_mut().unwrap().remove(field);
    }
    assert_eq!(after, protected);
    assert_eq!(sqlx::query_scalar::<_, serde_json::Value>("SELECT to_jsonb(cde) FROM cache_destination_environments cde WHERE cache_destination_id=$1")
        .bind(destination.id).fetch_one(&pool).await.unwrap(), scope_before);
    let invalid = UpdateCacheDestination {
        niks3_write_ca_cert: Some("PRIVATE_SENTINEL".into()),
        ..Default::default()
    };
    let saved = raw_snapshot(&pool, destination.id).await;
    let response = update_cache_destination(
        State(pool.clone()),
        headers.clone(),
        Path(destination.id),
        Json(invalid),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["diagnostic_code"], "ca_bundle_content_unsupported");
    assert_eq!(body["field"], "niks3_write_ca_cert");
    assert!(!body.to_string().contains("PRIVATE_SENTINEL"));
    assert_eq!(raw_snapshot(&pool, destination.id).await, saved);
    for field in ["niks3_write_ca_cert", "niks3_read_ca_cert"] {
        for raw in [
            format!(" {TEST_CERTIFICATE}"),
            format!("{TEST_CERTIFICATE}\n\t{TEST_CERTIFICATE}"),
        ] {
            let update: UpdateCacheDestination =
                serde_json::from_value(serde_json::json!({(field): raw})).unwrap();
            let response = update_cache_destination(
                State(pool.clone()),
                headers.clone(),
                Path(destination.id),
                Json(update.clone()),
            )
            .await
            .into_response();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["diagnostic_code"], "ca_bundle_framing_invalid");
            assert_eq!(body["field"], field);
            assert!(!body.to_string().contains("BEGIN CERTIFICATE"));
            let request = axum::http::Request::builder()
                .method("POST")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&update).unwrap(),
                ))
                .unwrap();
            let response = test_stored_with_probe(
                &pool,
                &headers,
                destination.id,
                request,
                |_| -> std::future::Ready<Result<CacheCredentialTestResult, String>> {
                    panic!("Indented CA must fail before probe callback")
                },
            )
            .await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["diagnostic_code"], "ca_bundle_framing_invalid");
            assert_eq!(body["field"], field);
            assert_eq!(raw_snapshot(&pool, destination.id).await, saved);
        }
    }
    // A legacy raw row with an indented first delimiter must fail closed before
    // API delivery or retained Test. Neither operation may rewrite that row.
    sqlx::query("UPDATE cache_destinations SET niks3_write_ca_cert=$2 WHERE id=$1")
        .bind(destination.id)
        .bind(format!(" {TEST_CERTIFICATE}"))
        .execute(&pool)
        .await
        .unwrap();
    let legacy_raw = raw_snapshot(&pool, destination.id).await;
    let error = cache_destinations::get_cache_destination(&pool, destination.id)
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<cf_config::ca_bundle::CaBundleError>(),
        Some(&cf_config::ca_bundle::CaBundleError::InvalidFraming)
    );
    let request = axum::http::Request::builder()
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{}"))
        .unwrap();
    let response = test_stored_with_probe(
        &pool,
        &headers,
        destination.id,
        request,
        |_| -> std::future::Ready<Result<CacheCredentialTestResult, String>> {
            panic!("Unsafe stored CA must fail before probe callback")
        },
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["diagnostic_code"], "ca_bundle_framing_invalid");
    assert_eq!(body["field"], "niks3_write_ca_cert");
    assert!(!body.to_string().contains("BEGIN CERTIFICATE"));
    assert_eq!(raw_snapshot(&pool, destination.id).await, legacy_raw);
    sqlx::query("UPDATE cache_destinations SET niks3_write_ca_cert=$2 WHERE id=$1")
        .bind(destination.id)
        .bind(saved["niks3_write_ca_cert"].as_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let clears = UpdateCacheDestination {
        niks3_write_auth_mode: Some("token".into()),
        niks3_auth_token: Some("runtime-token-fixture".into()),
        niks3_read_auth_mode: Some("none".into()),
        ..Default::default()
    };
    cache_destinations::update_cache_destination(&pool, destination.id, &clears)
        .await
        .unwrap();
    let cleared = raw_snapshot(&pool, destination.id).await;
    for field in [
        "niks3_write_client_cert",
        "niks3_write_client_key",
        "niks3_write_ca_cert",
        "niks3_read_client_cert",
        "niks3_read_client_key",
        "niks3_read_ca_cert",
    ] {
        assert!(cleared[field].is_null());
    }
}
