//! Migration-backed receiver authorization tests with independent sessions.
//!
//! Calls handlers directly and reuses the parent publication/session fixtures.
//! Each test owns its SQLx database; no process-global auth or mock state changes.

use super::*;
use crate::api::models::{ComplianceBundleSystemsResponse, CreateAssignmentRequest};
use crate::handlers::agent_request::CFState;
use crate::handlers::api::systems::get_system;
use crate::queries::compliance::totals_for_rollups;
use serde_json::{Value, json};

// Observe PostgreSQL's actual lock wait rather than treating a slow task as
// proof that a competing write is blocked by this read guard.
async fn wait_for_guard_blocker(pool: &PgPool, writer_pid: i32, guard_pid: i32) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blockers: Vec<i32> = sqlx::query_scalar("SELECT pg_blocking_pids($1)")
                .bind(writer_pid)
                .fetch_one(pool)
                .await
                .unwrap();
            if blockers.contains(&guard_pid) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("writer must wait on the receiver guard within five seconds");
}

async fn scope_writers(
    pool: &PgPool,
    user: Uuid,
    host: Uuid,
    environment: Uuid,
) -> [(i32, tokio::task::JoinHandle<u64>); 2] {
    let mut revoke = pool.acquire().await.unwrap();
    let revoke_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *revoke)
        .await
        .unwrap();
    let mut move_host = pool.acquire().await.unwrap();
    let move_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *move_host)
        .await
        .unwrap();
    let revoke_task = tokio::spawn(async move {
        sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1")
            .bind(user)
            .execute(&mut *revoke)
            .await
            .unwrap()
            .rows_affected()
    });
    let move_task = tokio::spawn(async move {
        sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
            .bind(host)
            .bind(environment)
            .execute(&mut *move_host)
            .await
            .unwrap()
            .rows_affected()
    });
    [(revoke_pid, revoke_task), (move_pid, move_task)]
}

async fn finish_scope_writers(writers: [(i32, tokio::task::JoinHandle<u64>); 2]) {
    for (_, writer) in writers {
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(5), writer)
                .await
                .expect("writer must finish after guard release")
                .unwrap(),
            1,
        );
    }
}

struct Fixture {
    users: [Uuid; 2],
    headers: [HeaderMap; 2],
    admin: HeaderMap,
    environments: [Uuid; 2],
    hosts: [Uuid; 2],
    names: [String; 2],
    bundle: Uuid,
    version: Uuid,
}

fn session_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "cookie",
        format!("{SESSION_COOKIE_NAME}={token}").parse().unwrap(),
    );
    headers
}

async fn fixture(pool: &PgPool) -> Fixture {
    let (admin_id, token) = session_token_for_role(pool, AuthRole::Admin).await;
    let admin = session_headers(&token);
    let (viewer, viewer_token) = session_token_for_role(pool, AuthRole::Viewer).await;
    let (operator, operator_token) = session_token_for_role(pool, AuthRole::Operator).await;
    let (policy, policy_version, _) =
        make_draft_policy(pool, &format!("receiver-policy-{}", Uuid::new_v4())).await;
    let (extra, extra_version, _) =
        make_draft_policy(pool, &format!("hidden-only-policy-{}", Uuid::new_v4())).await;
    sqlx::query("UPDATE deployment_policies SET enabled=true WHERE id=ANY($1)")
        .bind(vec![policy, extra])
        .execute(pool)
        .await
        .unwrap();
    db_trust_policy_version(pool, policy_version, admin_id).await;
    db_publish_policy_version(pool, policy, policy_version).await;
    db_trust_policy_version(pool, extra_version, admin_id).await;
    db_publish_policy_version(pool, extra, extra_version).await;
    let (bundle, version, _) = make_draft_bundle(
        pool,
        &format!("global-receiver-bundle-{}", Uuid::new_v4()),
        &[policy_version],
    )
    .await;
    db_publish_bundle_version(pool, version).await;
    let flake: i32 = sqlx::query_scalar(
        "INSERT INTO flakes(name,repo_url,branch) VALUES($1,$2,'main') RETURNING id",
    )
    .bind(format!("receiver-{}", Uuid::new_v4()))
    .bind(format!("https://example.invalid/{}.git", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .unwrap();
    let mut f = Fixture {
        users: [viewer, operator],
        headers: [
            session_headers(&viewer_token),
            session_headers(&operator_token),
        ],
        admin,
        environments: [Uuid::nil(); 2],
        hosts: [Uuid::nil(); 2],
        names: std::array::from_fn(|i| format!("receiver-host-{i}-{}", Uuid::new_v4())),
        bundle,
        version,
    };
    for i in 0..2 {
        f.environments[i] =
            sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
                .bind(format!("r-env-{i}-{}", Uuid::new_v4()))
                .fetch_one(pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
        )
        .bind(f.users[i])
        .bind(f.environments[i])
        .execute(pool)
        .await
        .unwrap();
        f.hosts[i] = sqlx::query_scalar(
            "INSERT INTO systems(hostname,public_key,derivation,environment_id,flake_id,system_configuration_name)
             VALUES($1,'receiver-test-key','',$2,$3,'shared-configuration') RETURNING id",
        )
        .bind(&f.names[i])
        .bind(f.environments[i])
        .bind(flake)
        .fetch_one(pool)
        .await
        .unwrap();
        // The first host's system assignment survives environment moves.
        // Applicability must not grant access through the old environment.
        // The other host uses an environment assignment, so its deletion does
        // not require deletion of immutable system-assignment history.
        persist_assignment(
            pool,
            admin_id,
            &CreateAssignmentRequest {
                bundle_version_id: version,
                scope_type: if i == 0 { "system" } else { "environment" }.into(),
                scope_id: if i == 0 {
                    f.hosts[i]
                } else {
                    f.environments[i]
                },
                enforcement_mode: Some(if i == 0 { "enforce" } else { "report_only" }.into()),
                exclusions: None,
                additions: (i == 1).then_some(vec![extra_version]),
                value_overrides: None,
                reason: Some(format!("host-{i}-private-assignment-detail")),
            },
            None,
            None,
        )
        .await
        .unwrap();
    }
    f
}

async fn body(response: axum::response::Response) -> (StatusCode, Vec<u8>) {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

async fn system_read(
    pool: &PgPool,
    headers: &HeaderMap,
    host: Uuid,
    compliance: bool,
) -> (StatusCode, Vec<u8>) {
    let response = if compliance {
        get_system_compliance_bundles(State(pool.clone()), headers.clone(), Path(host))
            .await
            .into_response()
    } else {
        let state = CFState::new(
            pool.clone(),
            crate::config::ServerConfig::default(),
            std::sync::Arc::new(crate::queue::QueueNotifier::new()),
            crate::server::jobs::BackgroundJobRegistry::new(),
        );
        get_system(
            State(state),
            State(pool.clone()),
            headers.clone(),
            Path(host),
        )
        .await
        .into_response()
    };
    body(response).await
}

async fn bundle_read(
    pool: &PgPool,
    headers: &HeaderMap,
    bundle: Uuid,
    version_id: Option<Uuid>,
) -> (StatusCode, Vec<u8>) {
    body(
        get_compliance_bundle_systems(
            State(pool.clone()),
            headers.clone(),
            Path(bundle),
            Query(BundleVersionQuery { version_id }),
        )
        .await
        .into_response(),
    )
    .await
}

async fn bundle_response(
    pool: &PgPool,
    headers: &HeaderMap,
    f: &Fixture,
    version: Option<Uuid>,
) -> ComplianceBundleSystemsResponse {
    let (status, bytes) = bundle_read(pool, headers, f.bundle, version).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}

async fn assert_hidden(pool: &PgPool, headers: &HeaderMap, host: Uuid) {
    for compliance in [false, true] {
        let missing = system_read(pool, headers, Uuid::new_v4(), compliance).await;
        assert_eq!(missing.0, StatusCode::NOT_FOUND);
        assert_eq!(system_read(pool, headers, host, compliance).await, missing);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn system_receivers_hide_out_of_scope_hosts_with_identical_404(pool: PgPool) {
    let f = fixture(&pool).await;
    for i in 0..2 {
        assert_hidden(&pool, &f.headers[i], f.hosts[1 - i]).await;
        for compliance in [false, true] {
            let (status, bytes) = system_read(&pool, &f.headers[i], f.hosts[i], compliance).await;
            assert_eq!(status, StatusCode::OK);
            assert!(!String::from_utf8_lossy(&bytes).contains(&f.names[1 - i]));
            for host in f.hosts {
                assert_eq!(
                    system_read(&pool, &f.admin, host, compliance).await.0,
                    StatusCode::OK
                );
            }
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn bundle_systems_scope_host_details_and_every_total_before_rollup(pool: PgPool) {
    let f = fixture(&pool).await;
    for version in [None, Some(f.version)] {
        let fleet = bundle_response(&pool, &f.admin, &f, version).await;
        assert_eq!(fleet.systems.len(), 2);
        assert_eq!(fleet.totals.system_count, 2);
        assert!(fleet.totals.total_controls > 0);
        for i in 0..2 {
            let visible = bundle_response(&pool, &f.headers[i], &f, version).await;
            let mut expected = fleet.clone();
            expected.systems.retain(|row| row.system_id == f.hosts[i]);
            assert_eq!(expected.systems.len(), 1);
            expected.totals = totals_for_rollups(&expected.systems);
            assert_eq!(
                serde_json::to_value(&visible).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
            let wire = serde_json::to_string(&visible).unwrap();
            for secret in [
                f.names[1 - i].clone(),
                f.hosts[1 - i].to_string(),
                format!("host-{}-private-assignment-detail", 1 - i),
            ] {
                assert!(!wire.contains(&secret), "hidden host data: {secret}");
            }
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn receiver_scope_rechecks_revocations_moves_unscoped_and_deleted_hosts(pool: PgPool) {
    let f = fixture(&pool).await;
    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1")
        .bind(f.users[0])
        .execute(&pool)
        .await
        .unwrap();
    assert_hidden(&pool, &f.headers[0], f.hosts[0]).await;
    for version in [None, Some(f.version)] {
        let empty = bundle_response(&pool, &f.headers[0], &f, version).await;
        assert!(empty.systems.is_empty());
        assert_eq!(
            serde_json::to_value(empty.totals).unwrap(),
            serde_json::to_value(totals_for_rollups(&[])).unwrap()
        );
    }
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(f.users[0])
        .bind(f.environments[0])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(f.hosts[0])
        .bind(f.environments[1])
        .execute(&pool)
        .await
        .unwrap();
    assert_hidden(&pool, &f.headers[0], f.hosts[0]).await;
    assert!(
        bundle_response(&pool, &f.headers[0], &f, None)
            .await
            .systems
            .is_empty()
    );
    assert_eq!(
        bundle_response(&pool, &f.headers[1], &f, Some(f.version))
            .await
            .systems
            .len(),
        2
    );
    sqlx::query("UPDATE systems SET environment_id=NULL WHERE id=$1")
        .bind(f.hosts[0])
        .execute(&pool)
        .await
        .unwrap();
    for headers in &f.headers {
        assert_hidden(&pool, headers, f.hosts[0]).await;
    }
    for compliance in [false, true] {
        assert_eq!(
            system_read(&pool, &f.admin, f.hosts[0], compliance).await.0,
            StatusCode::OK
        );
    }
    assert_eq!(
        bundle_response(&pool, &f.admin, &f, None)
            .await
            .systems
            .len(),
        2
    );
    sqlx::query("DELETE FROM systems WHERE id=$1")
        .bind(f.hosts[1])
        .execute(&pool)
        .await
        .unwrap();
    assert_hidden(&pool, &f.headers[1], f.hosts[1]).await;
    assert_hidden(&pool, &f.admin, f.hosts[1]).await;
    for version in [None, Some(f.version)] {
        let empty = bundle_response(&pool, &f.headers[1], &f, version).await;
        assert!(empty.systems.is_empty());
        assert_eq!(empty.totals.system_count, 0);
        assert_eq!(
            bundle_response(&pool, &f.admin, &f, version)
                .await
                .systems
                .len(),
            1
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn receiver_scope_preserves_empty_revisions_and_legacy_system_rollups(pool: PgPool) {
    let f = fixture(&pool).await;
    sqlx::query("UPDATE compliance_bundle_assignments SET active=false WHERE bundle_id=$1")
        .bind(f.bundle)
        .execute(&pool)
        .await
        .unwrap();
    // Keep an enabled, trusted direct policy after removing bundle
    // applicability. The receiver must preserve the legacy-only rollup.
    sqlx::query(
        "INSERT INTO system_policies(system_id,policy_id)
         SELECT $1,pv.policy_id FROM compliance_bundle_version_policies member
         JOIN deployment_policy_versions pv ON pv.id=member.policy_version_id
         WHERE member.bundle_version_id=$2",
    )
    .bind(f.hosts[0])
    .bind(f.version)
    .execute(&pool)
    .await
    .unwrap();
    for version in [None, Some(f.version)] {
        let empty = bundle_response(&pool, &f.headers[0], &f, version).await;
        assert!(empty.systems.is_empty());
        assert_eq!(empty.bundle_version_id, Some(f.version));
        assert_eq!(empty.totals.system_count, 0);
    }
    let (status, bytes) = system_read(&pool, &f.headers[0], f.hosts[0], true).await;
    assert_eq!(status, StatusCode::OK);
    let legacy: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(legacy["bundles"], json!([]));
    assert_eq!(legacy["direct_rollup"]["system_id"], json!(f.hosts[0]));
    assert_eq!(legacy["overall_rollup"]["system_id"], json!(f.hosts[0]));
    assert_eq!(legacy["direct_rollup"]["total"], json!(1));
    assert_eq!(legacy["overall_rollup"]["total"], json!(1));
    assert_hidden(&pool, &f.headers[1], f.hosts[0]).await;
    let (other_bundle, other_version, _) =
        make_draft_bundle(&pool, &format!("empty-receiver-{}", Uuid::new_v4()), &[]).await;
    let empty = bundle_read(&pool, &f.headers[0], other_bundle, Some(other_version)).await;
    assert_eq!(empty.0, StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<Value>(&empty.1).unwrap()["systems"],
        json!([])
    );
    let mismatch = bundle_read(&pool, &f.headers[0], f.bundle, Some(other_version)).await;
    let missing = bundle_read(&pool, &f.headers[0], Uuid::new_v4(), None).await;
    assert_eq!(mismatch.0, StatusCode::NOT_FOUND);
    assert_eq!(mismatch, missing);
    let no_default: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundles(name,framework,layer,owner) VALUES($1,'NIST','nixos','test-owner') RETURNING id",
    ).bind(format!("no-default-receiver-{}", Uuid::new_v4())).fetch_one(&pool).await.unwrap();
    // Bundle insertion creates its initial draft version. Preserve that actual
    // identity in an empty response rather than expecting a fabricated null.
    let initial_version: Uuid = sqlx::query_scalar(
        "SELECT id FROM compliance_bundle_versions WHERE bundle_id=$1 ORDER BY created_at DESC LIMIT 1",
    ).bind(no_default).fetch_one(&pool).await.unwrap();
    let no_default = bundle_read(&pool, &f.headers[0], no_default, None).await;
    assert_eq!(no_default.0, StatusCode::OK);
    let no_default: Value = serde_json::from_slice(&no_default.1).unwrap();
    assert_eq!(no_default["bundle_version_id"], json!(initial_version));
    assert_eq!(no_default["systems"], json!([]));
    assert_eq!(no_default["totals"]["system_count"], json!(0));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn single_receiver_guard_blocks_revocation_and_move_until_materialized(pool: PgPool) {
    let f = fixture(&pool).await;
    let (mut guard, ids) = lock_receiver_read_scope(
        &pool,
        f.users[0],
        crate::auth::models::Role::Viewer,
        Some(f.hosts[0]),
    )
    .await
    .unwrap();
    assert_eq!(ids, vec![f.hosts[0]]);
    let guard_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *guard)
        .await
        .unwrap();
    let writers = scope_writers(&pool, f.users[0], f.hosts[0], f.environments[1]).await;
    for (pid, _) in &writers {
        wait_for_guard_blocker(&pool, *pid, guard_pid).await;
    }
    // Exercise the same separate-pool detail and rollup loads as the two
    // receivers while the competing scope writes are confirmed blocked.
    let detail = crate::queries::systems::get_system_detail_by_id(&pool, f.hosts[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.hostname, f.names[0]);
    let rollups = list_system_bundles(&pool, f.hosts[0])
        .await
        .unwrap()
        .unwrap();
    let response = SystemComplianceBundlesResponse {
        system_id: f.hosts[0],
        bundles: rollups
            .bundles
            .into_iter()
            .map(|(bundle, rollup)| {
                let assigned = rollups.assignment_versions.get(&bundle.id);
                SystemComplianceBundle {
                    bundle,
                    rollup,
                    assigned_bundle_version_id: assigned.map(|version| version.id),
                    assignment_mode: assigned.map(|version| version.enforcement_mode.clone()),
                }
            })
            .collect(),
        direct_rollup: rollups.direct_rollup,
        overall_rollup: rollups.overall_rollup,
    };
    let materialized = body((StatusCode::OK, Json(response)).into_response()).await;
    assert_eq!(materialized.0, StatusCode::OK);
    for (pid, task) in &writers {
        assert!(!task.is_finished());
        wait_for_guard_blocker(&pool, *pid, guard_pid).await;
    }
    guard.commit().await.unwrap();
    finish_scope_writers(writers).await;
    assert_hidden(&pool, &f.headers[0], f.hosts[0]).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn bundle_receiver_guard_bounds_candidates_and_blocks_scope_writes(pool: PgPool) {
    let f = fixture(&pool).await;
    let (admin, _) = authenticated_user_roles(&pool, &f.admin).await.unwrap();
    persist_assignment(
        &pool,
        admin,
        &CreateAssignmentRequest {
            bundle_version_id: f.version,
            scope_type: "environment".into(),
            scope_id: f.environments[0],
            enforcement_mode: Some("enforce".into()),
            exclusions: None,
            additions: None,
            value_overrides: None,
            reason: None,
        },
        None,
        None,
    )
    .await
    .unwrap();
    let (mut guard, ids) =
        lock_receiver_read_scope(&pool, f.users[0], crate::auth::models::Role::Viewer, None)
            .await
            .unwrap();
    assert_eq!(ids, vec![f.hosts[0]]);
    let guard_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *guard)
        .await
        .unwrap();
    let before = list_bundle_systems_for_locked_ids(&pool, f.bundle, None, &ids)
        .await
        .unwrap()
        .unwrap();
    // Neither a previously hidden host moving into scope nor a new host is
    // locked by the original guard. Both become applicable via env assignment.
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(f.hosts[1])
        .bind(f.environments[0])
        .execute(&pool)
        .await
        .unwrap();
    let late_host: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,public_key,derivation,environment_id,flake_id,system_configuration_name)
         SELECT $2,'receiver-test-key','',environment_id,flake_id,system_configuration_name
         FROM systems WHERE id=$1 RETURNING id",
    ).bind(f.hosts[0]).bind(format!("late-receiver-{}", Uuid::new_v4()))
        .fetch_one(&pool).await.unwrap();
    let fresh = list_bundle_systems_for_user(&pool, f.bundle, None, Some(f.users[0]))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fresh.systems.len(), 3);
    assert!(fresh.systems.iter().any(|row| row.system_id == late_host));
    let writers = scope_writers(&pool, f.users[0], f.hosts[0], f.environments[1]).await;
    for (pid, _) in &writers {
        wait_for_guard_blocker(&pool, *pid, guard_pid).await;
    }
    for version in [None, Some(f.version)] {
        let bounded = list_bundle_systems_for_locked_ids(&pool, f.bundle, version, &ids)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&bounded).unwrap(),
            serde_json::to_value(&before).unwrap()
        );
        assert_eq!(bounded.totals.system_count, 1);
        let materialized = body((StatusCode::OK, Json(bounded)).into_response()).await;
        assert_eq!(materialized.0, StatusCode::OK);
        for (pid, task) in &writers {
            assert!(!task.is_finished());
            wait_for_guard_blocker(&pool, *pid, guard_pid).await;
        }
    }
    guard.commit().await.unwrap();
    finish_scope_writers(writers).await;
    assert_hidden(&pool, &f.headers[0], f.hosts[0]).await;
    assert!(
        bundle_response(&pool, &f.headers[0], &f, None)
            .await
            .systems
            .is_empty()
    );
}
