//! Time-driven attention reconciliation for systems, environments, and flakes.
//!
//! System health and flake sync staleness are computed continuously from
//! heartbeat/timestamp data rather than being pushed on every relevant
//! write, so opening and resolving their canonical attention occurrences
//! cannot rely solely on a request-triggered hook (e.g. an agent heartbeat):
//! a system that stops heartbeating, or a flake sync that gets stuck,
//! crosses its attention threshold with no request arriving at that moment.
//!
//! This module provides:
//!
//! * [`reconcile_system_attention`] — the shared reconciliation logic used
//!   both by the agent heartbeat handler (immediate reconciliation after
//!   every heartbeat/state change) and the periodic loop below (catches
//!   systems that silently went offline with no further heartbeat).
//! * [`run_attention_reconciliation_loop`] — a bounded background task that
//!   periodically reconciles all active systems' health attention and all
//!   flakes stuck in `syncing` past the staleness threshold.

use crate::queries::attention;
use chrono::Utc;
use sqlx::{PgPool, Row};
use std::time::Duration;
use tracing::{debug, error, warn};
use uuid::Uuid;

/// How often the periodic reconciliation sweep runs. The scan is bounded to
/// the active fleet (systems) and syncing flakes, not growing history, so a
/// short interval is safe. Frequent enough that a new offline/stale
/// condition surfaces well within the sidebar's 30-second badge poll cadence
/// without materially increasing database load.
const RECONCILIATION_INTERVAL: Duration = Duration::from_secs(120);

/// Reconcile system health attention (and the derived environment attention)
/// for one system given its already-computed `health` status. Idempotent and
/// safe to call redundantly — used both synchronously after a
/// heartbeat/state-change commit, and periodically for systems that have not
/// sent a heartbeat recently enough to trigger that hook.
///
/// All system and environment transitions for this system are performed
/// atomically inside a single transaction under the reason-independent system
/// subject lock (`attention_occurrence:systems:{system_id}`), preventing races
/// between concurrent heartbeat handlers and the periodic reconciliation sweep.
pub async fn reconcile_system_attention(
    pool: &PgPool,
    system_id: Uuid,
    hostname: &str,
    health: &str,
    environment_id: Option<Uuid>,
) {
    let subject_id = system_id.to_string();
    let opened_at = Utc::now();

    // Single transaction for the entire system+environment transition.
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            warn!("failed to begin system attention transaction: {e:#}");
            return;
        }
    };

    // Acquire the reason-independent system subject lock (same key as
    // transition_by_subject), serializing this entire function with any
    // concurrent call for the same system.
    let lock_key = format!("attention_occurrence:systems:{subject_id}");
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
    {
        warn!("failed to acquire system attention lock: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    match health {
        "critical" | "offline" => {
            // ── System transition (inline transition_by_subject logic) ──────
            //
            // We do this inline (rather than calling transition_by_subject
            // which starts its own transaction) because we need to perform
            // the environment transition atomically in the same transaction
            // under the same lock.

            // Inject reason into metadata early so both paths preserve it.
            let mut metadata = serde_json::json!({
                "system_id": system_id.to_string(),
                "hostname": hostname,
                "health_status": health,
            });
            if let serde_json::Value::Object(ref mut map) = metadata {
                map.insert(
                    "reason".to_string(),
                    serde_json::Value::String(health.to_string()),
                );
            }

            let system_occurrence_id = {
                // Check if there is already an open occurrence with the same reason.
                let existing: Option<uuid::Uuid> = match sqlx::query_scalar(
                    r#"
                    SELECT id FROM attention_occurrences
                    WHERE category = 'systems'
                      AND subject_id = $1
                      AND resolved_at IS NULL
                      AND metadata @> $2::jsonb
                    LIMIT 1
                    FOR UPDATE
                    "#,
                )
                .bind(&subject_id)
                .bind(serde_json::json!({"reason": health}))
                .fetch_optional(&mut *tx)
                .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("failed to find existing system occurrence: {e:#}");
                        let _ = tx.rollback().await;
                        return;
                    }
                };

                match existing {
                    Some(existing_id) => {
                        // Reason matches — just observe the existing occurrence.
                        if let Err(e) = sqlx::query(
                            "UPDATE attention_occurrences SET last_observed_at = GREATEST(last_observed_at, $1), metadata = $2 WHERE id = $3",
                        )
                        .bind(opened_at)
                        .bind(&metadata)
                        .bind(existing_id)
                        .execute(&mut *tx)
                        .await
                        {
                            warn!("failed to update system occurrence: {e:#}");
                            let _ = tx.rollback().await;
                            return;
                        }
                        existing_id
                    }
                    None => {
                        // Reason differs or no occurrence exists — resolve all
                        // open occurrences and insert a new one.
                        if let Err(e) = sqlx::query(
                            r#"
                            UPDATE attention_occurrences
                            SET resolved_at = NOW()
                            WHERE category = 'systems'
                              AND subject_id = $1
                              AND resolved_at IS NULL
                            "#,
                        )
                        .bind(&subject_id)
                        .execute(&mut *tx)
                        .await
                        {
                            warn!("failed to resolve open system occurrences: {e:#}");
                            let _ = tx.rollback().await;
                            return;
                        }

                        let episode_id = uuid::Uuid::new_v4();
                        let source_key =
                            attention::system_occurrence_key(system_id, health, episode_id);

                        match sqlx::query_scalar::<_, uuid::Uuid>(
                            r#"
                            INSERT INTO attention_occurrences (
                                category, subject_type, subject_id, source_occurrence_key,
                                opened_at, last_observed_at, metadata
                            )
                            VALUES ('systems', 'system_health', $1, $2, $3, $4, $5)
                            RETURNING id
                            "#,
                        )
                        .bind(&subject_id)
                        .bind(source_key)
                        .bind(opened_at)
                        .bind(opened_at)
                        .bind(&metadata)
                        .fetch_optional(&mut *tx)
                        .await
                        {
                            Ok(Some(id)) => id,
                            Ok(None) => {
                                warn!("system occurrence insert returned no row");
                                let _ = tx.rollback().await;
                                return;
                            }
                            Err(e) => {
                                warn!("failed to insert system occurrence: {e:#}");
                                let _ = tx.rollback().await;
                                return;
                            }
                        }
                    }
                }
            };

            // ── Environment transition (inside the same lock) ─────────────
            if let Some(env_id) = environment_id {
                let source_key: Option<String> = sqlx::query_scalar(
                    "SELECT source_occurrence_key FROM attention_occurrences WHERE id = $1",
                )
                .bind(system_occurrence_id)
                .fetch_optional(&mut *tx)
                .await
                .ok()
                .flatten();

                if let Some(sys_source_key) = source_key {
                    // Resolve stale env occurrences for this system EXCEPT
                    // the current health_status, so the env occurrence for
                    // this same reason is preserved (not destroyed and
                    // recreated) when the health status hasn't changed.
                    let _ =
                        attention::resolve_environment_occurrences_for_system_except_reason(
                            &mut *tx,
                            env_id,
                            system_id,
                            health,
                        )
                        .await
                        .map_err(|e| {
                            warn!("failed to resolve stale environment occurrence: {e:#}")
                        });

                    // Open or observe the env occurrence inline (we are
                    // already inside a transaction holding the system subject
                    // lock, so we cannot use open_or_observe_by_subject which
                    // starts its own transaction). Use episode-based keys so
                    // each env incident gets a unique source_occurrence_key
                    // that cannot collide with a previously resolved row.
                    let env_metadata = serde_json::json!({
                        "reason": health,
                        "environment_id": env_id.to_string(),
                        "underlying_system_id": system_id.to_string(),
                        "underlying_system_occurrence_key": &sys_source_key,
                        "health_status": health,
                    });

                    let existing_env: Option<uuid::Uuid> = sqlx::query_scalar(
                        r#"
                        SELECT id FROM attention_occurrences
                        WHERE category = 'environments'
                          AND subject_id = $1
                          AND resolved_at IS NULL
                          AND metadata @> $2::jsonb
                        LIMIT 1
                        FOR UPDATE
                        "#,
                    )
                    .bind(env_id.to_string())
                    .bind(serde_json::json!({"underlying_system_id": system_id.to_string(), "reason": health}))
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| warn!("failed to find existing env occurrence: {e:#}"))
                    .ok()
                    .flatten();

                    if let Some(existing_env_id) = existing_env {
                        let _ = sqlx::query(
                            "UPDATE attention_occurrences SET last_observed_at = GREATEST(last_observed_at, $1), metadata = $2 WHERE id = $3",
                        )
                        .bind(opened_at)
                        .bind(&env_metadata)
                        .bind(existing_env_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| warn!("failed to update env occurrence: {e:#}"));
                    } else {
                        let env_episode_id = uuid::Uuid::new_v4();
                        let env_source_key =
                            attention::environment_occurrence_key(env_id, env_episode_id);
                        let _ = sqlx::query(
                            r#"
                            INSERT INTO attention_occurrences (
                                category, subject_type, subject_id, source_occurrence_key,
                                opened_at, last_observed_at, metadata
                            )
                            VALUES ('environments', 'environment', $1, $2, $3, $4, $5)
                            "#,
                        )
                        .bind(env_id.to_string())
                        .bind(env_source_key)
                        .bind(opened_at)
                        .bind(opened_at)
                        .bind(&env_metadata)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| warn!("failed to insert env occurrence: {e:#}"));
                    }
                }
            }
        }
        _ => {
            // ── Healthy recovery (inside the lock) ─────────────────────────
            // System is healthy/warning — resolve all open occurrences for
            // this system, which prevents the stale reconciler from racing
            // with our resolution by holding the same subject lock.
            let _ = attention::resolve_open_occurrences_for_subject(
                &mut *tx,
                "systems",
                &subject_id,
            )
            .await
            .map_err(|e| warn!("failed to resolve system attention occurrence: {e:#}"));

            // Also resolve every open environment occurrence derived from
            // this system now that its underlying occurrence is resolved.
            if let Some(env_id) = environment_id {
                let _ = attention::resolve_environment_occurrences_for_system(
                    &mut *tx,
                    env_id,
                    system_id,
                )
                .await
                .map_err(|e| {
                    warn!("failed to resolve environment attention occurrence: {e:#}")
                });
            }
        }
    }

    if let Err(e) = tx.commit().await {
        warn!("failed to commit system attention transaction: {e:#}");
    }
}

/// Reconcile attention for every active system by re-deriving current health
/// from `view_system_list`. Bounded to the active fleet, not growing history.
async fn reconcile_all_systems(pool: &PgPool) {
    let rows: Vec<(Uuid, String, String, Option<Uuid>)> = match sqlx::query_as(
        "SELECT s.id, s.hostname, vsl.health_status, s.environment_id \
         FROM systems s \
         JOIN view_system_list vsl ON vsl.id = s.id \
         WHERE s.is_active = TRUE",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!("failed to load active systems for attention reconciliation: {e:#}");
            return;
        }
    };

    for (system_id, hostname, health, environment_id) in rows {
        reconcile_system_attention(pool, system_id, &hostname, &health, environment_id).await;
    }
}

/// Resolve open occurrences that are stale due to lifecycle changes:
///
/// * System attention occurrences for inactive or deleted systems.
/// * Environment attention occurrences whose underlying system no longer
///   belongs to that environment (e.g. a system that moved environments
///   while its occurrence was still open for the old environment, or a
///   system that was deactivated or deleted).
///
/// The environment predicate resolves an occurrence whenever no active
/// system exists that both matches `underlying_system_id` and currently
/// belongs to the occurrence's environment (`subject_id`). This naturally
/// handles inactive, deleted, moved, and unassigned systems — a system
/// with `environment_id = NULL` cannot match because `NULL::text <> ao.subject_id`
/// evaluates to unknown (i.e. not true), so the NOT EXISTS subquery's
/// JOIN rejects it.
///
/// Run this after the active-system sweep so deactivated or moved systems
/// do not leave permanent orphaned occurrences.
async fn reconcile_stale_occurrences(pool: &PgPool) {
    // Resolve system occurrences for inactive/missing systems.
    let _ = sqlx::query(
        r#"
        UPDATE attention_occurrences ao
        SET resolved_at = NOW()
        WHERE ao.category = 'systems'
          AND ao.resolved_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM systems s
              WHERE s.id::text = ao.subject_id
                AND s.is_active = TRUE
          )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| error!("failed to resolve occurrences for inactive systems: {e:#}"));

    // Resolve environment occurrences where no active system with the
    // matching underlying_system_id currently belongs to the occurrence's
    // environment.
    let _ = sqlx::query(
        r#"
        UPDATE attention_occurrences ao
        SET resolved_at = NOW()
        WHERE ao.category = 'environments'
          AND ao.resolved_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM systems s
              WHERE s.id::text = ao.metadata->>'underlying_system_id'
                AND s.is_active = TRUE
                AND s.environment_id::text = ao.subject_id
          )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| error!("failed to resolve stale environment occurrences: {e:#}"));
}

/// Reconcile attention for flakes stuck in `syncing` past the staleness
/// threshold, using the same 30-minute predicate as the read-side "Sync
/// appears stale" detection in `queries::flakes::list_flake_registry`.
/// Resolution on recovery is handled by `sync_flake_recorded`'s own
/// success/error transitions (which resolve by subject regardless of the
/// reason an occurrence was opened with); this sweep only needs to *open*
/// the occurrence for a flake that has gone stale without an explicit sync
/// completing, since there is no other event to hook for that case.
async fn reconcile_stale_flakes(pool: &PgPool) {
    let rows: Vec<(i32,)> = match sqlx::query_as(
        r#"
        SELECT id
        FROM flakes
        WHERE deleted_at IS NULL
          AND sync_status = 'syncing'
          AND last_sync_at IS NOT NULL
          AND last_sync_at < now() - interval '30 minutes'
        "#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!("failed to load stale-syncing flakes for attention reconciliation: {e:#}");
            return;
        }
    };

    let opened_at = Utc::now();
    for (flake_id,) in rows {
        let subject_id = flake_id.to_string();
        reconcile_single_stale_flake(pool, flake_id, &subject_id, opened_at).await;
    }
}

/// Reconcile a single stale flake, with recheck inside the attention lock.
///
/// The flake ID was selected before the lock was acquired, so the flake
/// could have completed or errored in the meantime. This function acquires
/// the subject-level attention lock, rechecks the stale predicate against
/// the current database state, and only transitions when the flake is still
/// `syncing` and past the staleness threshold.
async fn reconcile_single_stale_flake(
    pool: &PgPool,
    flake_id: i32,
    subject_id: &str,
    opened_at: chrono::DateTime<chrono::Utc>,
) {
    use anyhow::Context;

    // Acquire the subject-level lock and recheck inside the transaction.
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            warn!("failed to begin stale flake transaction: {e:#}");
            return;
        }
    };

    let lock_key = format!("attention_occurrence:flakes:{subject_id}");
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
    {
        warn!("failed to acquire stale flake lock: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    // Recheck: is the flake still stale?
    let still_stale: bool = match sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM flakes
            WHERE id = $1
              AND deleted_at IS NULL
              AND sync_status = 'syncing'
              AND last_sync_at IS NOT NULL
              AND last_sync_at < now() - interval '30 minutes'
        )
        "#,
    )
    .bind(flake_id)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            warn!("failed to recheck stale flake predicate: {e:#}");
            let _ = tx.rollback().await;
            return;
        }
    };

    if !still_stale {
        // Flake is no longer stale — nothing to do.
        let _ = tx.commit().await;
        return;
    }

    // Still stale — use transition_by_subject logic inline.
    // Check if there's already an open occurrence with reason = stale_sync.
    let existing: Option<uuid::Uuid> = match sqlx::query_scalar(
        r#"
        SELECT id FROM attention_occurrences
        WHERE category = 'flakes'
          AND subject_id = $1
          AND resolved_at IS NULL
          AND metadata @> $2::jsonb
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(subject_id)
    .bind(serde_json::json!({"reason": "stale_sync"}))
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            warn!("failed to find existing stale flake occurrence: {e:#}");
            let _ = tx.rollback().await;
            return;
        }
    };

    if let Some(existing_id) = existing {
        // Already open for this reason — just update last_observed_at.
        let metadata = serde_json::json!({
            "reason": "stale_sync",
            "flake_id": flake_id,
        });
        if let Err(e) = sqlx::query(
            "UPDATE attention_occurrences SET last_observed_at = GREATEST(last_observed_at, $1), metadata = $2 WHERE id = $3",
        )
        .bind(opened_at)
        .bind(metadata)
        .bind(existing_id)
        .execute(&mut *tx)
        .await
        {
            warn!("failed to update stale flake occurrence: {e:#}");
            let _ = tx.rollback().await;
            return;
        }
        if let Err(e) = tx.commit().await {
            warn!("failed to commit stale flake update: {e:#}");
        }
        return;
    }

    // Resolve any other open occurrence (e.g. sync_error) and insert
    // a new stale_sync occurrence.
    if let Err(e) = sqlx::query(
        r#"
        UPDATE attention_occurrences
        SET resolved_at = NOW()
        WHERE category = 'flakes'
          AND subject_id = $1
          AND resolved_at IS NULL
        "#,
    )
    .bind(subject_id)
    .execute(&mut *tx)
    .await
    {
        warn!("failed to resolve open flake occurrences: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    let episode_id = uuid::Uuid::new_v4();
    let source_key = attention::flake_occurrence_key(flake_id, episode_id);
    let metadata = serde_json::json!({
        "reason": "stale_sync",
        "flake_id": flake_id,
    });

    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO attention_occurrences (
            category, subject_type, subject_id, source_occurrence_key,
            opened_at, last_observed_at, metadata
        )
        VALUES ('flakes', 'flake_sync', $1, $2, $3, $4, $5)
        "#,
    )
    .bind(subject_id)
    .bind(source_key)
    .bind(opened_at)
    .bind(opened_at)
    .bind(metadata)
    .execute(&mut *tx)
    .await
    {
        warn!("failed to insert stale flake occurrence: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    if let Err(e) = tx.commit().await {
        warn!("failed to commit stale flake transaction: {e:#}");
    }
}

/// Periodic bounded reconciliation for time-derived attention state: system
/// health (systems that stopped heartbeating) and stale flake syncs. Runs on
/// a fixed interval and is idempotent, so overlapping runs (should a sweep
/// ever take longer than the interval) cannot create duplicate open
/// occurrences — the underlying open/observe helpers are safe under
/// concurrent execution (advisory-lock-serialized per subject+reason).
pub async fn run_attention_reconciliation_loop(pool: PgPool) {
    tracing::info!(
        "🔁 Starting attention reconciliation loop (interval={:?})",
        RECONCILIATION_INTERVAL
    );
    let mut ticker = tokio::time::interval(RECONCILIATION_INTERVAL);
    loop {
        ticker.tick().await;
        reconcile_all_systems(&pool).await;
        reconcile_stale_flakes(&pool).await;
        reconcile_terminal_events(&pool).await;
        reconcile_stale_occurrences(&pool).await;
        debug!("Attention reconciliation sweep complete");
    }
}

/// Reconcile terminal build failures and evaluation failures that may have
/// been lost due to a transient database error after the domain transaction
/// committed — the build/eval status update commits first, then the attention
/// occurrence insertion is best-effort (error logged and ignored). If that
/// insertion failed, no reconciliation would otherwise recreate it.
///
/// Only selects events whose deterministic occurrence key does not already
/// exist in `attention_occurrences`, so events that already have their
/// occurrence are skipped. Ordered by `completed_at DESC` so the most
/// recent (and most likely still attention-eligible) events are processed
/// first, preventing a large backlog from permanently starving an event
/// that needs recovery. Bounded to 500 rows per category per sweep.
///
/// The build occurrence's `opened_at` uses the build's own `completed_at`
/// rather than the current time, so the 24-hour attention window reflects
/// the actual failure time, not the reconciliation time.
async fn reconcile_terminal_events(pool: &PgPool) {
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);

    // Recent build failures whose occurrence key does not exist yet.
    let builds: Vec<(uuid::Uuid, chrono::DateTime<chrono::Utc>)> = match sqlx::query_as(
        r#"
        SELECT id, completed_at FROM build_jobs bj
        WHERE status = 'failed'
          AND completed_at >= $1
          AND NOT EXISTS (
              SELECT 1 FROM attention_occurrences ao
              WHERE ao.category = 'builds'
                AND ao.source_occurrence_key = 'build:' || bj.id::text
          )
        ORDER BY completed_at DESC
        LIMIT 500
        "#,
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!("failed to load recent build failures for reconciliation: {e:#}");
            return;
        }
    };

    for (job_id, completed_at) in &builds {
        let _ = attention::open_or_observe(
            pool,
            "builds",
            "build_job",
            &job_id.to_string(),
            &attention::build_occurrence_key(*job_id),
            *completed_at,
            serde_json::json!({"job_id": job_id.to_string()}),
        )
        .await
        .map_err(|e| warn!("failed to reconcile build attention occurrence: {e:#}"));
    }

    // Recent evaluation failures whose occurrence key does not exist yet.
    // The key includes microsecond-precision completed_at, so we must
    // query both commit_id and completed_at.
    //
    // IMPORTANT: The key MUST use EXTRACT(MICROSECONDS FROM interval)
    // (exact integer arithmetic) rather than EXTRACT(EPOCH FROM ...) *
    // 1000000 (float64-based), because float64 cannot represent all epoch
    // microsecond values exactly (~16-17 significant digits vs float64's
    // ~15.95), causing off-by-one truncation errors. The Rust side uses
    // DateTime::timestamp_micros() which is exact integer arithmetic;
    // interval-based subtraction is the only PostgreSQL approach that
    // reproduces the exact same value.
    let evals: Vec<(i32, chrono::DateTime<chrono::Utc>)> = match sqlx::query_as(
        r#"
        SELECT c.id, c.evaluation_completed_at FROM commits c
        WHERE c.evaluation_status = 'failed'
          AND c.evaluation_completed_at IS NOT NULL
          AND c.evaluation_completed_at >= $1
          AND NOT EXISTS (
              SELECT 1 FROM attention_occurrences ao
              WHERE ao.category = 'evals'
                AND ao.source_occurrence_key =
                    'eval:' || c.id::text || ':' || EXTRACT(MICROSECONDS FROM (c.evaluation_completed_at - 'epoch'::timestamptz))::bigint::text
          )
        ORDER BY c.evaluation_completed_at DESC
        LIMIT 500
        "#,
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!("failed to load recent eval failures for reconciliation: {e:#}");
            return;
        }
    };

    for (commit_id, completed_at) in &evals {
        let key = attention::eval_occurrence_key(*commit_id, *completed_at);
        let _ = attention::open_or_observe(
            pool,
            "evals",
            "commit_eval",
            &commit_id.to_string(),
            &key,
            *completed_at,
            serde_json::json!({"commit_id": commit_id}),
        )
        .await
        .map_err(|e| warn!("failed to reconcile eval attention occurrence: {e:#}"));
    }
}

/// Default retention for resolved attention occurrences and their
/// dismissals, matching the SQL function's own default and the design's
/// "retain resolved occurrences ... for at least 30 days" requirement.
const CLEANUP_RESOLVED_RETENTION: chrono::Duration = chrono::Duration::days(30);

/// Maximum rows deleted per cleanup category per run, bounding cleanup cost
/// regardless of how large the backlog has grown.
const CLEANUP_BATCH_SIZE: i32 = 1000;

/// Runs `attention::cleanup` daily. Cleanup is bounded, idempotent, and
/// never required for badge correctness (the 24-hour attention rule is a
/// query predicate on `opened_at`, not a function of whether this job has
/// run) — it exists solely to bound the long-term size of
/// `attention_occurrences`/`user_attention_dismissals`.
pub async fn run_attention_cleanup_loop(pool: PgPool) {
    tracing::info!(
        "🔁 Starting attention cleanup loop (retention={:?}, batch_size={})",
        CLEANUP_RESOLVED_RETENTION,
        CLEANUP_BATCH_SIZE
    );
    let mut ticker = tokio::time::interval(Duration::from_secs(24 * 60 * 60));
    loop {
        ticker.tick().await;
        match attention::cleanup(&pool, CLEANUP_RESOLVED_RETENTION, CLEANUP_BATCH_SIZE).await {
            Ok((deleted_occurrences, deleted_dismissals)) => {
                if deleted_occurrences > 0 || deleted_dismissals > 0 {
                    tracing::info!(
                        "🗑️  Attention cleanup: {} occurrences, {} dismissals removed",
                        deleted_occurrences,
                        deleted_dismissals
                    );
                }
            }
            Err(e) => error!("attention cleanup failed: {e:#}"),
        }
    }
}
