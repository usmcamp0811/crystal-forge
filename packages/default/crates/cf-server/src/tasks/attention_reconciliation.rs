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
use sqlx::PgPool;
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
pub async fn reconcile_system_attention(
    pool: &PgPool,
    system_id: Uuid,
    hostname: &str,
    health: &str,
    environment_id: Option<Uuid>,
) {
    let subject_id = system_id.to_string();
    match health {
        "critical" | "offline" => {
            let opened_at = Utc::now();

            // Close any occurrence open for a *different* reason first (e.g.
            // critical -> offline): the old episode ends and a new one opens
            // for the current reason, rather than leaving both open
            // simultaneously.
            let _ = attention::resolve_open_occurrences_except_reason(
                pool,
                "systems",
                &subject_id,
                health,
            )
            .await
            .map_err(|e| {
                warn!("failed to resolve prior-reason system attention occurrence: {e:#}")
            });
            if let Some(env_id) = environment_id {
                let _ = attention::resolve_environment_occurrences_for_system_except_reason(
                    pool, env_id, system_id, health,
                )
                .await
                .map_err(|e| {
                    warn!("failed to resolve prior-reason environment attention occurrence: {e:#}")
                });
            }

            let result = attention::open_or_observe_by_subject(
                pool,
                "systems",
                "system_health",
                &subject_id,
                health,
                opened_at,
                serde_json::json!({
                    "system_id": system_id.to_string(),
                    "hostname": hostname,
                    "health_status": health,
                }),
                |reason, episode_id| {
                    attention::system_occurrence_key(system_id, reason, episode_id)
                },
            )
            .await;

            if let Ok(occurrence_id) = result {
                // Look up the source_occurrence_key so we can create
                // the corresponding environment occurrence.
                let source_key: Option<String> = sqlx::query_scalar(
                    "SELECT source_occurrence_key FROM attention_occurrences WHERE id = $1",
                )
                .bind(occurrence_id)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();

                if let (Some(env_id), Some(sys_source_key)) = (environment_id, source_key) {
                    let env_key = attention::environment_occurrence_key(env_id, &sys_source_key);
                    let _ = attention::open_or_observe(
                        pool,
                        "environments",
                        "environment",
                        &env_id.to_string(),
                        &env_key,
                        opened_at,
                        serde_json::json!({
                            "environment_id": env_id.to_string(),
                            "underlying_system_id": system_id.to_string(),
                            "underlying_system_occurrence_key": &sys_source_key,
                            "health_status": health,
                        }),
                    )
                    .await
                    .map_err(|e| warn!("failed to open environment attention occurrence: {e:#}"));
                }
            } else if let Err(e) = result {
                warn!("failed to open system attention occurrence: {e:#}");
            }
        }
        _ => {
            // System is healthy/warning — resolve any unresolved occurrence.
            let _ = attention::resolve_open_occurrences_for_subject(pool, "systems", &subject_id)
                .await
                .map_err(|e| warn!("failed to resolve system attention occurrence: {e:#}"));

            // Also resolve every open environment occurrence derived from
            // this system now that its underlying occurrence is resolved.
            if let Some(env_id) = environment_id {
                let _ =
                    attention::resolve_environment_occurrences_for_system(pool, env_id, system_id)
                        .await
                        .map_err(|e| {
                            warn!("failed to resolve environment attention occurrence: {e:#}")
                        });
            }
        }
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
///   while its occurrence was still open for the old environment).
///
/// Run this after the active-system sweep so deactivated or moved systems
/// do not leave permanent orphaned occurrences.
async fn reconcile_stale_occurrences(pool: &PgPool) {
    // Resolve system occurrences for inactive/missing systems.
    let _ = sqlx::query(
        r#"
        UPDATE attention_occurrences ao
        SET resolved_at = NOW()
        FROM systems s
        WHERE ao.category = 'systems'
          AND ao.subject_id = s.id::text
          AND ao.resolved_at IS NULL
          AND s.is_active = FALSE
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| error!("failed to resolve occurrences for inactive systems: {e:#}"));

    // Resolve environment occurrences where the underlying system no longer
    // belongs to that environment (e.g. system was moved). We match on the
    // metadata's underlying_system_id and compare with the system's current
    // environment_id.
    let _ = sqlx::query(
        r#"
        UPDATE attention_occurrences ao
        SET resolved_at = NOW()
        FROM systems s
        WHERE ao.category = 'environments'
          AND ao.resolved_at IS NULL
          AND ao.metadata @> jsonb_build_object('underlying_system_id', s.id::text)
          AND ao.subject_id != s.environment_id::text
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
        // Use transition_by_subject to atomically resolve any open flake
        // occurrence (e.g. a prior sync_error from the same attempt) before
        // opening the stale_sync occurrence. Without this, a stale_sync that
        // later fails would leave two simultaneously-open occurrences.
        let result = attention::transition_by_subject(
            pool,
            "flakes",
            "flake_sync",
            &subject_id,
            "stale_sync",
            opened_at,
            serde_json::json!({
                "flake_id": flake_id,
                "reason": "stale_sync",
            }),
            |_reason, episode_id| attention::flake_occurrence_key(flake_id, episode_id),
        )
        .await;
        if let Err(e) = result {
            warn!("failed to open attention occurrence for stale flake {flake_id}: {e:#}");
        }
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
/// The deterministic key-based helpers (`open_or_observe`) are idempotent:
/// calling them again for an event that already has an occurrence is a safe
/// no-op (ON CONFLICT DO UPDATE silently observes the existing row).
///
/// Only events from the last 24 hours are considered, matching the attention
/// window. Bounded to 500 rows per category per sweep.
async fn reconcile_terminal_events(pool: &PgPool) {
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);

    // Recent build failures that may be missing their attention occurrence.
    let builds: Vec<(uuid::Uuid,)> = match sqlx::query_as(
        "SELECT id FROM build_jobs \
         WHERE status = 'failed' AND completed_at >= $1 \
         LIMIT 500",
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

    for (job_id,) in &builds {
        let _ = attention::open_or_observe(
            pool,
            "builds",
            "build_job",
            &job_id.to_string(),
            &attention::build_occurrence_key(*job_id),
            chrono::Utc::now(),
            serde_json::json!({"job_id": job_id.to_string()}),
        )
        .await
        .map_err(|e| warn!("failed to reconcile build attention occurrence: {e:#}"));
    }

    // Recent evaluation failures that may be missing their attention
    // occurrence. The occurrence key includes microsecond-precision
    // completed_at, so we must query both commit_id and completed_at.
    let evals: Vec<(i32, chrono::DateTime<chrono::Utc>)> = match sqlx::query_as(
        "SELECT id, evaluation_completed_at FROM commits \
         WHERE evaluation_status = 'failed' \
           AND evaluation_completed_at IS NOT NULL \
           AND evaluation_completed_at >= $1 \
         LIMIT 500",
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
