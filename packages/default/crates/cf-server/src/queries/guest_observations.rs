//! Persists signed parent unit evidence without granting guest runtime authority.
//! Metadata writers and heartbeat ingestion share the snapshot writer lock.
//! Observations never mutate system state, scan evidence, or deployment authority.

use anyhow::Result;
use cf_protocol::agent::{GuestObservation, GuestObservationTarget};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Retains one immutable parent report with separate collection and receipt times.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GuestObservationRow {
    /// Identifies this evidence row, not a system-state generation.
    pub id: i64,
    /// Identifies the external guest resource.
    pub guest_id: Uuid,
    /// Identifies the authenticated reporting parent at receipt time.
    pub parent_host_id: Uuid,
    /// Identifies the shared environment at receipt time.
    pub environment_id: Uuid,
    /// Retains the guest instance identity at receipt time.
    pub guest_identity: String,
    /// Retains the exact MicroVM unit inspected by the parent.
    pub unit: String,
    /// Contains ActiveState, or None when local collection was unavailable.
    pub unit_active_state: Option<String>,
    /// Contains a proven runner store object, never a guest current-system closure.
    pub runner_store_path: Option<String>,
    /// Records the parent's collection time, which may be stale.
    pub observed_at: DateTime<Utc>,
    /// Records server receipt time; does not prove guest freshness.
    pub received_at: DateTime<Utc>,
}

fn valid_report(report: &GuestObservation, now: DateTime<Utc>) -> bool {
    report.target.is_valid()
        && report.observed_at <= now
        && report.unit_active_state.as_ref().is_none_or(|state| {
            !state.is_empty()
                && state.len() <= 64
                && state.bytes().all(|c| c.is_ascii_lowercase() || c == b'-')
        })
        && report.runner_store_path.as_ref().is_none_or(|path| {
            // Only a store object root is evidence. No executable arguments,
            // relative paths, or guest-closure interpretation is accepted.
            let Some(object) = path.strip_prefix("/nix/store/") else {
                return false;
            };
            let Some((hash, name)) = object.split_once('-') else {
                return false;
            };
            hash.len() == 32
                && hash
                    .bytes()
                    .all(|c| b"0123456789abcdfghijklmnpqrsvwxyz".contains(&c))
                && !name.is_empty()
                && name.len() <= 211
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"+-._?=".contains(&c))
        })
}

/// Validates and appends reports, returning the next pull's explicit target list.
///
/// The caller MUST hold the snapshot writer lock and acquire POA&M locks before
/// calling. System row locks remain held through the heartbeat commit. The parent
/// key is rechecked under lock so a request waiting across rotation cannot report.
/// A None result means the parent identity is invalid and MUST roll back the
/// heartbeat. Invalid guest reports discard the whole observation batch, but
/// return the current authorized targets. Otherwise a cached target removed by
/// an association change would prevent the pull-only parent from recovering.
/// Missing reports append nothing and preserve older-agent compatibility.
///
/// # Errors
/// Returns database errors. Invalid parent credentials return None. Invalid guest
/// identities, associations, timestamps or duplicates append no observations.
pub async fn ingest_tx(
    tx: &mut Transaction<'_, Postgres>,
    parent_id: Uuid,
    verified_public_key: &str,
    reports: &[GuestObservation],
) -> Result<Option<Vec<GuestObservationTarget>>> {
    // CONCURRENCY: This function does not take the snapshot-writer lock. Row locks
    // serialize it: the parent row is locked FOR UPDATE here, and a guest edit
    // locks the guest row and then the parent row FOR SHARE, so a mode change,
    // re-association, key rotation or deactivation either waits for this batch or
    // is seen by it. The conditions below are re-read under that lock.
    let parent: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT environment_id FROM systems WHERE id=$1 AND management_mode='managed'
         AND is_active AND public_key=$2 FOR UPDATE",
    )
    .bind(parent_id)
    .bind(verified_public_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((environment,)) = parent else {
        return Ok(None);
    };
    let guests: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, hostname FROM systems WHERE parent_host_id=$1
          AND management_mode='external' AND is_active
          AND environment_id=$2 ORDER BY id FOR SHARE",
    )
    .bind(parent_id)
    .bind(environment)
    .fetch_all(&mut **tx)
    .await?;
    let targets: Vec<_> = guests
        .into_iter()
        .map(|(guest_id, guest_identity)| {
            let unit = format!("microvm@{guest_identity}.service");
            GuestObservationTarget {
                guest_id,
                guest_identity,
                unit,
            }
        })
        .filter(GuestObservationTarget::is_valid)
        .collect();
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let mut seen = std::collections::BTreeSet::new();
    if reports.iter().any(|report| {
        !valid_report(report, now)
            || !targets.contains(&report.target)
            || !seen.insert(report.target.guest_id)
    }) {
        tracing::warn!(%parent_id, "Discarded invalid guest observation batch; returning current authorized targets");
        return Ok(Some(targets));
    }
    for report in reports {
        sqlx::query(
            "INSERT INTO guest_observations
             (guest_id, parent_host_id, environment_id, guest_identity, unit,
              unit_active_state, runner_store_path, observed_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(report.target.guest_id)
        .bind(parent_id)
        .bind(environment)
        .bind(&report.target.guest_identity)
        .bind(&report.target.unit)
        .bind(&report.unit_active_state)
        .bind(&report.runner_store_path)
        .bind(report.observed_at)
        .execute(&mut **tx)
        .await?;
    }
    Ok(Some(targets))
}

/// Returns the latest evidence for the guest's current external association.
///
/// Callers MUST enforce existing viewer and environment authorization and pass
/// the authorized environment. A concurrent environment move returns None.
/// Reference, inactive or detached resources return None; history is retained.
/// Parent revocation, identity changes and environment moves suppress old semantics.
/// No refresh or parent contact occurs, and no configuration match is inferred.
///
/// # Errors
/// Returns database errors.
pub async fn latest(
    pool: &PgPool,
    guest_id: Uuid,
    authorized_environment: Option<Uuid>,
) -> Result<Option<GuestObservationRow>> {
    Ok(sqlx::query_as(
        "SELECT observation.* FROM guest_observations observation
         JOIN systems guest ON guest.id=observation.guest_id
         JOIN systems parent ON parent.id=guest.parent_host_id
          WHERE guest.id=$1 AND guest.management_mode='external' AND guest.is_active
           AND guest.environment_id IS NOT DISTINCT FROM $2
           AND parent.management_mode='managed' AND parent.is_active
           AND parent.environment_id=guest.environment_id
           AND observation.parent_host_id=parent.id
           AND observation.environment_id=guest.environment_id
           AND observation.guest_identity=guest.hostname
         ORDER BY observation.observed_at DESC, observation.id DESC LIMIT 1",
    )
    .bind(guest_id)
    .bind(authorized_environment)
    .fetch_optional(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> GuestObservation {
        GuestObservation {
            target: GuestObservationTarget {
                guest_id: Uuid::new_v4(),
                guest_identity: "guest".into(),
                unit: "microvm@guest.service".into(),
            },
            unit_active_state: Some("active".into()),
            runner_store_path: None,
            observed_at: Utc::now(),
        }
    }

    #[test]
    fn guest_observation_validation_rejects_future_and_fabricated_paths() {
        let mut report = report();
        let now = report.observed_at;
        assert!(valid_report(&report, now));
        report.observed_at = now + chrono::Duration::seconds(1);
        assert!(!valid_report(&report, now));
        report.observed_at = now;
        for path in [
            "/run/current-system",
            "/nix/store/arbitrary-argument",
            "/nix/store/00000000000000000000000000000000-runner/bin/run",
        ] {
            report.runner_store_path = Some(path.into());
            assert!(!valid_report(&report, now));
        }
        report.runner_store_path =
            Some("/nix/store/00000000000000000000000000000000-runner".into());
        assert!(valid_report(&report, now));
        report.unit_active_state = Some("active\nExecStart=evil".into());
        assert!(!valid_report(&report, now));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified isolated database and database creation privileges"]
    async fn guest_observations_require_current_parent_environment_and_identity(pool: PgPool) {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        use ed25519_dalek::Signer;
        let (signing_key, verifying_key) = crate::test_utils::crypto::generate_keypair();
        let public_key = STANDARD.encode(verifying_key.to_bytes());
        let env: Uuid = sqlx::query_scalar(
            "INSERT INTO environments(name) VALUES ('guest-observation') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let other_env: Uuid = sqlx::query_scalar(
            "INSERT INTO environments(name) VALUES ('guest-observation-other') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let parent: Uuid = sqlx::query_scalar("INSERT INTO systems(hostname,public_key,environment_id,derivation) VALUES ('parent',$2,$1,'') RETURNING id").bind(env).bind(&public_key).fetch_one(&pool).await.unwrap();
        let guest: Uuid = sqlx::query_scalar("INSERT INTO systems(hostname,management_mode,parent_host_id,environment_id,derivation) VALUES ('guest','external',$1,$2,'') RETURNING id").bind(parent).bind(env).fetch_one(&pool).await.unwrap();
        let mut report = report();
        report.target.guest_id = guest;
        let body = axum::body::Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "hostname":"parent", "change_reason":"heartbeat", "guest_observations":[report.clone()]
            }))
            .unwrap(),
        );
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-key-id", "parent".parse().unwrap());
        headers.insert(
            "x-signature",
            STANDARD
                .encode(signing_key.sign(&body).to_bytes())
                .parse()
                .unwrap(),
        );
        let authenticated =
            crate::handlers::agent_request::authenticate_agent_request(&headers, body, &pool)
                .await
                .unwrap();
        assert_eq!(authenticated.system.id, parent);
        async fn ingest(
            pool: &PgPool,
            parent: Uuid,
            key: &str,
            reports: &[GuestObservation],
        ) -> bool {
            let mut tx = pool.begin().await.unwrap();
            crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx)
                .await
                .unwrap();
            let accepted = ingest_tx(&mut tx, parent, key, reports)
                .await
                .unwrap()
                .is_some();
            if accepted {
                tx.commit().await.unwrap();
            }
            accepted
        }
        assert!(!ingest(&pool, parent, "wrong-key", &[report.clone()]).await);
        let mut unknown = report.clone();
        unknown.target.guest_id = Uuid::new_v4();
        assert!(ingest(&pool, parent, &public_key, &[unknown]).await);
        let mut spoof = report.clone();
        spoof.target.guest_identity = "other".into();
        spoof.target.unit = "microvm@other.service".into();
        assert!(ingest(&pool, parent, &public_key, &[spoof]).await);
        assert!(
            ingest(
                &pool,
                parent,
                &public_key,
                &[report.clone(), report.clone()]
            )
            .await
        );
        let rejected_count: i64 = sqlx::query_scalar("SELECT count(*) FROM guest_observations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            rejected_count, 0,
            "unauthorized and duplicate reports are never stored"
        );
        assert!(
            ingest(
                &pool,
                authenticated.system.id,
                &public_key,
                &[report.clone()]
            )
            .await
        );
        assert!(latest(&pool, guest, Some(env)).await.unwrap().is_some());
        sqlx::query("UPDATE systems SET is_active=false WHERE id=$1")
            .bind(guest)
            .execute(&pool)
            .await
            .unwrap();
        assert!(ingest(&pool, parent, &public_key, &[report.clone()]).await);
        assert!(latest(&pool, guest, Some(env)).await.unwrap().is_none());
        sqlx::query("UPDATE systems SET is_active=true WHERE id=$1")
            .bind(guest)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
            .bind(guest)
            .bind(other_env)
            .execute(&pool)
            .await
            .unwrap();
        assert!(ingest(&pool, parent, &public_key, &[report.clone()]).await);
        assert!(latest(&pool, guest, Some(env)).await.unwrap().is_none());
        sqlx::query("UPDATE systems SET environment_id=$2, management_mode='reference', parent_host_id=NULL WHERE id=$1").bind(guest).bind(env).execute(&pool).await.unwrap();
        // A stale cached target must not strand the parent in an authentication
        // failure loop: accept its heartbeat, discard the stale observation and
        // return an empty next-pull target list after reference conversion.
        let mut tx = pool.begin().await.unwrap();
        crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx)
            .await
            .unwrap();
        let targets = ingest_tx(&mut tx, parent, &public_key, &[report])
            .await
            .unwrap()
            .expect("valid parent receives updated targets despite stale guest report");
        assert!(targets.is_empty());
        tx.commit().await.unwrap();
        assert!(latest(&pool, guest, Some(env)).await.unwrap().is_none());
        let retained: i64 =
            sqlx::query_scalar("SELECT count(*) FROM guest_observations WHERE guest_id=$1")
                .bind(guest)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(retained, 1);
        assert!(ingest(&pool, parent, &public_key, &[]).await);
        sqlx::query("UPDATE systems SET is_active=false WHERE id=$1")
            .bind(parent)
            .execute(&pool)
            .await
            .unwrap();
        assert!(!ingest(&pool, parent, &public_key, &[]).await);
    }

    /// TASK-508 AC #11: a second, valid managed parent in the same environment
    /// cannot report another parent's guest. Its batch stores nothing; the
    /// associated parent's report is stored.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn management_mode_guest_observation_rejects_a_second_managed_parent(pool: PgPool) {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let key = |seed: u8| {
            STANDARD.encode(
                ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                    .verifying_key()
                    .to_bytes(),
            )
        };
        let (key_a, key_b) = (key(81), key(82));
        let env: Uuid = sqlx::query_scalar(
            "INSERT INTO environments(name) VALUES ('guest-rogue-parent') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut parents = Vec::new();
        for (name, public_key) in [("parent-a", &key_a), ("parent-b", &key_b)] {
            parents.push(
                sqlx::query_scalar::<_, Uuid>(
                    "INSERT INTO systems(hostname,public_key,environment_id,derivation) VALUES ($1,$2,$3,'') RETURNING id",
                )
                .bind(name)
                .bind(public_key)
                .bind(env)
                .fetch_one(&pool)
                .await
                .unwrap(),
            );
        }
        let guest: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname,management_mode,parent_host_id,environment_id,derivation) VALUES ('guest','external',$1,$2,'') RETURNING id",
        )
        .bind(parents[0])
        .bind(env)
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut observation = report();
        observation.target.guest_id = guest;
        let stored = || {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM guest_observations WHERE guest_id=$1",
                )
                .bind(guest)
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };
        for (parent, public_key) in [(parents[1], &key_b), (parents[0], &key_a)] {
            let mut tx = pool.begin().await.unwrap();
            crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx)
                .await
                .unwrap();
            let targets = ingest_tx(&mut tx, parent, public_key, &[observation.clone()])
                .await
                .unwrap()
                .expect("both parents are authenticated managed hosts");
            tx.commit().await.unwrap();
            if parent == parents[1] {
                assert!(targets.is_empty(), "parent B has no guest targets");
                assert_eq!(stored().await, 0, "parent B cannot report parent A's guest");
            } else {
                assert_eq!(targets.len(), 1);
                assert_eq!(
                    stored().await,
                    1,
                    "the associated parent's report is stored"
                );
            }
        }
    }
}
