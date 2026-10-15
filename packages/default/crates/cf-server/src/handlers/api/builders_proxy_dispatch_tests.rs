//! Exercises confidential builder dispatch with signed requests over real TCP.

use super::*;
use axum::{Router, routing::post};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use std::{net::SocketAddr, sync::Arc};

fn proxy_config(flag: bool, cidrs: &[&str]) -> crate::config::ServerConfig {
    crate::config::ServerConfig {
        trust_forwarded_builder_https: flag,
        trusted_proxy_cidrs: cidrs.iter().map(|cidr| (*cidr).into()).collect(),
        remote_build_execution_strategy: RemoteBuildExecutionStrategy::ServerDerivation,
        ..Default::default()
    }
}

#[test]
fn proxy_evidence_exact_direct_peer_matrix() {
    let loopback: SocketAddr = "127.0.0.1:47000".parse().unwrap();
    for (name, flag, cidrs, peer, values, accepted) in [
        (
            "false flag",
            false,
            vec!["127.0.0.1/32"],
            Some(loopback),
            vec!["https"],
            false,
        ),
        (
            "empty CIDRs",
            true,
            vec![],
            Some(loopback),
            vec!["https"],
            false,
        ),
        (
            "wrong peer",
            true,
            vec!["192.0.2.1/32"],
            Some(loopback),
            vec!["https"],
            false,
        ),
        (
            "missing peer",
            true,
            vec!["127.0.0.1/32"],
            None,
            vec!["https"],
            false,
        ),
        (
            "missing XFP",
            true,
            vec!["127.0.0.1/32"],
            Some(loopback),
            vec![],
            false,
        ),
        (
            "duplicate XFP",
            true,
            vec!["127.0.0.1/32"],
            Some(loopback),
            vec!["https", "https"],
            false,
        ),
        (
            "comma protocol",
            true,
            vec!["127.0.0.1/32"],
            Some(loopback),
            vec!["https,http"],
            false,
        ),
        (
            "HTTP",
            true,
            vec!["127.0.0.1/32"],
            Some(loopback),
            vec!["http"],
            false,
        ),
        (
            "case",
            true,
            vec!["127.0.0.1/32"],
            Some(loopback),
            vec!["HTTPS"],
            false,
        ),
        (
            "leading space",
            true,
            vec!["127.0.0.1/32"],
            Some(loopback),
            vec![" https"],
            false,
        ),
        (
            "trailing space",
            true,
            vec!["127.0.0.1/32"],
            Some(loopback),
            vec!["https "],
            false,
        ),
        (
            "exact HTTPS",
            true,
            vec!["127.0.0.1/32"],
            Some(loopback),
            vec!["https"],
            true,
        ),
    ] {
        let config = proxy_config(flag, &cidrs);
        let mut headers = HeaderMap::new();
        // Alternative assertions cannot repair missing direct-peer/XFP evidence.
        headers.insert("forwarded", "for=127.0.0.1;proto=https".parse().unwrap());
        headers.insert("x-forwarded-for", "127.0.0.1".parse().unwrap());
        headers.insert("x-forwarded-ssl", "on".parse().unwrap());
        for value in &values {
            headers.append("x-forwarded-proto", value.parse().unwrap());
        }
        let evidence = BuilderHttpsEvidence::from_request(&config, &headers, peer);
        assert_eq!(
            evidence.direct_peer_ip,
            peer.map(|peer| peer.ip()),
            "{name}"
        );
        assert_eq!(evidence.trust_forwarded_builder_https, flag, "{name}");
        assert_eq!(
            evidence.peer_cidr_match,
            peer.is_some() && cidrs == ["127.0.0.1/32"],
            "{name}"
        );
        assert_eq!(evidence.x_forwarded_proto_count, values.len(), "{name}");
        assert_eq!(evidence.exact_https, values == ["https"], "{name}");
        assert_eq!(evidence.verified(), accepted, "{name}");
        assert_eq!(
            builder_https_verified_by_trusted_proxy(&config, &headers, peer),
            accepted,
            "{name}"
        );
    }
}

fn cache_input(
    cache_type: &str,
    environment: Uuid,
) -> crate::models::cache_destination::CreateCacheDestination {
    use crate::models::cache_destination::{CreateCacheDestination, nix_public_key_fixture};
    let mut input = CreateCacheDestination {
        name: format!("proxy-{cache_type}-{environment}"),
        cache_type: cache_type.into(),
        push_to: Some("https://read.example.invalid/cache".into()),
        enabled: Some(true),
        environment_ids: Some(vec![environment]),
        ..Default::default()
    };
    match cache_type {
        "Attic" => {
            input.attic_token = Some("proxy-fixture-attic-secret".into());
            input.attic_cache_name = Some("proxy-fixture".into());
            input.attic_public_key = Some(nix_public_key_fixture("proxy-fixture"));
        }
        "S3" => {
            input.push_to = Some("s3://proxy-fixture-cache".into());
            input.s3_region = Some("us-east-1".into());
            input.s3_endpoint_url = Some("https://s3.example.invalid".into());
            input.s3_access_key_id = Some("proxy-fixture-access-id".into());
            input.s3_secret_access_key = Some("proxy-fixture-s3-secret".into());
            input.s3_session_token = Some("proxy-fixture-session-token".into());
        }
        "Niks3" => {
            input.niks3_server_url = Some("https://write.example.invalid".into());
            input.niks3_public_keys = vec![nix_public_key_fixture("proxy-fixture")];
            input.niks3_write_auth_mode = Some("token".into());
            input.niks3_auth_token = Some("proxy-fixture-niks3-secret".into());
            input.niks3_read_auth_mode = Some("none".into());
        }
        "Http" | "Nix" => {}
        _ => panic!("unexpected fixture cache type"),
    }
    input.validate().unwrap();
    input
}

#[test]
fn proxy_credential_detection_covers_each_secret_field() {
    use cf_protocol::cache::Niks3WriteAuth;
    let public = BuilderCachePushConfig::disabled();
    assert!(!cache_push_config_contains_credentials(&public));
    for field in [
        "attic_token",
        "s3_access_key_id",
        "s3_secret_access_key",
        "s3_session_token",
    ] {
        let mut value = serde_json::to_value(&public).unwrap();
        value[field] = serde_json::json!("proxy-fixture-secret");
        assert!(
            cache_push_config_contains_credentials(&serde_json::from_value(value).unwrap()),
            "{field}"
        );
    }
    for auth in [
        Niks3WriteAuth::Token {
            token: "proxy-fixture-token".into(),
        },
        Niks3WriteAuth::Mtls {
            client_certificate: "fixture-certificate".into(),
            client_private_key: "fixture-private-key".into(),
            ca_certificate: None,
        },
    ] {
        let mut config = public.clone();
        config.niks3_write_auth = Some(auth);
        assert!(cache_push_config_contains_credentials(&config));
    }
}

// Uses the same public insertion APIs and policy-passing queue fixture as the
// preclaim regression. Every case gets a scoped environment and real session;
// retries and earlier claimed jobs cannot change another case's candidate.
async fn queued_fixture(pool: &PgPool, environment: Uuid) -> (Uuid, Uuid, SigningKey, Uuid) {
    let suffix = Uuid::new_v4().simple().to_string();
    let key = SigningKey::generate(&mut rand::thread_rng());
    let (builder, _) = builders::create_builder(
        pool,
        &CreateBuilderRequest {
            name: format!("proxy-{suffix}"),
            host: None,
            arch: "x86_64-linux".into(),
            public_key: Some(general_purpose::STANDARD.encode(key.verifying_key().to_bytes())),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: Some(4),
            enabled: Some(true),
            environment_ids: vec![environment],
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE builders SET status = 'active' WHERE id = $1")
        .bind(builder.id)
        .execute(pool)
        .await
        .unwrap();
    let session = Uuid::new_v4();
    builders::establish_builder_session(pool, &builder.id, &session, 60, "proxy regression")
        .await
        .unwrap();
    let repo = format!("https://example.invalid/{suffix}.git");
    crate::queries::flakes::insert_flake(pool, &suffix, &repo, "main", "all_configs")
        .await
        .unwrap();
    let hash = format!("{suffix}00000000");
    crate::queries::commits::insert_commit_with_metadata(
        pool,
        &hash,
        &repo,
        Utc::now(),
        Some("proxy fixture"),
        Some("test"),
    )
    .await
    .unwrap();
    let commit = crate::queries::commits::get_commit_by_hash(pool, &hash)
        .await
        .unwrap();
    let derivation = crate::queries::derivations::insert_derivation_with_target(
        pool,
        Some(&commit),
        &suffix,
        "nixos",
        Some(&suffix),
        Some(true),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE derivations SET cf_agent_enabled = TRUE, policy_requirements_met = TRUE WHERE id = $1")
        .bind(derivation.id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO systems (hostname, system_configuration_name, public_key, derivation, flake_id, environment_id) VALUES ($1, $1, 'test-key', '', $2, $3)")
        .bind(&suffix).bind(commit.flake_id).bind(environment).execute(pool).await.unwrap();
    let job = sqlx::query_scalar("INSERT INTO build_jobs (derivation_id, environment_id, status, priority_weight, queue_position) VALUES ($1, $2, 'queued', 1.0, 1) RETURNING id")
        .bind(derivation.id).bind(environment).fetch_one(pool).await.unwrap();
    (job, builder.id, key, session)
}

// Test-only listener ownership ensures a panic also stops the TCP server.
struct ListenerTask(tokio::task::JoinHandle<()>);
impl Drop for ListenerTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires verified isolated database creation privileges"]
async fn proxy_credential_dispatch_signed_tcp_next_job(pool: PgPool) {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    for cache_type in ["Attic", "S3", "Niks3", "Http", "Nix"] {
        for (name, flag, cidrs, connect_info, xfp, accepted) in [
            (
                "false-flag",
                false,
                vec!["127.0.0.1/32"],
                true,
                vec!["https"],
                false,
            ),
            ("empty-cidrs", false, vec![], true, vec!["https"], false),
            (
                "wrong-peer",
                true,
                vec!["192.0.2.1/32"],
                true,
                vec!["https"],
                false,
            ),
            (
                "missing-peer",
                true,
                vec!["127.0.0.1/32"],
                false,
                vec!["https"],
                false,
            ),
            (
                "missing-xfp",
                true,
                vec!["127.0.0.1/32"],
                true,
                vec![],
                false,
            ),
            (
                "duplicate-xfp",
                true,
                vec!["127.0.0.1/32"],
                true,
                vec!["https", "https"],
                false,
            ),
            (
                "comma-proto",
                true,
                vec!["127.0.0.1/32"],
                true,
                vec!["https,http"],
                false,
            ),
            (
                "http",
                true,
                vec!["127.0.0.1/32"],
                true,
                vec!["http"],
                false,
            ),
            (
                "case",
                true,
                vec!["127.0.0.1/32"],
                true,
                vec!["HTTPS"],
                false,
            ),
            (
                "trusted",
                true,
                vec!["127.0.0.1/32"],
                true,
                vec!["https"],
                true,
            ),
        ] {
            let environment: Uuid =
                sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                    .bind(format!("proxy-{cache_type}-{name}"))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let destination = crate::queries::cache_destinations::create_cache_destination(
                &pool,
                &cache_input(cache_type, environment),
            )
            .await
            .unwrap();
            let (job_id, builder_id, key, session) = queued_fixture(&pool, environment).await;
            let state = CFState::new(
                pool.clone(),
                proxy_config(flag, &cidrs),
                Arc::new(crate::queue::QueueNotifier::new()),
                crate::server::jobs::BackgroundJobRegistry::new(),
            );
            let app = Router::new()
                .route("/api/v1/builders/:builder_id/next-job", post(get_next_job))
                .with_state(state);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let _server = ListenerTask(tokio::spawn(async move {
                if connect_info {
                    axum::serve(
                        listener,
                        app.into_make_service_with_connect_info::<SocketAddr>(),
                    )
                    .await
                    .unwrap();
                } else {
                    axum::serve(listener, app.into_make_service())
                        .await
                        .unwrap();
                }
            }));
            let path = format!("/api/v1/builders/{builder_id}/next-job");
            let url = format!("http://{address}{path}");
            // Anonymous callers and unsigned capability headers cannot claim.
            let anonymous = client
                .post(&url)
                .header("X-Forwarded-Proto", "https")
                .header("X-Builder-Capabilities", "niks3_cache")
                .body("{}")
                .send()
                .await
                .unwrap();
            assert_eq!(anonymous.status().as_u16(), 401);
            assert_eq!(
                builders::get_build_job_by_id(&pool, &job_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .status,
                "queued"
            );
            let body = r#"{"capabilities":{"niks3_cache":true}}"#;
            let timestamp = Utc::now().to_rfc3339();
            let payload = format!("POST\n{path}\n{timestamp}\n{body}");
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("X-Builder-ID", builder_id.to_string().parse().unwrap());
            headers.insert("X-Builder-Session-ID", session.to_string().parse().unwrap());
            headers.insert("X-Timestamp", timestamp.parse().unwrap());
            headers.insert(
                "X-Signature",
                general_purpose::STANDARD
                    .encode(key.sign(payload.as_bytes()).to_bytes())
                    .parse()
                    .unwrap(),
            );
            headers.insert("Forwarded", "for=127.0.0.1;proto=https".parse().unwrap());
            headers.insert("X-Forwarded-For", "127.0.0.1".parse().unwrap());
            headers.insert("X-Forwarded-Ssl", "on".parse().unwrap());
            for value in xfp {
                headers.append("X-Forwarded-Proto", value.parse().unwrap());
            }
            if cache_type == "Niks3" {
                let mut spoof = headers.clone();
                spoof.insert("X-Builder-Capabilities", "niks3_cache".parse().unwrap());
                spoof.insert(
                    "X-Signature",
                    general_purpose::STANDARD
                        .encode(
                            key.sign(format!("POST\n{path}\n{timestamp}\n{{}}").as_bytes())
                                .to_bytes(),
                        )
                        .parse()
                        .unwrap(),
                );
                let rejected = client
                    .post(&url)
                    .headers(spoof)
                    .body("{}")
                    .send()
                    .await
                    .unwrap();
                assert_eq!(rejected.status().as_u16(), 409);
                assert_eq!(
                    rejected.json::<serde_json::Value>().await.unwrap(),
                    serde_json::json!({"reason":"unsupported_cache_type"})
                );
                assert_eq!(
                    builders::get_build_job_by_id(&pool, &job_id)
                        .await
                        .unwrap()
                        .unwrap()
                        .status,
                    "queued"
                );
            }
            let response = client
                .post(&url)
                .headers(headers)
                .body(body)
                .send()
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.bytes().await.unwrap();
            let persisted = builders::get_build_job_by_id(&pool, &job_id)
                .await
                .unwrap()
                .unwrap();
            let public = matches!(cache_type, "Http" | "Nix");
            if accepted || public {
                assert_eq!(status.as_u16(), 200, "{cache_type}/{name}");
                let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(value["job"]["id"], job_id.to_string());
                assert_eq!(
                    value["derivation"]["execution_strategy"],
                    "server_derivation"
                );
                assert_eq!(value["derivation"]["source_input_delivery"], "none");
                assert!(value["derivation"]["evaluator"].is_null());
                let cache = &value["derivation"]["cache_push"];
                assert_eq!(cache["cache_type"], cache_type);
                assert_eq!(cache["cache_destination_id"], destination.id);
                match cache_type {
                    "Attic" => assert_eq!(cache["attic_token"], "proxy-fixture-attic-secret"),
                    "S3" => {
                        assert_eq!(cache["s3_access_key_id"], "proxy-fixture-access-id");
                        assert_eq!(cache["s3_secret_access_key"], "proxy-fixture-s3-secret");
                        assert_eq!(cache["s3_session_token"], "proxy-fixture-session-token");
                    }
                    "Niks3" => assert_eq!(
                        cache["niks3_write_auth"]["token"],
                        "proxy-fixture-niks3-secret"
                    ),
                    _ => assert!(!cache_push_config_contains_credentials(
                        &serde_json::from_value(cache.clone()).unwrap()
                    )),
                }
                assert_eq!(persisted.status, "building");
                assert_eq!(persisted.builder_id, Some(builder_id));
                assert_eq!(persisted.builder_session_id, Some(session));
                assert_eq!(
                    persisted.dispatched_cache_destination_id,
                    Some(destination.id)
                );
                assert!(persisted.cache_dispatch_recorded_at.is_some());
                assert_eq!(persisted.retry_count, 0);
            } else {
                assert_eq!(status.as_u16(), 404, "{cache_type}/{name}");
                assert!(
                    bytes.is_empty(),
                    "denial must not return a credential payload"
                );
                assert!(persisted.dispatched_cache_destination_id.is_none());
                assert!(persisted.cache_dispatch_recorded_at.is_none());
                assert_eq!(persisted.status, "failed");
                let error = sqlx::query_scalar::<_, Option<String>>(
                    "SELECT logs FROM build_jobs WHERE id = $1",
                )
                .bind(job_id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .unwrap();
                assert!(error.contains("cache_config"));
                assert!(
                    error.contains("services.crystal-forge.server.trust_forwarded_builder_https")
                );
                assert!(error.contains("services.crystal-forge.server.trustedProxyCidrs"));
                for secret in [
                    "proxy-fixture-attic-secret",
                    "proxy-fixture-s3-secret",
                    "proxy-fixture-session-token",
                    "proxy-fixture-niks3-secret",
                    "read.example.invalid",
                    "write.example.invalid",
                ] {
                    assert!(!error.contains(secret));
                }
            }
        }
    }
}
