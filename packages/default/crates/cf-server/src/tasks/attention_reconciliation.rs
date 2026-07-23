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
use futures::stream::{self, StreamExt};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
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
///
/// After acquiring the lock, the function re-reads the system's current health,
/// hostname, and environment from the database. If the health has changed since
/// the caller's snapshot, the operation is skipped — the caller that wrote the
/// newer health already handled the transition.
/// Inner implementation of [`reconcile_system_attention`] that returns
/// `Result` so every required DB operation uses `?` instead of error-swallowing
/// patterns (`.ok()`, `.map_err(warn)`, `let _ =`).
///
/// Round 12: the previous version's environment transition used `.ok().flatten()`
/// and `.map_err(|e| warn!(...))` on required lookups and mutations.  A decode
/// or client-side error could silently skip the environment transition while
/// the system transition still committed — breaking the claimed atomicity that
/// a system incident and its derived environment incident are always updated
/// together.  Every required operation now returns `Result` and uses `?`;
/// any error causes the entire transaction to be rolled back via the outer
/// wrapper function.
async fn reconcile_system_attention_inner(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    system_id: Uuid,
) -> anyhow::Result<()> {
    use anyhow::Context;
    let subject_id = system_id.to_string();

    // Acquire the reason-independent system subject lock (same key as
    // transition_by_subject), serializing this entire function with any
    // concurrent call for the same system.
    let lock_key = format!("attention_occurrence:systems:{subject_id}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut **tx)
        .await
        .context("failed to acquire system attention lock")?;

    // ── Re-read authoritative state inside the lock ──────────────────────
    // The caller's `health` and `environment_id` were read before the lock
    // was acquired, so a stale snapshot could race with a newer transition
    // and re-open an already-resolved incident.  Re-read from the database
    // under the lock so we act on the latest committed state.
    //
    // `opened_at` is also captured here, via `statement_timestamp()` rather
    // than `NOW()`/`transaction_timestamp()` (both fixed at transaction
    // start, i.e. before the advisory lock wait above) or a
    // pre-transaction `Utc::now()`. `statement_timestamp()` reflects the
    // time this specific statement runs — i.e. after the lock has been
    // acquired — so a caller delayed waiting for the lock does not record
    // an observation timestamp earlier than the state it eventually acts
    // on.
    let (health, hostname, environment_id, opened_at): (
        String,
        String,
        Option<Uuid>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as::<_, (String, String, Option<Uuid>, chrono::DateTime<chrono::Utc>)>(
        "SELECT vsl.health_status, s.hostname, s.environment_id, statement_timestamp() \
             FROM view_system_list vsl \
             JOIN systems s ON s.id = vsl.id \
             WHERE vsl.id = $1",
    )
    .bind(system_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to re-read system state")?
    .ok_or_else(|| anyhow::anyhow!("system {system_id} not found during attention reconciliation"))?;

    if health.is_empty() {
        anyhow::bail!("system {system_id} has empty health status during attention reconciliation");
    }

    match health.as_str() {
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
                "hostname": &hostname,
                "health_status": &health,
            });
            if let serde_json::Value::Object(ref mut map) = metadata {
                map.insert(
                    "reason".to_string(),
                    serde_json::Value::String(health.clone()),
                );
            }

            let system_occurrence_id = {
                // Check if there is already an open occurrence with the same reason.
                let existing: Option<uuid::Uuid> = sqlx::query_scalar(
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
                .bind(serde_json::json!({"reason": &health}))
                .fetch_optional(&mut **tx)
                .await
                .context("failed to find existing system occurrence")?;

                match existing {
                    Some(existing_id) => {
                        // Condition metadata replacement on observation ordering
                        // so an older caller that acquires the lock later cannot
                        // overwrite newer diagnostic information with stale metadata.
                        sqlx::query(
                            "UPDATE attention_occurrences \
                             SET metadata = CASE WHEN $1 >= last_observed_at THEN $2 ELSE metadata END, \
                                 last_observed_at = GREATEST(last_observed_at, $1) \
                             WHERE id = $3",
                        )
                        .bind(opened_at)
                        .bind(&metadata)
                        .bind(existing_id)
                        .execute(&mut **tx)
                        .await
                        .context("failed to update system occurrence")?;
                        existing_id
                    }
                    None => {
                        // Reason differs or no occurrence exists — resolve all
                        // open occurrences and insert a new one.
                        sqlx::query(
                            r#"
                            UPDATE attention_occurrences
                            SET resolved_at = NOW()
                            WHERE category = 'systems'
                              AND subject_id = $1
                              AND resolved_at IS NULL
                            "#,
                        )
                        .bind(&subject_id)
                        .execute(&mut **tx)
                        .await
                        .context("failed to resolve open system occurrences")?;

                        let episode_id = uuid::Uuid::new_v4();
                        let source_key =
                            attention::system_occurrence_key(system_id, &health, episode_id);

                        sqlx::query_scalar::<_, uuid::Uuid>(
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
                        .fetch_one(&mut **tx)
                        .await
                        .context("failed to insert system occurrence")?
                    }
                }
            };

            // ── Environment transition (inside the same lock) ─────────────
            if let Some(env_id) = environment_id {
                let sys_source_key: String = sqlx::query_scalar(
                    "SELECT source_occurrence_key FROM attention_occurrences WHERE id = $1",
                )
                .bind(system_occurrence_id)
                .fetch_optional(&mut **tx)
                .await
                .context("failed to read system occurrence key for environment transition")?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "system occurrence {system_occurrence_id} not found after insert"
                    )
                })?;

                // Resolve env occurrences for this system ACROSS EVERY
                // ENVIRONMENT, except the exact current tuple:
                //   current environment
                //   current underlying_system_occurrence_key
                //   current reason
                //
                // This prevents reusing an env occurrence from a prior
                // system episode (e.g. a system that moved away and back)
                // while keeping the one for the current episode open.
                sqlx::query(
                    r#"
                    UPDATE attention_occurrences
                    SET resolved_at = NOW()
                    WHERE category = 'environments'
                      AND resolved_at IS NULL
                      AND metadata @> $1::jsonb
                      AND NOT (
                          subject_id = $2
                          AND metadata @> $3::jsonb
                          AND metadata @> $4::jsonb
                      )
                    "#,
                )
                .bind(serde_json::json!({"underlying_system_id": system_id.to_string()}))
                .bind(env_id.to_string())
                .bind(serde_json::json!({"underlying_system_occurrence_key": &sys_source_key}))
                .bind(serde_json::json!({"reason": &health}))
                .execute(&mut **tx)
                .await
                .context("failed to resolve stale environment occurrence")?;

                // Open or observe the env occurrence inline (we are
                // already inside a transaction holding the system subject
                // lock, so we cannot use open_or_observe_by_subject which
                // starts its own transaction). Use episode-based keys so
                // each env incident gets a unique source_occurrence_key
                // that cannot collide with a previously resolved row.
                //
                // The existing-row lookup includes
                // `underlying_system_occurrence_key` so a resolved env
                // occurrence from a prior system episode cannot be
                // recycled for a different one.
                let env_metadata = serde_json::json!({
                    "reason": &health,
                    "environment_id": env_id.to_string(),
                    "underlying_system_id": system_id.to_string(),
                    "underlying_system_occurrence_key": &sys_source_key,
                    "health_status": &health,
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
                .bind(serde_json::json!({
                    "underlying_system_id": system_id.to_string(),
                    "underlying_system_occurrence_key": &sys_source_key,
                    "reason": &health,
                }))
                .fetch_optional(&mut **tx)
                .await
                .context("failed to find existing env occurrence")?;

                if let Some(existing_env_id) = existing_env {
                    sqlx::query(
                        "UPDATE attention_occurrences \
                         SET last_observed_at = GREATEST(last_observed_at, $1), metadata = $2 \
                         WHERE id = $3",
                    )
                    .bind(opened_at)
                    .bind(&env_metadata)
                    .bind(existing_env_id)
                    .execute(&mut **tx)
                    .await
                    .context("failed to update env occurrence")?;
                } else {
                    let env_episode_id = uuid::Uuid::new_v4();
                    let env_source_key =
                        attention::environment_occurrence_key(env_id, env_episode_id);
                    sqlx::query(
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
                    .execute(&mut **tx)
                    .await
                    .context("failed to insert env occurrence")?;
                }
            }
        }
        "healthy" | "warning" | "" => {
            // ── Healthy recovery (inside the lock) ─────────────────────────
            // System is healthy/warning — resolve all open occurrences for
            // this system, which prevents the stale reconciler from racing
            // with our resolution by holding the same subject lock.
            attention::resolve_open_occurrences_for_subject(
                &mut **tx,
                "systems",
                &subject_id,
            )
            .await
            .context("failed to resolve system attention occurrence")?;

            // Also resolve every open environment occurrence derived from
            // this system now that its underlying occurrence is resolved —
            // across ALL environments, not just the system's *current* one.
            // A system that was critical in environment A and has since
            // moved to B (or been unassigned) before recovering must not
            // leave A's derived occurrence open.
            attention::resolve_environment_occurrences_for_system_any_environment(
                &mut **tx, system_id,
            )
            .await
            .context("failed to resolve environment attention occurrence")?;
        }
        other => {
            debug!("unknown health status '{other}' for system {system_id}; skipping");
        }
    }

    Ok(())
}

pub async fn reconcile_system_attention(
    pool: &PgPool,
    system_id: Uuid,
    _caller_health: &str,
    _caller_hostname: &str,
    _caller_environment_id: Option<Uuid>,
) {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            warn!("failed to begin system attention transaction: {e:#}");
            return;
        }
    };

    if let Err(e) = reconcile_system_attention_inner(&mut tx, system_id).await {
        warn!("system {system_id} attention reconciliation failed: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    if let Err(e) = tx.commit().await {
        warn!("failed to commit system attention transaction: {e:#}");
    }
}

/// Reconcile attention for every active system by re-deriving current health
/// from `view_system_list`. Bounded concurrency via semaphore so a large
/// fleet or blocked subject locks cannot starve later sweep passes (round 13
/// review, P2).
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

    const MAX_CONCURRENT: usize = 16;
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));

    stream::iter(rows)
        .map(|(system_id, hostname, health, environment_id)| {
            // Owned clones for the async block.
            let pool = pool.clone();
            let sem = semaphore.clone();
            async move {
                let _permit = match sem.acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => return, // semaphore closed — should not happen
                };
                reconcile_system_attention(
                    &pool,
                    system_id,
                    &hostname,
                    &health,
                    environment_id,
                )
                .await;
            }
        })
        .buffer_unordered(MAX_CONCURRENT)
        .for_each(|()| async {})
        .await;
}

/// Resolve open occurrences that are stale due to lifecycle changes:
///
/// * System attention occurrences for inactive or deleted systems.
/// * Environment attention occurrences whose underlying system no longer
///   belongs to that environment (e.g. a system that moved environments
///   while its occurrence was still open for the old environment, or a
///   system that was deactivated or deleted).
/// * Flake attention occurrences for deleted or missing flakes — a flake
///   that was soft-deleted or hard-deleted before its open occurrence was
///   explicitly resolved (either by this safety net or by the
///   delete-path resolve calls in `queries::flakes`) leaves an invisible
///   occurrence that still counts toward sidebar badges since the badge
///   query does not join against active flakes.
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

    // Resolve flake occurrences for deleted or missing flakes.  Both the
    // soft-delete and cascade-delete paths now resolve occurrences directly,
    // but this safety net catches any that were missed (e.g. a hard-delete
    // that predates this fix, or a delete that bypassed the resolve path
    // via `delete_flake_by_id`).
    let _ = sqlx::query(
        r#"
        UPDATE attention_occurrences ao
        SET resolved_at = NOW()
        WHERE ao.category = 'flakes'
          AND ao.resolved_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM flakes f
              WHERE f.id::text = ao.subject_id
                AND f.deleted_at IS NULL
          )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| error!("failed to resolve occurrences for deleted flakes: {e:#}"));

    // Resolve build occurrences whose build job no longer exists or is no
    // longer in a failed state.  Build attention is normally resolved
    // directly by retry/success hooks, but stale occurrences can remain
    // after a crash or if the build_job was deleted via flake-history
    // operations before this safety net was deployed (round 13).
    let _ = sqlx::query(
        r#"
        UPDATE attention_occurrences ao
        SET resolved_at = statement_timestamp()
        WHERE ao.category = 'builds'
          AND ao.resolved_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM build_jobs bj
              WHERE bj.id::text = ao.subject_id
                AND bj.status = 'failed'
          )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| error!("failed to resolve occurrences for non-failed/missing builds: {e:#}"));

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
    // Bounded (LIMIT 500, like every other sweep in this module) and
    // deterministically ordered (oldest-stale-first) so a large backlog
    // cannot delay the errored-flake, terminal-event, and stale-occurrence
    // sweeps that run after this one in the same loop iteration
    // indefinitely — each iteration makes bounded, fair progress on the
    // oldest incidents first, and any remainder is picked up by the next
    // sweep 2 minutes later.
    let rows: Vec<(i32,)> = match sqlx::query_as(
        r#"
        SELECT id
        FROM flakes
        WHERE deleted_at IS NULL
          AND sync_status = 'syncing'
          AND last_sync_at IS NOT NULL
          AND last_sync_at < now() - interval '30 minutes'
        ORDER BY last_sync_at ASC
        LIMIT 500
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

    for (flake_id,) in rows {
        let subject_id = flake_id.to_string();
        reconcile_single_stale_flake(pool, flake_id, &subject_id).await;
    }
}

/// Reconcile flakes currently in `error` status that are missing an authoritative
/// `sync_error` attention occurrence.
///
/// `record_sync_error` commits the flake's `sync_status = 'error'` first,
/// then performs the attention transition as a separate, best-effort
/// operation whose errors are only logged. A process crash or transient
/// failure between those two steps leaves an errored flake with no
/// canonical occurrence — and unlike a flake stuck in `syncing`, an
/// already-`error` flake is never picked up by
/// [`reconcile_stale_flakes`], so nothing would otherwise recover it.
///
/// This sweep is bounded to flakes currently in `error` with a recorded
/// `sync_attempt_id` and no *current* open `sync_error` occurrence, and
/// delegates the actual (locked, attempt-verified) transition to
/// [`crate::flake::commits::transition_flake_attention_to_error_if_current`]
/// — the same function the direct call site uses — so the recheck-then-act
/// safety this sweep exists to backstop is still honored even here.
///
/// An open `sync_error` occurrence is treated as "current" (and thus
/// excluding the flake from this sweep) only if no successful sync has
/// completed since it was last observed — checked via
/// `f.last_synced_at <= ao.last_observed_at`. When a success DID occur
/// after the occurrence was last observed (`last_synced_at > last_observed_at`),
/// the occurrence belongs to an earlier, superseded incident and must not
/// prevent the lineage-aware transition helper from running (and replacing it).
/// This is the same staleness check used inside the transition helper itself
/// (see `transition_flake_attention_to_error_if_current`).
///
/// The exclusion checks specifically for a *current*`sync_error` occurrence,
/// not "any open occurrence at all" — a flake can be `error` while still
/// carrying an open `stale_sync` occurrence from before it finished erroring
/// (e.g. a long sync opens `stale_sync`, then finally records
/// `sync_status = 'error'`, then the process crashes before transitioning
/// attention). Excluding on "any occurrence exists" would skip that flake
/// forever, leaving the user with a stale-sync incident instead of the
/// actual failure. `transition_flake_attention_to_error_if_current` itself
/// resolves the mismatched-reason `stale_sync` occurrence before opening
/// `sync_error`, so selecting the flake here is safe and idempotent.
async fn reconcile_errored_flakes(pool: &PgPool) {
    let rows: Vec<(i32, uuid::Uuid, Option<String>)> = match sqlx::query_as(
        r#"
        SELECT f.id, f.sync_attempt_id, f.last_sync_error
        FROM flakes f
        WHERE f.deleted_at IS NULL
          AND f.sync_status = 'error'
          AND f.sync_attempt_id IS NOT NULL
          AND NOT EXISTS (
              SELECT 1 FROM attention_occurrences ao
              WHERE ao.category = 'flakes'
                AND ao.subject_id = f.id::text
                AND ao.resolved_at IS NULL
                AND ao.metadata @> '{"reason": "sync_error"}'::jsonb
                AND (f.last_synced_at IS NULL OR f.last_synced_at <= ao.last_observed_at)
          )
        LIMIT 500
        "#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!("failed to load errored flakes for attention reconciliation: {e:#}");
            return;
        }
    };

    for (flake_id, attempt_id, last_sync_error) in rows {
        let metadata = serde_json::json!({
            "flake_id": flake_id,
            "last_sync_error": last_sync_error,
        });
        crate::flake::commits::transition_flake_attention_to_error_if_current(
            pool, flake_id, attempt_id, metadata,
        )
        .await;
    }
}

/// Reconcile flakes currently in `synced` status that still have an open
/// flake attention occurrence.
///
/// `sync_flake_recorded`'s success path commits `sync_status = 'synced'`
/// first, then calls `resolve_flake_attention_if_current` as a separate,
/// best-effort operation. A process crash between the two leaves a
/// successfully-synced flake with a stale `sync_error`/`stale_sync`
/// occurrence still open — a false alert that would otherwise remain
/// visible for the rest of its 24-hour attention window. No other sweep in
/// this loop examines `synced` flakes (`reconcile_stale_flakes` only looks
/// at `syncing`, `reconcile_errored_flakes` only at `error`), so nothing
/// else would recover it.
///
/// Delegates to [`crate::flake::commits::resolve_flake_attention_if_current`]
/// — the same locked, attempt-verified function the direct call site uses —
/// so a flake that has since moved on to a NEWER attempt (syncing again, or
/// already failed) is safely skipped by that function's own recheck rather
/// than incorrectly resolved here.
async fn reconcile_synced_flakes_missing_resolution(pool: &PgPool) {
    let rows: Vec<(i32, uuid::Uuid)> = match sqlx::query_as(
        r#"
        SELECT f.id, f.sync_attempt_id
        FROM flakes f
        WHERE f.deleted_at IS NULL
          AND f.sync_status = 'synced'
          AND f.sync_attempt_id IS NOT NULL
          AND EXISTS (
              SELECT 1 FROM attention_occurrences ao
              WHERE ao.category = 'flakes'
                AND ao.subject_id = f.id::text
                AND ao.resolved_at IS NULL
          )
        LIMIT 500
        "#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!("failed to load synced flakes for attention reconciliation: {e:#}");
            return;
        }
    };

    for (flake_id, attempt_id) in rows {
        crate::flake::commits::resolve_flake_attention_if_current(pool, flake_id, attempt_id)
            .await;
    }
}

/// Reconcile a single stale flake, with recheck inside the attention lock.
///
/// The flake ID was selected before the lock was acquired, so the flake
/// could have completed or errored in the meantime. This function acquires
/// the subject-level attention lock, rechecks the stale predicate against
/// the current database state, and only transitions when the flake is still
/// `syncing` and past the staleness threshold.
///
/// As with [`crate::flake::commits::transition_flake_attention_to_error_if_current`],
/// an existing `stale_sync` occurrence is only reused (observed) if no
/// successful sync has completed since it was last observed (checked via
/// `flakes.last_synced_at`) — otherwise a stale occurrence from an earlier,
/// already-resolved stuck-sync episode could be silently reused (and its
/// dismissal inherited) for an unrelated, later one.
async fn reconcile_single_stale_flake(pool: &PgPool, flake_id: i32, subject_id: &str) {
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

    // Recheck: is the flake still stale? Two DISTINCT timestamps are
    // derived, not one:
    //   - `opened_at` (`last_sync_at + 30 minutes`): the actual moment the
    //     sync crossed the staleness threshold — used for the occurrence's
    //     `opened_at`, so it reflects the incident's real start rather than
    //     whenever this periodic sweep happened to notice it.
    //   - `observed_at` (`statement_timestamp()`, i.e. after the lock wait):
    //     used to bump `last_observed_at` on repeated sweeps against a
    //     still-ongoing incident, so it keeps advancing instead of being
    //     pinned to the original threshold-crossing time forever.
    // `last_synced_at` is also fetched for the same-incident staleness
    // check described in the doc comment above.
    let recheck: Option<(
        bool,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        Option<chrono::DateTime<chrono::Utc>>,
    )> = match sqlx::query_as(
        r#"
        SELECT
            last_sync_at < now() - interval '30 minutes',
            last_sync_at,
            statement_timestamp(),
            last_synced_at
        FROM flakes
        WHERE id = $1
          AND deleted_at IS NULL
          AND sync_status = 'syncing'
          AND last_sync_at IS NOT NULL
        "#,
    )
    .bind(flake_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            warn!("failed to recheck stale flake predicate: {e:#}");
            let _ = tx.rollback().await;
            return;
        }
    };

    let (still_stale, last_sync_at, observed_at, last_synced_at) = match recheck {
        Some((stale, sync_at, observed, synced)) => (stale, sync_at, observed, synced),
        None => (false, chrono::Utc::now(), chrono::Utc::now(), None),
    };

    if !still_stale {
        // Flake is no longer stale — nothing to do.
        let _ = tx.commit().await;
        return;
    }

    let opened_at = last_sync_at + chrono::Duration::minutes(30);

    // Still stale — use transition_by_subject logic inline.
    // Check if there's already an open occurrence with reason = stale_sync,
    // also fetching its last_observed_at for the staleness check below.
    let existing: Option<(uuid::Uuid, chrono::DateTime<chrono::Utc>)> = match sqlx::query_as(
        r#"
        SELECT id, last_observed_at FROM attention_occurrences
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

    // Only reusable if no successful sync has completed since it was last
    // observed; otherwise it belongs to an earlier, superseded incident.
    let existing = existing.filter(|(_, last_observed_at)| {
        !matches!(last_synced_at, Some(synced) if synced > *last_observed_at)
    });

    if let Some((existing_id, _)) = existing {
        // Already open for this reason — just update last_observed_at.
        let metadata = serde_json::json!({
            "reason": "stale_sync",
            "flake_id": flake_id,
        });
        if let Err(e) = sqlx::query(
            "UPDATE attention_occurrences SET last_observed_at = GREATEST(last_observed_at, $1), metadata = $2 WHERE id = $3",
        )
        .bind(observed_at)
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

    // Resolve any other open occurrence (e.g. sync_error, or a stale
    // stale_sync from an earlier superseded incident) and insert a new
    // stale_sync occurrence.
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
    .bind(observed_at)
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
        reconcile_errored_flakes(&pool).await;
        reconcile_synced_flakes_missing_resolution(&pool).await;
        reconcile_terminal_events(&pool).await;
        reconcile_cve_attention(&pool).await;
        reconcile_stale_occurrences(&pool).await;
        debug!("Attention reconciliation sweep complete");
    }
}

/// Reconcile a single CVE's attention state under its per-CVE advisory lock.
///
/// Uses the same lock key as `open_or_observe_by_subject` (the scan-save
/// producer), so this reconciler and the producer are serialized per CVE:
///
/// ```text
/// lock_key = "attention_occurrence:cves:{cve_id}:critical"
/// ```
///
/// Inside the lock the function rechecks fleet relevance against the
/// authoritative view and either opens exactly one episode (using the
/// earliest scan that detected this CVE as `opened_at`, not the
/// reconciliation time) or resolves every open occurrence.
async fn reconcile_single_cve(pool: &PgPool, cve_id: &str) {
    // Delegate to the canonical per-CVE helper.  The helper acquires the
    // per-CVE lock, rechecks fleet relevance, and reconciles both the
    // attention_occurrence and the persisted cves.fleet_relevant_since
    // transition timestamp (round 16 review).
    if let Err(e) = attention::reconcile_cve_attention_subject(pool, cve_id).await {
        warn!("failed to reconcile CVE attention for {cve_id}: {e:#}");
    }
}

/// Reconcile CVE attention against authoritative fleet relevance.
///
/// CVEs have no hook-based attention lifecycle (unlike builds, evals, and
/// flake syncs whose domain transitions directly produce or resolve attention
/// occurrences) and no other periodic sweep recreates missing occurrences.
/// The CVE scan save path opens attention during completed scan-result
/// persistence, but that path writes the scan data first and performs the
/// attention operation as a separate, best-effort step — a crash between the
/// two can leave a currently critical, fleet-relevant CVE with no occurrence.
/// Additionally, the scan worker is disabled by default and may run on an
/// arbitrarily distant future schedule for a given target. Relying on a
/// future scan to eventually recreate a lost occurrence is not a bounded
/// repair strategy — see the Round 11 review for MR !307.
///
/// This safety net closes that gap by running on every periodic sweep.
/// Each CVE is processed under its per-CVE advisory lock (matching the
/// scan-save producer's lock), serializing the reconciler with any
/// concurrent producer and eliminating the duplicate-open and stale-resolution
/// races described in the Round 13 review.
///
/// Bounded to 500 CVEs per sweep (matching the other sweeps).
async fn reconcile_cve_attention(pool: &PgPool) {
    // Find all CVEs whose attention state may be out of sync with fleet
    // relevance. We select both missing and stale candidates in one pass
    // to avoid missing a CVE that transitions between the two queries
    // without needing a snapshot.
    let candidates: Vec<String> = match sqlx::query_scalar(
        r#"
        SELECT DISTINCT candidate FROM (
            -- Fleet-relevant CVEs with no open occurrence
            SELECT v.cve_id AS candidate
            FROM view_cve_list_with_metadata v
            WHERE v.severity = 'CRITICAL'
              AND v.affected_count > 0
              AND NOT EXISTS (
                  SELECT 1 FROM attention_occurrences ao
                  WHERE ao.category = 'cves'
                    AND ao.subject_id = v.cve_id
                    AND ao.resolved_at IS NULL
              )
            UNION
            -- Open occurrences whose CVE is no longer fleet-relevant
            SELECT ao.subject_id
            FROM attention_occurrences ao
            WHERE ao.category = 'cves'
              AND ao.resolved_at IS NULL
              AND NOT EXISTS (
                  SELECT 1 FROM view_cve_list_with_metadata v
                  WHERE v.cve_id = ao.subject_id
                    AND v.severity = 'CRITICAL'
                    AND v.affected_count > 0
              )
        ) candidates
        LIMIT 500
        "#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!("failed to load CVE candidates for attention reconciliation: {e:#}");
            return;
        }
    };

    for cve_id in candidates {
        reconcile_single_cve(pool, &cve_id).await;
    }
}

/// Reconcile terminal build failures and evaluation failures that may have
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
pub(crate) async fn reconcile_terminal_events(pool: &PgPool) {
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);

    // Recent build failures whose occurrence key does not exist yet.
    // Only consider commits that are still in the active branch snapshot,
    // so archived failures from a history rewrite cannot retain or reacquire
    // attention (round 16 review).
    let builds: Vec<(uuid::Uuid, chrono::DateTime<chrono::Utc>)> = match sqlx::query_as(
        r#"
        SELECT bj.id, bj.completed_at
        FROM build_jobs bj
        JOIN derivations d ON d.id = bj.derivation_id
        JOIN flake_branch_commit_snapshot snapshot ON snapshot.commit_id = d.commit_id
        WHERE bj.status = 'failed'
          AND bj.completed_at >= $1
          AND NOT EXISTS (
              SELECT 1 FROM attention_occurrences ao
              WHERE ao.category = 'builds'
                AND ao.source_occurrence_key = 'build:' || bj.id::text
          )
        ORDER BY bj.completed_at DESC
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

    // Recent evaluation failures whose occurrence does not exist yet.
    // Match on (subject_type, subject_id, opened_at) rather than
    // duplicating the source_occurrence_key encoding in SQL, which is
    // fragile and can diverge from the Rust encoding.  The attention
    // producer stores opened_at = evaluation_completed_at, so the
    // equality is exact at microsecond precision.
    //
    // Restrict to commits in the active branch snapshot so archived failures
    // from a history rewrite cannot retain or reacquire attention (round 16).
    let evals: Vec<(i32, chrono::DateTime<chrono::Utc>)> = match sqlx::query_as(
        r#"
        SELECT c.id, c.evaluation_completed_at
        FROM commits c
        JOIN flake_branch_commit_snapshot snapshot ON snapshot.commit_id = c.id
        WHERE c.evaluation_status = 'failed'
          AND c.evaluation_completed_at IS NOT NULL
          AND c.evaluation_completed_at >= $1
          AND NOT EXISTS (
              SELECT 1 FROM attention_occurrences ao
              WHERE ao.category = 'evals'
                AND ao.subject_type = 'commit_eval'
                AND ao.subject_id = c.id::text
                AND ao.opened_at = c.evaluation_completed_at
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

    // Safety net: resolve any open eval occurrence whose commit is no
    // longer in the exact (failed, completed_at) state that created it —
    // e.g. it was reset, re-evaluated, or completed. The direct-call paths
    // in queries::commits already do this under a per-commit lock with a
    // recheck (open_eval_attention_if_current /
    // resolve_eval_attention_unless_failed), but this sweep catches any
    // occurrence left behind by a process crash or dropped connection
    // between the domain commit and the attention action.
    //
    // Also resolve eval attention for commits that are no longer in the
    // active branch snapshot (round 16 review).
    if let Err(e) = sqlx::query(
        r#"
        UPDATE attention_occurrences ao
        SET resolved_at = NOW()
        WHERE ao.category = 'evals'
          AND ao.resolved_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM commits c
              WHERE c.id::text = ao.subject_id
                AND c.evaluation_status = 'failed'
                AND c.evaluation_completed_at = ao.opened_at
                AND EXISTS (
                    SELECT 1 FROM flake_branch_commit_snapshot snapshot
                    WHERE snapshot.commit_id = c.id
                )
          )
        "#,
    )
    .execute(pool)
    .await
    {
        error!("failed to resolve stale eval attention occurrences: {e:#}");
    }

    // Safety net for builds: resolve any open build occurrence whose job is
    // no longer a failed, recent, active-snapshot build.  This is the
    // mirror of the eval safety net above.
    if let Err(e) = sqlx::query(
        r#"
        UPDATE attention_occurrences ao
        SET resolved_at = NOW()
        WHERE ao.category = 'builds'
          AND ao.resolved_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM build_jobs bj
              JOIN derivations d ON d.id = bj.derivation_id
              WHERE ('build:' || bj.id::text) = ao.source_occurrence_key
                AND bj.status = 'failed'
                AND bj.completed_at = ao.opened_at
                AND EXISTS (
                    SELECT 1 FROM flake_branch_commit_snapshot snapshot
                    WHERE snapshot.commit_id = d.commit_id
                )
          )
        "#,
    )
    .execute(pool)
    .await
    {
        error!("failed to resolve stale build attention occurrences: {e:#}");
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

#[cfg(test)]
mod tests {
    use super::{
        reconcile_cve_attention, reconcile_errored_flakes,
        reconcile_synced_flakes_missing_resolution,
    };
    use crate::queries::attention;

    // Run against a repository-provided isolated database:
    //   DATABASE_URL=postgres://crystal_forge:password@localhost:3042/crystal_forge \
    //     cargo test -p cf-server --lib tasks::attention_reconciliation -- --ignored

    async fn test_pool() -> sqlx::PgPool {
        sqlx::PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"),
        )
        .await
        .expect("failed to connect to test database")
    }

    async fn insert_throwaway_flake(pool: &sqlx::PgPool) -> i32 {
        let short = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("att-recon-flake-{short}"))
        .bind(format!("https://git.example/att-recon-flake-{short}.git"))
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test flake")
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_errored_flakes_recovers_missing_occurrence() {
        // Regression test for round 7: record_sync_error commits
        // sync_status = 'error' and performs the attention transition as a
        // separate best-effort operation. If the latter is lost (process
        // crash, transient failure), the flake is left errored with no
        // canonical occurrence, and reconcile_stale_flakes never looks at
        // it (it only examines flakes stuck in 'syncing'). This sweep must
        // recover it.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let attempt_id = uuid::Uuid::new_v4();

        // Simulate: the status commit succeeded, but the attention
        // transition was never performed (e.g. process crashed right
        // after the UPDATE below).
        sqlx::query(
            "UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2, last_sync_error = $3 WHERE id = $1",
        )
        .bind(flake_id)
        .bind(attempt_id)
        .bind("simulated crash before attention transition")
        .execute(&pool)
        .await
        .unwrap();

        let open_before: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(flake_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(open_before, 0, "no occurrence should exist yet");

        reconcile_errored_flakes(&pool).await;

        let open_after: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(flake_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_after, 1,
            "the reconciliation sweep must recover the missing occurrence"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1",
        )
        .bind(flake_id.to_string())
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_errored_flakes_skips_flake_with_existing_sync_error_occurrence() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let attempt_id = uuid::Uuid::new_v4();

        sqlx::query(
            "UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2 WHERE id = $1",
        )
        .bind(flake_id)
        .bind(attempt_id)
        .execute(&pool)
        .await
        .unwrap();

        // A sync_error occurrence already exists (the normal, non-crashed
        // path) -- must be excluded by reason, not merely "any occurrence".
        let existing_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'flakes', 'flake_sync', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(existing_id)
        .bind(flake_id.to_string())
        .bind(format!("flake:{flake_id}:{}", uuid::Uuid::new_v4()))
        .bind(serde_json::json!({"reason": "sync_error"}))
        .execute(&pool)
        .await
        .unwrap();

        reconcile_errored_flakes(&pool).await;

        // The exact same row must still be open and untouched -- not
        // merely "some row is open" (which the old, weaker assertion
        // would not have distinguished from a resolve+reinsert).
        let still_open: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id FROM attention_occurrences \
             WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(flake_id.to_string())
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(
            still_open,
            Some(existing_id),
            "a flake that already has an open sync_error occurrence must be left untouched"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1",
        )
        .bind(flake_id.to_string())
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_errored_flakes_recovers_when_stale_sync_occurrence_exists() {
        // Regression test for round 8: a flake can be `error` while still
        // carrying an open `stale_sync` occurrence from before it finished
        // erroring (long sync opens stale_sync, then finally records
        // sync_status='error', then the process crashes before
        // transitioning attention). The sweep must still recover this --
        // excluding on "any open occurrence exists" would skip it forever.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let attempt_id = uuid::Uuid::new_v4();

        sqlx::query(
            "UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2, last_sync_at = now() WHERE id = $1",
        )
        .bind(flake_id)
        .bind(attempt_id)
        .execute(&pool)
        .await
        .unwrap();

        let stale_sync_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'flakes', 'flake_sync', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(stale_sync_id)
        .bind(flake_id.to_string())
        .bind(format!("flake:{flake_id}:{}", uuid::Uuid::new_v4()))
        .bind(serde_json::json!({"reason": "stale_sync"}))
        .execute(&pool)
        .await
        .unwrap();

        reconcile_errored_flakes(&pool).await;

        // The stale_sync occurrence must be resolved, and a new sync_error
        // occurrence must be open in its place.
        let stale_sync_resolved: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT resolved_at FROM attention_occurrences WHERE id = $1",
        )
        .bind(stale_sync_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            stale_sync_resolved.is_some(),
            "the mismatched-reason stale_sync occurrence must be resolved"
        );

        let sync_error_open: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL \
               AND metadata @> '{\"reason\": \"sync_error\"}'::jsonb",
        )
        .bind(flake_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            sync_error_open, 1,
            "a new sync_error occurrence must be opened once stale_sync no longer applies"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1",
        )
        .bind(flake_id.to_string())
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_errored_flakes_preserves_historical_failure_time() {
        // Regression test for round 8: recovering a lost attention
        // transition for an OLD failure must not open a fresh occurrence
        // timestamped "now" -- that would resurrect a days-old failure as
        // a brand-new 24-hour sidebar alert. opened_at must be derived
        // from the flake's own last_sync_at (the true failure time), not
        // from when the recovery sweep happened to run.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let attempt_id = uuid::Uuid::new_v4();
        let historical_failure_time = chrono::Utc::now() - chrono::Duration::days(3);

        sqlx::query(
            "UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2, last_sync_at = $3 WHERE id = $1",
        )
        .bind(flake_id)
        .bind(attempt_id)
        .bind(historical_failure_time)
        .execute(&pool)
        .await
        .unwrap();

        reconcile_errored_flakes(&pool).await;

        let opened_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT opened_at FROM attention_occurrences \
             WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(flake_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();

        let diff = (opened_at - historical_failure_time).num_seconds().abs();
        assert!(
            diff < 5,
            "recovered occurrence's opened_at ({opened_at}) must match the flake's \
             historical last_sync_at ({historical_failure_time}), not the current time"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1",
        )
        .bind(flake_id.to_string())
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_synced_flakes_missing_resolution_recovers_lingering_alert() {
        // Regression test for round 9: sync_flake_recorded commits
        // sync_status = 'synced' and only afterward calls
        // resolve_flake_attention_if_current as a separate best-effort
        // operation. If a crash happens between those two steps, a
        // successfully-synced flake is left with a stale open occurrence
        // that would otherwise remain a false alert for the rest of its
        // 24-hour attention window -- no other sweep examines `synced`
        // flakes.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let attempt_id = uuid::Uuid::new_v4();

        // Simulate: an earlier attempt opened a sync_error occurrence, then
        // THIS attempt succeeded (status committed) but its attention
        // resolution was lost to a simulated crash (never called).
        sqlx::query(
            "UPDATE flakes SET sync_status = 'synced', sync_attempt_id = $2, last_synced_at = now() WHERE id = $1",
        )
        .bind(flake_id)
        .bind(attempt_id)
        .execute(&pool)
        .await
        .unwrap();

        let stale_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'flakes', 'flake_sync', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(stale_id)
        .bind(flake_id.to_string())
        .bind(format!("flake:{flake_id}:{}", uuid::Uuid::new_v4()))
        .bind(serde_json::json!({"reason": "sync_error"}))
        .execute(&pool)
        .await
        .unwrap();

        reconcile_synced_flakes_missing_resolution(&pool).await;

        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(flake_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_count, 0,
            "the lingering occurrence for a now-synced flake must be resolved"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1",
        )
        .bind(flake_id.to_string())
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_synced_flakes_missing_resolution_skips_flake_with_no_open_occurrence() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let attempt_id = uuid::Uuid::new_v4();

        sqlx::query(
            "UPDATE flakes SET sync_status = 'synced', sync_attempt_id = $2, last_synced_at = now() WHERE id = $1",
        )
        .bind(flake_id)
        .bind(attempt_id)
        .execute(&pool)
        .await
        .unwrap();

        // No open occurrence exists -- the SELECT's EXISTS filter should
        // exclude this flake, so this call should be a cheap no-op.
        reconcile_synced_flakes_missing_resolution(&pool).await;

        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'flakes' AND subject_id = $1",
        )
        .bind(flake_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(open_count, 0, "no occurrence should have been created");

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Round 12: CVE reconciliation ─────────────────────────────────────

    /// Seed a critical CVE visible in `view_cve_list_with_metadata` with no
    /// open occurrence. Returns the CVE id and a cleanup token.
    async fn seed_critical_cve(pool: &sqlx::PgPool) -> (String, uuid::Uuid, uuid::Uuid) {
        let short = uuid::Uuid::new_v4().simple().to_string()[..11].to_string();
        let cve_id = format!("CVE-2026-{short}");

        // Environment (required by system).
        let env_id = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO environments (id, name) VALUES ($1, $2)")
            .bind(env_id)
            .bind(format!("cve-test-env-{short}"))
            .execute(pool)
            .await
            .unwrap();

        // System (use explicit id since we need it back).
        let system_hostname = format!("cve-host-{short}");
        let system_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO systems (id, hostname, environment_id, flake_id, is_active, public_key, derivation) \
             VALUES ($1, $2, $3, NULL, TRUE, $4, $5)",
        )
        .bind(system_id)
        .bind(&system_hostname)
        .bind(env_id)
        .bind(format!("ssh-ed25519 AAAA-cve-test-{short}"))
        .bind("/nix/store/cve-test-derivation")
        .execute(pool)
        .await
        .unwrap();

        // NixOS derivation for the system (derivation_name must match system hostname
        // for view_cve_list_with_metadata's affected_count computation).
        let nixos_derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, attempt_count) \
             VALUES (NULL, 'nixos', $1, 10, 0) RETURNING id",
        )
        .bind(&system_hostname)
        .fetch_one(pool)
        .await
        .unwrap();

        // Completed CVE scan.
        let scan_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO cve_scans (id, derivation_id, scanner_name, status, completed_at, \
                                     total_packages, total_vulnerabilities, critical_count) \
             VALUES ($1, $2, 'vulnix', 'completed', NOW(), 1, 1, 1)",
        )
        .bind(scan_id)
        .bind(nixos_derivation_id)
        .execute(pool)
        .await
        .unwrap();

        // Package derivation.
        let pkg_derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, pname, version, status_id, attempt_count) \
             VALUES (NULL, 'package', $1, 'test-pkg', '1.0.0', 11, 0) RETURNING id",
        )
        .bind(format!("cve-test-pkg-{short}"))
        .fetch_one(pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO scan_packages (scan_id, derivation_id) VALUES ($1, $2)")
            .bind(scan_id)
            .bind(pkg_derivation_id)
            .execute(pool)
            .await
            .unwrap();

        // The CVE: cvss_v3_score >= 9.0 => severity 'CRITICAL'.
        sqlx::query("INSERT INTO cves (id, cvss_v3_score) VALUES ($1, 9.8)")
            .bind(&cve_id)
            .execute(pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO package_vulnerabilities (derivation_id, cve_id, is_whitelisted) \
             VALUES ($1, $2, FALSE)",
        )
        .bind(pkg_derivation_id)
        .bind(&cve_id)
        .execute(pool)
        .await
        .unwrap();

        // Sanity check.
        let (severity, affected_count): (String, i64) = sqlx::query_as(
            "SELECT severity, affected_count FROM view_cve_list_with_metadata WHERE cve_id = $1",
        )
        .bind(&cve_id)
        .fetch_one(pool)
        .await
        .expect("view must return a row for the seeded CVE");
        assert_eq!(severity, "CRITICAL");
        assert!(affected_count > 0);

        (cve_id, system_id, env_id)
    }

    async fn cleanup_critical_cve(
        pool: &sqlx::PgPool,
        cve_id: &str,
        system_id: uuid::Uuid,
        env_id: uuid::Uuid,
    ) {
        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(cve_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM package_vulnerabilities WHERE cve_id = $1")
            .bind(cve_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM cves WHERE id = $1")
            .bind(cve_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM cve_scans WHERE derivation_id IN (SELECT id FROM derivations WHERE derivation_name LIKE 'cve-test-%')")
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM derivations WHERE derivation_name LIKE 'cve-test-%'")
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM systems WHERE id = $1")
            .bind(system_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM environments WHERE id = $1")
            .bind(env_id)
            .execute(pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_cve_attention_opens_missing_critical_occurrence() {
        let pool = test_pool().await;
        let (cve_id, system_id, env_id) = seed_critical_cve(&pool).await;

        // Verify: no occurrence exists initially.
        let before: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'cves' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(before, 0, "CVE must start with no open occurrence");

        reconcile_cve_attention(&pool).await;

        let after_first: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'cves' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            after_first, 1,
            "exactly one open CVE occurrence must be created by reconciler"
        );

        // Idempotency: running again must not create a second occurrence.
        reconcile_cve_attention(&pool).await;

        let after_second: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'cves' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            after_second, 1,
            "reconcile_cve_attention must be idempotent"
        );

        cleanup_critical_cve(&pool, &cve_id, system_id, env_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_cve_attention_persists_fleet_relevant_since() {
        let pool = test_pool().await;
        let (cve_id, system_id, env_id) = seed_critical_cve(&pool).await;

        reconcile_cve_attention(&pool).await;

        let (occurrence_opened, persisted_since): (Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
            "SELECT \
                 (SELECT opened_at FROM attention_occurrences \
                  WHERE category = 'cves' AND subject_id = $1 AND resolved_at IS NULL LIMIT 1), \
                 (SELECT fleet_relevant_since FROM cves WHERE id = $1)",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(
            occurrence_opened.is_some(),
            "occurrence must have opened_at"
        );
        assert!(
            persisted_since.is_some(),
            "cves.fleet_relevant_since must be persisted"
        );
        assert_eq!(
            occurrence_opened.unwrap(),
            persisted_since.unwrap(),
            "fleet_relevant_since must equal the occurrence opened_at"
        );

        cleanup_critical_cve(&pool, &cve_id, system_id, env_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_cve_attention_new_episode_after_resolution() {
        let pool = test_pool().await;
        let (cve_id, system_id, env_id) = seed_critical_cve(&pool).await;

        // First episode: CVE becomes fleet-relevant.
        reconcile_cve_attention(&pool).await;
        let first_opened: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT opened_at FROM attention_occurrences \
             WHERE category = 'cves' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let first_fleet_relevant_since: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT fleet_relevant_since FROM cves WHERE id = $1",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Make the CVE no longer relevant by whitelisting the vulnerability.
        sqlx::query(
            "UPDATE package_vulnerabilities SET is_whitelisted = TRUE, whitelist_reason = 'test' \
             WHERE cve_id = $1",
        )
        .bind(&cve_id)
        .execute(&pool)
        .await
        .unwrap();
        reconcile_cve_attention(&pool).await;

        let resolved_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'cves' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(resolved_count, 0, "CVE must be resolved after whitelisting");

        let cleared_since: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT fleet_relevant_since FROM cves WHERE id = $1",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(cleared_since.is_none(), "fleet_relevant_since must be cleared when resolved");

        // Second episode: remove whitelist and reconcile again.
        sqlx::query(
            "UPDATE package_vulnerabilities SET is_whitelisted = FALSE, whitelist_reason = NULL \
             WHERE cve_id = $1",
        )
        .bind(&cve_id)
        .execute(&pool)
        .await
        .unwrap();
        reconcile_cve_attention(&pool).await;

        let second_opened: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT opened_at FROM attention_occurrences \
             WHERE category = 'cves' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let second_fleet_relevant_since: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT fleet_relevant_since FROM cves WHERE id = $1",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(
            second_opened > first_opened,
            "new episode opened_at must be after the first"
        );
        assert!(
            second_fleet_relevant_since > first_fleet_relevant_since,
            "new episode fleet_relevant_since must be after the first"
        );
        assert_eq!(
            second_opened, second_fleet_relevant_since,
            "second episode opened_at must equal fleet_relevant_since"
        );

        cleanup_critical_cve(&pool, &cve_id, system_id, env_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_cve_attention_backfills_fleet_relevant_since_from_existing_occurrence() {
        let pool = test_pool().await;
        let (cve_id, system_id, env_id) = seed_critical_cve(&pool).await;

        // Pre-create an open occurrence as if it existed before migration 0182.
        let opened_at = chrono::Utc::now() - chrono::Duration::hours(2);
        let episode_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (category, subject_type, subject_id, \
             source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ('cves', 'cve', $1, $2, $3, $3, $4)",
        )
        .bind(&cve_id)
        .bind(attention::cve_occurrence_key(&cve_id, episode_id))
        .bind(opened_at)
        .bind(serde_json::json!({"reason": "critical", "cve_id": &cve_id}))
        .execute(&pool)
        .await
        .unwrap();

        // Simulate the pre-migration state where fleet_relevant_since is NULL.
        sqlx::query("UPDATE cves SET fleet_relevant_since = NULL WHERE id = $1")
            .bind(&cve_id)
            .execute(&pool)
            .await
            .unwrap();

        reconcile_cve_attention(&pool).await;

        let persisted_since: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT fleet_relevant_since FROM cves WHERE id = $1",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            persisted_since, opened_at,
            "fleet_relevant_since must be backfilled from the existing occurrence"
        );

        cleanup_critical_cve(&pool, &cve_id, system_id, env_id).await;
    }
}
