//! Isolated PostgreSQL test for TASK-453 AC #10 (fixed in TASK-525.1): a
//! deployment whose target is the store path the host already runs completes
//! on the next unchanged agent report instead of staying `picked_up`.
//! Runs in the isolated CI database path (`packages/devScripts/tests/database-regressions.sh`).

use crystal_forge::models::system_states::SystemState;
use crystal_forge::queries::system_events::{
    lock_observed_system_state_by_hostname_tx, record_report_events_tx,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const PATH: &str = "/nix/store/already-current-nixos-system";

async fn host(pool: &PgPool) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,public_key,derivation) VALUES('already-current','test-key','test-derivation') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("system");
    sqlx::query(
        "INSERT INTO system_states(hostname,store_path,change_reason,timestamp) \
         VALUES('already-current',$1,'startup',now() - interval '10 minutes')",
    )
    .bind(PATH)
    .execute(pool)
    .await
    .expect("state");
    id
}

async fn pending(pool: &PgPool, system: Uuid, target: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO pending_system_deployments(system_id,target_store_path,status,source,issued_at,expires_at,delivered_at) \
         VALUES($1,$2,'pending','auto_latest',now() - interval '5 minutes',now() + interval '2 hours',now() - interval '4 minutes') \
         RETURNING id",
    )
    .bind(system)
    .bind(target)
    .fetch_one(pool)
    .await
    .expect("pending deployment")
}

async fn report_unchanged(pool: &PgPool) {
    let payload: SystemState = serde_json::from_value(json!({
        "hostname": "already-current",
        "change_reason": "heartbeat",
        "store_path": PATH,
        "timestamp": chrono::Utc::now(),
    }))
    .expect("payload");
    let mut tx = pool.begin().await.expect("tx");
    let previous = lock_observed_system_state_by_hostname_tx(&mut tx, "already-current")
        .await
        .expect("lock");
    record_report_events_tx(&mut tx, previous.as_ref(), &payload, None, None)
        .await
        .expect("record");
    tx.commit().await.expect("commit");
}

async fn status(pool: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM pending_system_deployments WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("status")
}

#[sqlx::test(migrations = "./migrations")]
async fn already_current_target_completes_on_unchanged_report(pool: PgPool) {
    let system = host(&pool).await;
    let current = pending(&pool, system, PATH).await;

    report_unchanged(&pool).await;

    assert_eq!(status(&pool, current).await, "succeeded");
    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM system_events WHERE deployment_id=$1 \
         AND event_type='cf_deployment_succeeded' AND metadata->>'already_current'='true'",
    )
    .bind(current)
    .fetch_one(&pool)
    .await
    .expect("events");
    assert_eq!(events, 1);

    // A second unchanged report does not duplicate the event.
    report_unchanged(&pool).await;
    let again: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM system_events WHERE deployment_id=$1 AND event_type='cf_deployment_succeeded'",
    )
    .bind(current)
    .fetch_one(&pool)
    .await
    .expect("events");
    assert_eq!(again, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn different_target_stays_pending_on_unchanged_report(pool: PgPool) {
    let system = host(&pool).await;
    let other = pending(&pool, system, "/nix/store/other-target-nixos-system").await;

    report_unchanged(&pool).await;

    assert_eq!(status(&pool, other).await, "pending");
}
