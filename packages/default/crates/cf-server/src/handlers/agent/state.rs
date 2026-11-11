use crate::handlers::agent_request::deserialize_system_state_versioned;
use crate::handlers::agent_request::{
    CFState, SystemLookup, authenticate_agent_request_with_lookup,
};
use crate::models::system_states::SystemState;
use crate::queries::system_events::{
    lock_observed_system_state_by_hostname_tx, record_report_events_tx,
};
use crate::queries::system_states::insert_system_state;
use anyhow::Result;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use tracing::{debug, info};

type InsertFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// Persists one reported system state and its retention side effects atomically.
///
/// The transaction records state-transition events, inserts the observation,
/// and retains an exact deployment-bound evaluation artifact when eligible. If
/// ordinary retention finds no eligible binding, it runs the bounded
/// reconciliation repair for the same observation.
/// It acquires the snapshot-writer advisory lock before POA&M keys and the
/// system observation row to preserve the repository-wide lock order. A
/// retention miss is a successful no-op and does not roll back the observation.
///
/// # Errors
///
/// Returns an error when a lock cannot be acquired, an event or observation
/// cannot be persisted, retention fails, or PostgreSQL cannot commit the
/// transaction. Any error rolls back all writes in this transaction.
#[cfg(test)]
pub(crate) async fn persist_reported_system_state(
    pool: &PgPool,
    payload: &SystemState,
    version_compatible: bool,
) -> Result<()> {
    persist_system_state_inner(pool, payload, version_compatible, None).await
}

async fn persist_system_state_inner(
    pool: &PgPool,
    payload: &SystemState,
    version_compatible: bool,
    verified: Option<&crate::models::systems::System>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    // CONCURRENCY: Snapshot publication, deployment creation, and state
    // ingestion acquire this lock before POA&M and system-row locks.
    crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx).await?;
    crate::services::composite_enforcement::lock_poam_derivations_for_store_path_tx(
        &mut tx,
        payload.store_path.as_deref(),
    )
    .await?;
    let system_id =
        crate::queries::system_events::find_system_id_by_hostname_tx(&mut tx, &payload.hostname)
            .await?;
    if let Some(system_id) = system_id {
        crate::services::composite_enforcement::lock_poam_findings_for_system_tx(
            &mut tx, system_id,
        )
        .await?;
    }
    // SECURITY: Production callers supply the authenticated identity. Recheck
    // after POA&M locks, before events, observations or retention can be written.
    if let Some(verified) = verified {
        if crate::queries::system_states::lock_agent_enrollment_tx(&mut tx, verified)
            .await?
            .is_none()
        {
            return Err(anyhow::Error::new(RevokedAgentEnrollment));
        }
    }
    let previous_observed =
        lock_observed_system_state_by_hostname_tx(&mut tx, &payload.hostname).await?;

    // The legacy state endpoint does not perform boot_id classification;
    // restart_type is not known here and is left NULL. Generation/store-path
    // transitions can still emit event-backed history transactionally.
    record_report_events_tx(&mut tx, previous_observed.as_ref(), payload, None, None).await?;
    insert_system_state(&mut *tx, payload, version_compatible, None, None).await?;
    crate::queries::evaluation_snapshots::retain_observed_generation_snapshot_tx(
        &mut tx,
        &payload.hostname,
        system_id,
        payload.generation,
        payload.store_path.as_deref(),
        payload.timestamp.unwrap_or_else(chrono::Utc::now),
    )
    .await?;

    tx.commit().await?;
    if let Some(system_id) = system_id {
        crate::services::poam::schedule_scheduled_environment_cve_reconciliation_for_system(
            pool, system_id,
        );
    }
    Ok(())
}

#[derive(Debug)]
struct RevokedAgentEnrollment;

impl std::fmt::Display for RevokedAgentEnrollment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Agent enrollment was revoked before the state write")
    }
}

impl std::error::Error for RevokedAgentEnrollment {}

async fn update_with_lookup_and_insert<L, I>(
    headers: HeaderMap,
    body: Bytes,
    lookup: &L,
    insert: I,
) -> StatusCode
where
    L: SystemLookup,
    I: for<'a> Fn(&'a SystemState, bool, &'a crate::models::systems::System) -> InsertFuture<'a>,
{
    let agent_request = match authenticate_agent_request_with_lookup(&headers, body, lookup).await {
        Ok(req) => req,
        Err(status) => return status,
    };

    let (payload, version_compatible) = match deserialize_system_state_versioned(&agent_request) {
        Ok((state, compatible)) => (state, compatible),
        Err(e) => {
            debug!("All deserialization attempts failed: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    info!(
        "System state received from {}: {}",
        agent_request.system.hostname, payload
    );

    if let Err(e) = insert(&payload, version_compatible, &agent_request.system).await {
        if e.is::<RevokedAgentEnrollment>() {
            return StatusCode::FORBIDDEN;
        }
        debug!("failed to insert into DB: {e:?}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    if version_compatible {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    }
}

/// Handles the `/current-system` POST route.
/// Verifies the body signature using headers, parses the payload, and
/// stores system state info in the database.
pub async fn update(
    State(_state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let pool = pool.clone();
    update_with_lookup_and_insert(
        headers,
        body,
        &pool,
        |payload, version_compatible, verified| {
            let pool = pool.clone();
            Box::pin(async move {
                persist_system_state_inner(&pool, payload, version_compatible, Some(verified)).await
            })
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::agent_request::SystemLookup;
    use crate::models::{public_key::PublicKey, system_states::SystemStateV1, systems::System};
    use crate::test_utils::{builders::SystemStateBuilder, crypto::generate_keypair};
    use axum::http::HeaderName;
    use base64::engine::{Engine, general_purpose};
    use chrono::Utc;
    use ed25519_dalek::Signer;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex;
    use uuid::Uuid;

    #[derive(Clone)]
    struct MockLookup {
        system: Option<System>,
    }

    impl MockLookup {
        fn new(system: Option<System>) -> Self {
            Self { system }
        }
    }

    impl SystemLookup for MockLookup {
        fn lookup(
            &self,
            _hostname: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<System>>> + Send>> {
            let system = self.system.clone();
            Box::pin(async move { Ok(system) })
        }
    }

    fn create_test_system(hostname: &str) -> (ed25519_dalek::SigningKey, System) {
        let (signing_key, verifying_key) = generate_keypair();
        let public_key = PublicKey::from_verifying_key(verifying_key);

        let system = System {
            id: Uuid::new_v4(),
            hostname: hostname.to_string(),
            environment_id: None,
            is_active: true,
            management_mode: crate::models::systems::ManagementMode::Managed,
            agent_enrolled_at: None,
            public_key: Some(public_key),
            parent_host_id: None,
            flake_id: Some(1),
            derivation: "/nix/store/test-system".to_string(),
            system_configuration_name: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            desired_target: None,
            deployment_policy: "manual".to_string(),
        };

        (signing_key, system)
    }

    fn create_signed_headers(
        hostname: &str,
        body: &[u8],
        signing_key: &ed25519_dalek::SigningKey,
    ) -> HeaderMap {
        let signature = signing_key.sign(body);
        let sig_b64 = general_purpose::STANDARD.encode(signature.to_bytes());

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-key-id"),
            hostname.parse().expect("valid hostname header"),
        );
        headers.insert(
            HeaderName::from_static("x-signature"),
            sig_b64.parse().expect("valid signature header"),
        );
        headers
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified isolated database and database creation privileges"]
    async fn managed_detail_counts_require_exact_current_enrollment_not_configuration_history(
        pool: PgPool,
    ) {
        async fn artifact(pool: &PgPool, flake: i32, tag: &str, cve: &str, score: f64) -> String {
            let commit: i32 = sqlx::query_scalar("INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES($1,$2,clock_timestamp()) RETURNING id")
                .bind(flake).bind(tag.repeat(40)).fetch_one(pool).await.unwrap();
            let path = format!("/nix/store/{}-detail-{tag}", tag.repeat(32));
            let derivation: i32 = sqlx::query_scalar("INSERT INTO derivations(commit_id,derivation_name,derivation_type,derivation_path,store_path,status_id,completed_at) VALUES($1,'detail-host','nixos',$2,$3,(SELECT id FROM derivation_statuses WHERE name='build-complete'),clock_timestamp()) RETURNING id")
                .bind(commit).bind(format!("{path}.drv")).bind(&path).fetch_one(pool).await.unwrap();
            sqlx::query(
                "INSERT INTO cves(id,description,cvss_v3_score) VALUES($1,'detail regression',$2)",
            )
            .bind(cve)
            .bind(score)
            .execute(pool)
            .await
            .unwrap();
            let package: i32 = sqlx::query_scalar("INSERT INTO derivations(commit_id,derivation_name,pname,version,derivation_type,derivation_path,status_id) VALUES($1,$2,$2,'1','package',$3,(SELECT id FROM derivation_statuses WHERE name='complete')) RETURNING id")
                .bind(commit).bind(format!("package-{tag}")).bind(format!("/nix/store/{}-package-{tag}.drv", tag.repeat(32))).fetch_one(pool).await.unwrap();
            let scan: Uuid = sqlx::query_scalar("INSERT INTO cve_scans(derivation_id,scanner_name,status) VALUES($1,'vulnix','in_progress') RETURNING id")
                .bind(derivation).fetch_one(pool).await.unwrap();
            sqlx::query("INSERT INTO scan_packages(scan_id,derivation_id) VALUES($1,$2)")
                .bind(scan)
                .bind(package)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO package_vulnerabilities(derivation_id,cve_id,is_whitelisted,detection_method) VALUES($1,$2,false,'vulnix')")
                .bind(package).bind(cve).execute(pool).await.unwrap();
            sqlx::query("INSERT INTO cve_scan_vulnerability_observations(scan_id,canonical_cve_id,canonical_package_name,observed_package_name,observed_package_version,observed_derivation_path,is_whitelisted,detection_method) VALUES($1,$2,$3,$3,'1',$4,false,'vulnix')")
                .bind(scan).bind(cve).bind(format!("package-{tag}")).bind(format!("/nix/store/{}-package-{tag}.drv", tag.repeat(32))).execute(pool).await.unwrap();
            sqlx::query("UPDATE cve_scans SET status='completed',completed_at=clock_timestamp(),evidence_schema_version=1 WHERE id=$1")
                .bind(scan).execute(pool).await.unwrap();
            path
        }
        async fn counts(pool: &PgPool, id: Uuid, expected: (i32, i32, i32, i32)) {
            let detail: (i32, i32, i32, i32) = sqlx::query_as("SELECT critical_cve_count,high_cve_count,medium_cve_count,low_cve_count FROM view_system_detail WHERE id=$1")
                .bind(id).fetch_one(pool).await.unwrap();
            let list: (i32, i32, i32, i32) = sqlx::query_as("SELECT critical_cve_count,high_cve_count,medium_cve_count,low_cve_count FROM view_system_list WHERE id=$1")
                .bind(id).fetch_one(pool).await.unwrap();
            assert_eq!(detail, expected);
            assert_eq!(list, expected);
        }
        let (key, verifying) = generate_keypair();
        let flake: i32 = sqlx::query_scalar("INSERT INTO flakes(name,repo_url) VALUES('detail-owned','https://example.invalid/owned') RETURNING id").fetch_one(&pool).await.unwrap();
        let other_flake: i32 = sqlx::query_scalar("INSERT INTO flakes(name,repo_url) VALUES('detail-foreign','https://example.invalid/foreign') RETURNING id").fetch_one(&pool).await.unwrap();
        let id: Uuid = sqlx::query_scalar("INSERT INTO systems(hostname,public_key,flake_id,derivation) VALUES('detail-host',$1,$2,'') RETURNING id")
            .bind(general_purpose::STANDARD.encode(verifying.to_bytes())).bind(flake).fetch_one(&pool).await.unwrap();
        let _historical = artifact(&pool, flake, "a", "CVE-2026-50801", 9.8).await;
        let current = artifact(&pool, flake, "b", "CVE-2026-50802", 7.5).await;
        let _foreign = artifact(&pool, other_flake, "c", "CVE-2026-50803", 1.0).await;
        let legacy: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM view_system_vulnerabilities WHERE hostname='detail-host'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            legacy > 0,
            "the legacy source must contain real configuration-only findings"
        );
        counts(&pool, id, (0, 0, 0, 0)).await;
        let state = CFState::new(
            pool.clone(),
            crate::config::ServerConfig::default(),
            Arc::new(crate::queue::QueueNotifier::new()),
            crate::server::jobs::BackgroundJobRegistry::new(),
        );
        let mut payload = SystemStateBuilder::new()
            .hostname("detail-host")
            .store_path(&current)
            .build();
        payload.generation = Some(1);
        payload.generation_matches_current_store_path = Some(true);
        let body = serde_json::to_vec(&payload).unwrap();
        let response = update(
            State(state.clone()),
            State(pool.clone()),
            create_signed_headers("detail-host", &body, &key),
            Bytes::from(body),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        counts(&pool, id, (0, 1, 0, 0)).await;
        sqlx::query("UPDATE systems SET management_mode='reference',public_key=NULL WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        counts(&pool, id, (0, 0, 0, 0)).await;
        let (fresh, verifying) = generate_keypair();
        sqlx::query("UPDATE systems SET management_mode='managed',public_key=$2 WHERE id=$1")
            .bind(id)
            .bind(general_purpose::STANDARD.encode(verifying.to_bytes()))
            .execute(&pool)
            .await
            .unwrap();
        counts(&pool, id, (0, 0, 0, 0)).await;
        let body = serde_json::to_vec(&payload).unwrap();
        let response = update(
            State(state),
            State(pool.clone()),
            create_signed_headers("detail-host", &body, &fresh),
            Bytes::from(body),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        counts(&pool, id, (0, 1, 0, 0)).await;
        sqlx::query("UPDATE systems SET is_active=false WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let detail: (String, Option<String>, i32) = sqlx::query_as("SELECT health_status,current_store_path,high_cve_count FROM view_system_detail WHERE id=$1")
            .bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!(
            detail,
            ("healthy".into(), Some(current), 0),
            "0320 inactive telemetry remains observable without granting Current CVE authority"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified isolated database and database creation privileges"]
    async fn signed_agent_cannot_poison_another_registration_on_either_endpoint(pool: PgPool) {
        let (key, verifying) = generate_keypair();
        let other_key = general_purpose::STANDARD.encode(generate_keypair().1.to_bytes());
        for (hostname, public_key) in [
            (
                "signer-a",
                general_purpose::STANDARD.encode(verifying.to_bytes()),
            ),
            ("victim-b", other_key),
            (
                "duplicate-a",
                general_purpose::STANDARD.encode(verifying.to_bytes()),
            ),
        ] {
            sqlx::query("INSERT INTO systems(hostname,public_key,derivation) VALUES($1,$2,'')")
                .bind(hostname)
                .bind(public_key)
                .execute(&pool)
                .await
                .unwrap();
        }
        let payload = SystemStateBuilder::new().hostname("victim-b").build();
        let body = serde_json::to_vec(&payload).unwrap();
        let headers = create_signed_headers("signer-a", &body, &key);
        let state = CFState::new(
            pool.clone(),
            crate::config::ServerConfig::default(),
            Arc::new(crate::queue::QueueNotifier::new()),
            crate::server::jobs::BackgroundJobRegistry::new(),
        );
        let legacy = update(
            State(state.clone()),
            State(pool.clone()),
            headers.clone(),
            Bytes::from(body.clone()),
        )
        .await
        .into_response();
        assert_eq!(legacy.status(), StatusCode::BAD_REQUEST);
        let heartbeat = crate::handlers::agent::heartbeat::log(
            State(state),
            State(pool.clone()),
            None,
            headers,
            Bytes::from(body),
        )
        .await;
        assert_eq!(heartbeat.status(), StatusCode::BAD_REQUEST);
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM system_states), (SELECT count(*) FROM agent_heartbeats),
                    (SELECT count(*) FROM systems WHERE is_active)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            counts,
            (0, 0, 3),
            "neither endpoint may write evidence or clean up duplicates before identity validation"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified isolated database and database creation privileges"]
    async fn authenticated_state_cannot_follow_renamed_hostname_to_new_registration(pool: PgPool) {
        let (key, verifying) = generate_keypair();
        let encoded = general_purpose::STANDARD.encode(verifying.to_bytes());
        sqlx::query("INSERT INTO systems(hostname,public_key,derivation) VALUES('old-name',$1,'')")
            .bind(&encoded)
            .execute(&pool)
            .await
            .unwrap();
        let verified = pool.lookup("old-name".into()).await.unwrap().unwrap();
        let payload = SystemStateBuilder::new().hostname("old-name").build();
        let body = serde_json::to_vec(&payload).unwrap();
        let headers = create_signed_headers("old-name", &body, &key);
        // Authentication has completed. A is renamed; C takes its old name.
        sqlx::query("UPDATE systems SET hostname='new-name' WHERE id=$1")
            .bind(verified.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO systems(hostname,public_key,derivation) VALUES('old-name',$1,'')")
            .bind(&encoded)
            .execute(&pool)
            .await
            .unwrap();
        let result = update_with_lookup_and_insert(
            headers,
            Bytes::from(body),
            &MockLookup::new(Some(verified.clone())),
            |payload, compatible, verified| {
                let pool = pool.clone();
                Box::pin(async move {
                    persist_system_state_inner(&pool, payload, compatible, Some(verified)).await
                })
            },
        )
        .await;
        assert_eq!(result, StatusCode::FORBIDDEN);
        let mut tx = pool.begin().await.unwrap();
        crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx)
            .await
            .unwrap();
        assert!(
            crate::queries::systems::deactivate_duplicate_active_systems_by_public_key(
                &mut tx, &verified
            )
            .await
            .is_err()
        );
        assert!(
            crate::queries::system_states::lock_agent_enrollment_tx(&mut tx, &verified)
                .await
                .unwrap()
                .is_none()
        );
        tx.commit().await.unwrap();
        let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM system_states), (SELECT count(*) FROM systems WHERE is_active)")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(counts, (0, 2));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified isolated database and database creation privileges"]
    async fn authenticated_state_waiting_across_reenrollment_is_rejected(pool: PgPool) {
        for mode in ["external", "reference"] {
            let hostname = format!("state-epoch-{mode}");
            let (old_key, verifying) = generate_keypair();
            sqlx::query("INSERT INTO systems(hostname,public_key,derivation,deployment_policy) VALUES($1,$2,'','manual')")
                .bind(&hostname).bind(general_purpose::STANDARD.encode(verifying.to_bytes()))
                .execute(&pool).await.unwrap();
            let verified = pool.lookup(hostname.clone()).await.unwrap().unwrap();
            let payload = SystemStateBuilder::new().hostname(&hostname).build();
            let body = serde_json::to_vec(&payload).unwrap();
            let headers = create_signed_headers(&hostname, &body, &old_key);
            let (authenticated, ready) = tokio::sync::oneshot::channel();
            let (resume, resumed) = tokio::sync::oneshot::channel();
            let authenticated = Arc::new(std::sync::Mutex::new(Some(authenticated)));
            let resumed = Arc::new(std::sync::Mutex::new(Some(resumed)));
            let request_pool = pool.clone();
            let stale = verified.clone();
            let request = tokio::spawn(async move {
                update_with_lookup_and_insert(
                    headers,
                    Bytes::from(body),
                    &MockLookup::new(Some(stale)),
                    move |payload, compatible, verified| {
                        let pool = request_pool.clone();
                        let signal = authenticated.lock().unwrap().take().unwrap();
                        let resumed = resumed.lock().unwrap().take().unwrap();
                        Box::pin(async move {
                            signal.send(()).unwrap();
                            resumed.await.unwrap();
                            persist_system_state_inner(&pool, payload, compatible, Some(verified))
                                .await
                        })
                    },
                )
                .await
            });
            ready.await.unwrap();
            sqlx::query("UPDATE systems SET management_mode=$2,public_key=NULL WHERE hostname=$1")
                .bind(&hostname)
                .bind(mode)
                .execute(&pool)
                .await
                .unwrap();
            // Even an operator reusing the same key cannot revive an already
            // authenticated request from the previous enrollment.
            let (fresh_key, verifying) = if mode == "reference" {
                (old_key.clone(), old_key.verifying_key())
            } else {
                generate_keypair()
            };
            sqlx::query(
                "UPDATE systems SET management_mode='managed',public_key=$2 WHERE hostname=$1",
            )
            .bind(&hostname)
            .bind(general_purpose::STANDARD.encode(verifying.to_bytes()))
            .execute(&pool)
            .await
            .unwrap();
            resume.send(()).unwrap();
            assert_eq!(request.await.unwrap(), StatusCode::FORBIDDEN);
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM system_states WHERE hostname=$1")
                    .bind(&hostname)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                count, 0,
                "pre-transition authentication must not write after re-enrollment"
            );

            // The shared heartbeat/state guard rejects the same stale identity.
            let mut tx = pool.begin().await.unwrap();
            crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx)
                .await
                .unwrap();
            assert_eq!(
                crate::queries::system_states::lock_agent_enrollment_tx(&mut tx, &verified)
                    .await
                    .unwrap(),
                None
            );
            tx.rollback().await.unwrap();

            let body = serde_json::to_vec(&payload).unwrap();
            let headers = create_signed_headers(&hostname, &body, &fresh_key);
            let result = update_with_lookup_and_insert(
                headers,
                Bytes::from(body),
                &pool,
                |payload, compatible, verified| {
                    let pool = pool.clone();
                    Box::pin(async move {
                        persist_system_state_inner(&pool, payload, compatible, Some(verified)).await
                    })
                },
            )
            .await;
            assert_eq!(result, StatusCode::OK);
            let current: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM view_agent_current_system_states WHERE hostname=$1",
            )
            .bind(&hostname)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(current, 1);
        }
    }

    #[tokio::test]
    async fn test_valid_state_update_request_returns_ok_and_calls_insert() {
        let (signing_key, system) = create_test_system("test-host");
        let state = SystemStateBuilder::new()
            .hostname("test-host")
            .change_reason("startup")
            .build();
        let body_bytes = serde_json::to_vec(&state).expect("serialize current state");
        let headers = create_signed_headers("test-host", &body_bytes, &signing_key);
        let body = Bytes::from(body_bytes);

        let lookup = MockLookup::new(Some(system));
        let insert_calls = Arc::new(AtomicUsize::new(0));
        let last_compat = Arc::new(Mutex::new(None));

        let status = update_with_lookup_and_insert(headers, body, &lookup, {
            let insert_calls = Arc::clone(&insert_calls);
            let last_compat = Arc::clone(&last_compat);
            move |_payload, version_compatible, _verified| {
                insert_calls.fetch_add(1, Ordering::SeqCst);
                let last_compat = Arc::clone(&last_compat);
                Box::pin(async move {
                    *last_compat.lock().await = Some(version_compatible);
                    Ok(())
                })
            }
        })
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(insert_calls.load(Ordering::SeqCst), 1);
        assert_eq!(*last_compat.lock().await, Some(true));
    }

    #[tokio::test]
    async fn test_invalid_payload_format_returns_bad_request() {
        let (signing_key, system) = create_test_system("test-host");
        let body = Bytes::from_static(b"not valid json");
        let headers = create_signed_headers("test-host", &body, &signing_key);

        let lookup = MockLookup::new(Some(system));
        let insert_calls = Arc::new(AtomicUsize::new(0));

        let status = update_with_lookup_and_insert(headers, body, &lookup, {
            let insert_calls = Arc::clone(&insert_calls);
            move |_payload, _version_compatible, _verified| {
                insert_calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move { Ok(()) })
            }
        })
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(insert_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_version_compatibility_current_vs_v1_status_codes() {
        let (signing_key, system) = create_test_system("test-host");
        let lookup = MockLookup::new(Some(system));

        let current = SystemStateBuilder::new()
            .hostname("test-host")
            .change_reason("startup")
            .build();
        let current_bytes = serde_json::to_vec(&current).expect("serialize current state");
        let current_headers = create_signed_headers("test-host", &current_bytes, &signing_key);
        let current_status = update_with_lookup_and_insert(
            current_headers,
            Bytes::from(current_bytes),
            &lookup,
            |_payload, _version_compatible, _verified| Box::pin(async move { Ok(()) }),
        )
        .await;

        let v1 = SystemStateV1 {
            id: None,
            hostname: "test-host".to_string(),
            context: "agent-startup".to_string(),
            timestamp: None,
            store_path: Some("/nix/store/legacy".to_string()),
            os: Some("24.11".to_string()),
            kernel: Some("6.6.0".to_string()),
            memory_gb: Some(8.0),
            uptime_secs: Some(3600),
            cpu_brand: Some("Old CPU".to_string()),
            cpu_cores: Some(2),
            board_serial: Some("LEGACY-SERIAL".to_string()),
            product_uuid: Some("legacy-uuid".to_string()),
            rootfs_uuid: Some("legacy-rootfs".to_string()),
            chassis_serial: Some("LEGACY-CHASSIS".to_string()),
            bios_version: Some("0.9.0".to_string()),
            cpu_microcode: None,
            network_interfaces: None,
            primary_mac_address: Some("00:11:22:33:44:55".to_string()),
            primary_ip_address: Some("10.0.0.1".to_string()),
            gateway_ip: Some("10.0.0.254".to_string()),
            selinux_status: None,
            tpm_present: Some(false),
            secure_boot_enabled: Some(false),
            fips_mode: Some(false),
            agent_version: Some("0.1.0".to_string()),
            agent_build_hash: None,
            nixos_version: Some("24.11".to_string()),
        };
        let v1_bytes = serde_json::to_vec(&v1).expect("serialize v1 state");
        let v1_headers = create_signed_headers("test-host", &v1_bytes, &signing_key);
        let v1_status = update_with_lookup_and_insert(
            v1_headers,
            Bytes::from(v1_bytes),
            &lookup,
            |_payload, _version_compatible, _verified| Box::pin(async move { Ok(()) }),
        )
        .await;

        assert_eq!(current_status, StatusCode::OK);
        assert_eq!(v1_status, StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn test_insert_failure_returns_internal_server_error() {
        let (signing_key, system) = create_test_system("test-host");
        let state = SystemStateBuilder::new()
            .hostname("test-host")
            .change_reason("startup")
            .build();
        let body_bytes = serde_json::to_vec(&state).expect("serialize current state");
        let headers = create_signed_headers("test-host", &body_bytes, &signing_key);

        let lookup = MockLookup::new(Some(system));
        let status = update_with_lookup_and_insert(
            headers,
            Bytes::from(body_bytes),
            &lookup,
            |_payload, _version_compatible, _verified| {
                Box::pin(async move { Err(anyhow::anyhow!("simulated insert failure")) })
            },
        )
        .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
