//! Server-owned remote CVE scan lease lifecycle.
//!
//! API builders receive exact scan inputs but never database access. The server
//! validates session ownership, canonicalizes schema-1 evidence, recomputes its
//! digest, and delegates persistence to the same transaction used by the local
//! Vulnix fallback.

use anyhow::{Context, Result, bail};
use cf_protocol::builder::{
    CVE_SCAN_MAX_BODY_BYTES, CVE_SCAN_MAX_ENTRIES, CVE_SCAN_MAX_OBSERVATIONS, CveDerivationOutput,
    CveObservation, CveScanClaim, CveScanCompleteRequest, CveScanDerivation, CveScanFailureClass,
    CveScanLease, CveScanPolicy, CveScanResult, CveScanSchemaVersion, CveScannerIdentity,
    canonical_cve_result_digest, is_canonical_nix_store_path,
};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use uuid::Uuid;

use crate::vulnix::process_group::{ScannerProcessGroup, isolate};
use crate::vulnix::vulnix_parser::VulnixEntry;

const LEASE_SECONDS: i64 = 120;
const MAX_STRING_CHARS: usize = 1024;
const MAX_PATH_CHARS: usize = 4096;
const MAX_SCANNER_ARGS: usize = 32;
pub(super) const MAX_FAILURE_CHARS: usize = 2048;
const MAX_DURATION_MS: u64 = 24 * 60 * 60 * 1000;
const CLOSURE_QUERY_SECONDS: u64 = 60;

/// Result of a canonical remote completion attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteCompletion {
    /// The evidence was validated and sealed by this request.
    Completed(String),
    /// The same canonical digest was sealed by an earlier request.
    AlreadyCompleted(String),
    /// The supplied digest does not match canonical evidence.
    DigestMismatch,
    /// A different result was already sealed for this scan.
    DigestConflict,
    /// Semantic evidence validation failed. The lease remains active.
    Invalid(String),
    /// The execution no longer owns an active lease.
    Stale,
}

/// Records capabilities for the authenticated current builder session.
///
/// Returns `false` without mutation if the builder is disabled, unregistered,
/// neither active nor offline, or the session is no longer current. An offline
/// builder can record capabilities before its authenticated heartbeat restores
/// the active state.
///
/// # Errors
///
/// Returns an error when capability persistence fails.
pub async fn record_session_cve_capabilities(
    pool: &PgPool,
    builder_id: Uuid,
    session_id: Uuid,
    capabilities: cf_protocol::builder::BuilderCapabilities,
) -> Result<bool> {
    // COMPATIBILITY: Unknown schema versions keep the builder session usable
    // but do not authorize CVE work during a rolling protocol upgrade.
    let supports_current_schema = capabilities.supports_current_cve_schema();
    let scanner = supports_current_schema
        .then(|| capabilities.cve_scanner.as_ref())
        .flatten();
    let result = sqlx::query(
        r#"
        UPDATE builders
        SET cve_scanning_enabled = $3,
            cve_scan_schema_version = $4,
            cve_scanner_name = $5,
            cve_scanner_version = $6,
            updated_at = NOW()
        WHERE id = $1 AND current_session_id = $2
          AND enabled AND registered AND status IN ('active', 'offline')
        "#,
    )
    .bind(builder_id)
    .bind(session_id)
    .bind(supports_current_schema)
    .bind(if supports_current_schema { 1 } else { 0 })
    .bind(scanner.map(|identity| identity.name.as_str()))
    .bind(scanner.map(|identity| identity.version.trim()))
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Creates durable post-build scan intent for admitted NixOS derivations.
///
/// ATOMICITY: Callers invoke this helper in the transaction that inserts the
/// build jobs, so each build admission and its scan intent commit or roll back
/// together. Eligibility is read from the persisted singleton policy and is
/// limited to `derivation_type = 'nixos'`.
///
/// IDEMPOTENCY: The active-scan partial index retains the identity and immutable
/// trigger of existing work. Manual and fleet work is unchanged. A replacement
/// build rebinds an unattempted `awaiting_build`, `awaiting_closure`, or
/// `pending` post-build intent to `awaiting_build` with the new prerequisite,
/// even when `on_build` is disabled. Policy controls new intent only. Only
/// derivation IDs returned by a successful build insert may be supplied.
///
/// # Errors
///
/// Returns an error when policy lookup or intent persistence fails.
pub(crate) async fn create_post_build_scan_intents_tx(
    tx: &mut Transaction<'_, Postgres>,
    derivation_ids: &[i32],
) -> Result<u64> {
    if derivation_ids.is_empty() {
        return Ok(0);
    }

    let changed: i64 = sqlx::query_scalar(
        r#"
        WITH admitted AS (
            SELECT job.id
                 , job.derivation_id
            FROM build_jobs job
            WHERE job.derivation_id = ANY($1)
              AND job.status IN ('queued', 'building', 'cancelling')
              AND NOT EXISTS (
                  SELECT 1
                  FROM build_jobs newer
                  WHERE newer.derivation_id = job.derivation_id
                    AND (newer.created_at, newer.id) > (job.created_at, job.id)
              )
        ), rebound AS (
            UPDATE cve_scans scan
            SET completed_build_job_id = admitted.id, status = 'awaiting_build'
            FROM admitted
            WHERE scan.derivation_id = admitted.derivation_id
              AND scan.source_trigger = 'post_build'
               AND scan.status IN ('awaiting_build', 'awaiting_closure', 'pending')
              AND scan.attempts = 0
              AND scan.completed_build_job_id IS DISTINCT FROM admitted.id
            RETURNING scan.id
        ), inserted AS (
            INSERT INTO cve_scans (
                id, derivation_id, scanner_name, status, attempts,
                source_trigger, completed_build_job_id
            )
            SELECT gen_random_uuid(), derivation.id, 'vulnix',
                   'awaiting_build', 0, 'post_build', admitted.id
            FROM admitted
            JOIN derivations derivation ON derivation.id = admitted.derivation_id
            JOIN scan_schedule_policy policy ON policy.id = 1 AND policy.on_build
            WHERE derivation.derivation_type = 'nixos'
            ON CONFLICT (derivation_id) WHERE status IN (
                'awaiting_build', 'awaiting_closure', 'pending', 'in_progress'
            ) DO NOTHING
            RETURNING id
        )
        SELECT COUNT(*)
        FROM (
            SELECT id FROM rebound
            UNION ALL
            SELECT id FROM inserted
        ) changed
        "#,
    )
    .bind(derivation_ids)
    .fetch_one(&mut **tx)
    .await?;
    Ok(changed as u64)
}

/// Reconciles durable post-build intents with authoritative build attempts.
///
/// The pass is bounded by `limit` and handles only zero-attempt
/// `post_build` scans in `awaiting_build`. Each candidate acquires the build
/// derivation lock before reading build history or locking the scan row. The
/// latest same-derivation attempt is authoritative. Active and successful
/// replacements rebind the intent. A latest failed or cancelled attempt
/// terminalizes the intent. An intent with no remaining build attempt fails as
/// unavailable. Current `on_build` policy does not affect an existing intent.
///
/// Guarded writes make repeated and concurrent passes idempotent. They also
/// prevent an old build event from changing an intent after replacement
/// rebinding.
///
/// # Errors
///
/// Returns an error when candidate discovery, locking, or persistence fails.
pub(crate) async fn reconcile_post_build_scan_prerequisites(
    pool: &PgPool,
    limit: i64,
) -> Result<i64> {
    if limit <= 0 {
        return Ok(0);
    }

    let candidates: Vec<(Uuid, i32)> = sqlx::query_as(
        r#"
        SELECT id, derivation_id
        FROM cve_scans
        WHERE source_trigger = 'post_build'
          AND status = 'awaiting_build'
          AND attempts = 0
        ORDER BY created_at, id
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut reconciled = 0;
    for (scan_id, derivation_id) in candidates {
        let mut tx = pool.begin().await?;
        crate::queries::build_jobs::lock_build_derivation(&mut tx, derivation_id).await?;

        let latest: Option<(Uuid, String)> = sqlx::query_as(
            r#"
            SELECT id, status
            FROM build_jobs
            WHERE derivation_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(derivation_id)
        .fetch_optional(&mut *tx)
        .await?;

        let changed = match latest {
            Some((job_id, status))
                if matches!(
                    status.as_str(),
                    "queued" | "building" | "cancelling" | "success"
                ) =>
            {
                sqlx::query(
                    r#"
                    UPDATE cve_scans
                    SET completed_build_job_id = $3
                    WHERE id = $1
                      AND derivation_id = $2
                      AND source_trigger = 'post_build'
                      AND status = 'awaiting_build'
                      AND attempts = 0
                      AND completed_build_job_id IS DISTINCT FROM $3
                      AND EXISTS (
                          SELECT 1
                          FROM build_jobs authoritative
                          WHERE authoritative.id = $3
                            AND authoritative.derivation_id = $2
                            AND authoritative.status IN (
                                'queued', 'building', 'cancelling', 'success'
                            )
                            AND NOT EXISTS (
                                SELECT 1
                                FROM build_jobs newer
                                WHERE newer.derivation_id = authoritative.derivation_id
                                  AND (newer.created_at, newer.id) >
                                      (authoritative.created_at, authoritative.id)
                            )
                      )
                    "#,
                )
                .bind(scan_id)
                .bind(derivation_id)
                .bind(job_id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
            }
            Some((job_id, status)) if matches!(status.as_str(), "failed" | "cancelled") => {
                let error = if status == "cancelled" {
                    "Prerequisite build was cancelled"
                } else {
                    "Prerequisite build failed"
                };
                sqlx::query(
                    r#"
                    UPDATE cve_scans
                    SET status = 'failed',
                        completed_at = NOW(),
                        completed_build_job_id = $3,
                        scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
                            || jsonb_build_object(
                                'error', $5::text,
                                'build_prerequisite', jsonb_build_object(
                                    'job_id', $3::uuid,
                                    'status', $4::text
                                )
                            )
                    WHERE id = $1
                      AND derivation_id = $2
                      AND source_trigger = 'post_build'
                      AND status = 'awaiting_build'
                      AND attempts = 0
                      AND EXISTS (
                          SELECT 1
                          FROM build_jobs authoritative
                          WHERE authoritative.id = $3
                            AND authoritative.derivation_id = $2
                            AND authoritative.status = $4
                            AND NOT EXISTS (
                                SELECT 1
                                FROM build_jobs newer
                                WHERE newer.derivation_id = authoritative.derivation_id
                                  AND (newer.created_at, newer.id) >
                                      (authoritative.created_at, authoritative.id)
                            )
                      )
                    "#,
                )
                .bind(scan_id)
                .bind(derivation_id)
                .bind(job_id)
                .bind(&status)
                .bind(error)
                .execute(&mut *tx)
                .await?
                .rows_affected()
            }
            None => sqlx::query(
                r#"
                UPDATE cve_scans
                SET status = 'failed',
                    completed_at = NOW(),
                    completed_build_job_id = NULL,
                    scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
                        || jsonb_build_object(
                            'error', 'Build prerequisite is unavailable',
                            'build_prerequisite', jsonb_build_object(
                                'status', 'unavailable'
                            )
                        )
                WHERE id = $1
                  AND derivation_id = $2
                  AND source_trigger = 'post_build'
                  AND status = 'awaiting_build'
                  AND attempts = 0
                  AND NOT EXISTS (
                      SELECT 1 FROM build_jobs WHERE derivation_id = $2
                  )
                "#,
            )
            .bind(scan_id)
            .bind(derivation_id)
            .execute(&mut *tx)
            .await?
            .rows_affected(),
            Some(_) => 0,
        };

        tx.commit().await?;
        reconciled += changed as i64;
    }

    Ok(reconciled)
}

/// Terminalizes persisted post-build obligations past their build-time window.
///
/// CONCURRENCY: Build admission and prerequisite reconciliation take the build
/// derivation lock first. The POA&M derivation lock follows it, before the scan
/// row is updated. A live local or remote execution is never revoked here;
/// recovery can finish it through its existing owner/lease protocol. Failed
/// attempts retain their original error and completion time in metadata, while
/// the expiration marker prevents the legacy selector from recreating work.
/// No row is synthesized for an old build that never had intent.
///
/// # Errors
///
/// Returns an error if candidate selection, locking, or persistence fails.
pub(crate) async fn expire_post_build_scan_obligations(pool: &PgPool, limit: i64) -> Result<i64> {
    if limit <= 0 {
        return Ok(0);
    }
    let candidates: Vec<(Uuid, i32)> = sqlx::query_as(
        r#"
        SELECT scan.id, scan.derivation_id
        FROM cve_scans scan
        JOIN build_jobs job ON job.id = scan.completed_build_job_id
                             AND job.derivation_id = scan.derivation_id
        JOIN scan_schedule_policy policy ON policy.id = 1
        WHERE scan.source_trigger = 'post_build'
          AND scan.status IN ('awaiting_build', 'awaiting_closure', 'pending', 'failed')
          AND scan.scan_metadata ->> 'terminal_reason' IS DISTINCT FROM
              'post_build_recovery_window_expired'
          AND job.status = 'success' AND job.completed_at IS NOT NULL
          AND job.completed_at <= NOW() - policy.post_build_recovery_window::interval
          AND NOT EXISTS (
              SELECT 1 FROM build_jobs newer
              WHERE newer.derivation_id = job.derivation_id
                AND (newer.created_at, newer.id) > (job.created_at, job.id)
          )
          AND NOT EXISTS (
              SELECT 1 FROM cve_scans evidence
              WHERE evidence.derivation_id = scan.derivation_id
                AND evidence.id <> scan.id
                AND (evidence.status IN (
                    'awaiting_build', 'awaiting_closure', 'pending', 'in_progress'
                ) OR (evidence.status = 'completed'
                    AND evidence.completed_at > job.completed_at
                    AND evidence.completed_at <= job.completed_at
                        + policy.post_build_recovery_window::interval)
                    OR (evidence.source_trigger = 'post_build'
                    AND (evidence.created_at, evidence.id) > (scan.created_at, scan.id)))
          )
        ORDER BY job.completed_at, scan.id
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut expired = 0;
    for (scan_id, derivation_id) in candidates {
        let mut tx = pool.begin().await?;
        crate::queries::build_jobs::lock_build_derivation(&mut tx, derivation_id).await?;
        crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
            &mut tx,
            derivation_id,
            &[],
        )
        .await?;
        let changed = sqlx::query(
            r#"
            UPDATE cve_scans scan
            SET status = 'failed', completed_at = NOW(),
                scan_metadata = COALESCE(scan.scan_metadata, '{}'::jsonb)
                    || jsonb_build_object(
                        'terminal_reason', 'post_build_recovery_window_expired',
                        'error', 'Post-build scan not completed. The recovery window expired before a successful scan was recorded.',
                        'completed_build_at', job.completed_at,
                        'recovery_deadline_at', job.completed_at
                            + policy.post_build_recovery_window::interval,
                        'last_attempt_at', scan.completed_at,
                        'last_failure', scan.scan_metadata ->> 'error'
                    )
            FROM build_jobs job, scan_schedule_policy policy
            WHERE scan.id = $1 AND scan.derivation_id = $2
              AND scan.completed_build_job_id = job.id
              AND job.derivation_id = scan.derivation_id
              AND policy.id = 1 AND job.status = 'success'
              AND job.completed_at IS NOT NULL
              AND job.completed_at <= NOW() - policy.post_build_recovery_window::interval
              AND scan.source_trigger = 'post_build'
              AND scan.status IN ('awaiting_build', 'awaiting_closure', 'pending', 'failed')
              AND scan.scan_metadata ->> 'terminal_reason' IS DISTINCT FROM
                  'post_build_recovery_window_expired'
              AND NOT EXISTS (
                  SELECT 1 FROM build_jobs newer
                  WHERE newer.derivation_id = job.derivation_id
                    AND (newer.created_at, newer.id) > (job.created_at, job.id)
              )
              AND NOT EXISTS (
                  SELECT 1 FROM cve_scans other
                  WHERE other.derivation_id = scan.derivation_id
                    AND other.id <> scan.id
                    AND (other.status IN (
                        'awaiting_build', 'awaiting_closure', 'pending', 'in_progress'
                    ) OR (other.status = 'completed'
                        AND other.completed_at > job.completed_at
                        AND other.completed_at <= job.completed_at
                            + policy.post_build_recovery_window::interval)
                        OR (other.source_trigger = 'post_build'
                        AND (other.created_at, other.id) > (scan.created_at, scan.id)))
              )
            "#,
        )
        .bind(scan_id)
        .bind(derivation_id)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() == 1 {
            crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, scan_id)
                .await?;
            tx.commit().await?;
            expired += 1;
        } else {
            tx.rollback().await?;
        }
    }
    Ok(expired)
}

/// Confirms successful build provenance for active post-build scan intent.
///
/// The helper does not mutate scan status. Newly admitted or repaired intent
/// therefore remains `awaiting_build`; only
/// [`promote_waiting_cve_scans`](crate::queries::cve_scans::promote_waiting_cve_scans)
/// owns prerequisite-driven lifecycle transitions. Existing active manual or
/// fleet work retains its trigger and provenance. The exact prerequisite guard
/// prevents a delayed completion retry from replacing a newer admitted build.
/// Completion repairs a missing intent only for an eligible NixOS derivation
/// with no scan history. This compatibility path covers build jobs admitted
/// before atomic intent creation. It does not create fresh work after terminal
/// evidence or replace active manual or fleet provenance.
///
/// # Errors
///
/// Returns an error when build lookup or provenance persistence fails.
pub(crate) async fn attach_completed_build_to_post_build_scan_tx(
    tx: &mut Transaction<'_, Postgres>,
    build_job_id: Uuid,
) -> Result<bool> {
    let attached = sqlx::query_scalar::<_, Uuid>(
        r#"
        WITH repaired AS (
            INSERT INTO cve_scans (
                id, derivation_id, scanner_name, status, attempts,
                source_trigger, completed_build_job_id
            )
            SELECT
                gen_random_uuid(), derivation.id, 'vulnix', 'awaiting_build', 0,
                'post_build', job.id
            FROM build_jobs job
            JOIN derivations derivation ON derivation.id = job.derivation_id
            JOIN scan_schedule_policy policy ON policy.id = 1 AND policy.on_build
            WHERE job.id = $1
              AND job.status = 'success'
              AND derivation.derivation_type = 'nixos'
              AND NOT EXISTS (
                  SELECT 1
                  FROM cve_scans history
                  WHERE history.derivation_id = derivation.id
              )
            ON CONFLICT (derivation_id) WHERE status IN (
                'awaiting_build', 'awaiting_closure', 'pending', 'in_progress'
            ) DO NOTHING
            RETURNING id
        ), attached AS (
            UPDATE cve_scans scan
            SET completed_build_job_id = job.id
            FROM build_jobs job
            WHERE job.id = $1 AND job.status = 'success'
              AND scan.derivation_id = job.derivation_id
              AND scan.source_trigger = 'post_build'
              AND scan.status = 'awaiting_build'
              AND scan.completed_build_job_id = job.id
            RETURNING scan.id
        )
        SELECT id FROM repaired
        UNION ALL
        SELECT id FROM attached
        LIMIT 1
        "#,
    )
    .bind(build_job_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(attached.is_some())
}

/// Fails the exact post-build scan intent blocked on a terminal build attempt.
///
/// ATOMICITY: The caller must invoke this helper in the transaction that makes
/// the build terminal. Build lifecycle paths lock the derivation and build job
/// before this scan update, which preserves the existing lock order.
///
/// The exact job identity, `post_build` trigger, `awaiting_build` state, and
/// zero-attempt guard prevent a delayed terminal event from failing a scan that
/// was rebound to a replacement build or started by another trigger. A
/// `cancelling` build is not terminal and cannot satisfy the update.
///
/// # Errors
///
/// Returns an error when the guarded scan update fails.
pub(crate) async fn fail_post_build_scan_for_terminal_build_tx(
    tx: &mut Transaction<'_, Postgres>,
    build_job_id: Uuid,
    failure_detail: Option<&str>,
) -> Result<bool> {
    let failure_detail = failure_detail.map(sanitize_failure);
    let failed_scan = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE cve_scans scan
        SET status = 'failed',
            completed_at = NOW(),
            scan_metadata = COALESCE(scan.scan_metadata, '{}'::jsonb)
                || jsonb_build_object(
                    'error', CASE job.status
                        WHEN 'cancelled' THEN 'Prerequisite build was cancelled'
                        ELSE 'Prerequisite build failed'
                    END,
                    'build_prerequisite', jsonb_strip_nulls(jsonb_build_object(
                        'job_id', job.id,
                        'status', job.status,
                        'message', $2::text
                    ))
                )
        FROM build_jobs job
        WHERE job.id = $1
          AND job.status IN ('failed', 'cancelled')
          AND scan.completed_build_job_id = job.id
          AND scan.source_trigger = 'post_build'
          AND scan.status = 'awaiting_build'
          AND scan.attempts = 0
        RETURNING scan.id
        "#,
    )
    .bind(build_job_id)
    .bind(failure_detail)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(failed_scan.is_some())
}

/// Claims one queued scan for an authenticated scanner-capable builder.
///
/// CONCURRENCY: The transaction locks the builder row first, then acquires the
/// established POA&M derivation lock before mutating `cve_scans`. It tries
/// the build derivation lock without waiting after the builder row: build
/// completion can hold that lock while waiting for the builder row. A failed
/// try-lock releases the builder row and defers the claim. The guarded
/// pending-to-in-progress update and
/// the unique active-builder index enforce one winner and one scan per builder.
/// Build work has priority: a builder with assigned active work, or while queued
/// build work exists, receives no background scan lease.
///
/// # Errors
///
/// Returns an error for a stale session, an inactive builder, or a
/// database/persistence failure.
pub async fn claim_remote_cve_scan(
    pool: &PgPool,
    builder_id: Uuid,
    session_id: Uuid,
    completed_build_job_id: Option<Uuid>,
) -> Result<Option<CveScanClaim>> {
    let mut tx = pool.begin().await?;
    let capability: Option<(Option<Uuid>, bool, bool, String, bool, i32)> = sqlx::query_as(
        r#"
        SELECT current_session_id, enabled, registered, status,
               cve_scanning_enabled, cve_scan_schema_version
        FROM builders WHERE id = $1 FOR UPDATE
        "#,
    )
    .bind(builder_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((current_session, enabled, registered, status, cve_enabled, schema)) = capability
    else {
        bail!("builder_not_found");
    };
    if current_session != Some(session_id) {
        bail!("builder_session_mismatch");
    }
    if !enabled || !registered || status != "active" {
        bail!("builder_inactive");
    }
    if !cve_enabled || schema != 1 {
        tx.rollback().await?;
        return Ok(None);
    }

    let blocked: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM cve_scans
            WHERE lease_builder_id = $1 AND status = 'in_progress'
        ) OR EXISTS (
            SELECT 1 FROM build_jobs
            WHERE builder_id = $1 AND status = 'building'
        ) OR ($2::uuid IS NULL AND EXISTS (
            SELECT 1 FROM build_jobs WHERE status = 'queued'
        ))
        "#,
    )
    .bind(builder_id)
    .bind(completed_build_job_id)
    .fetch_one(&mut *tx)
    .await?;
    if blocked {
        tx.rollback().await?;
        return Ok(None);
    }

    // Candidate selection is unlocked. The POA&M lock must precede the guarded
    // scan-row update to preserve the global CVE writer lock order.
    let candidate: Option<(Uuid, i32, String)> = sqlx::query_as(
        r#"
        WITH builder_environments AS (
            SELECT environment_id
            FROM builder_environment_assignments
            WHERE builder_id = $1
        )
        SELECT scan.id, scan.derivation_id,
               COALESCE(scan.source_trigger, 'legacy') AS source_trigger
        FROM cve_scans scan
        LEFT JOIN build_jobs affinity ON affinity.id = scan.completed_build_job_id
        WHERE scan.status = 'pending'
          AND (
              NOT EXISTS (SELECT 1 FROM builder_environments)
              OR affinity.environment_id IN (SELECT environment_id FROM builder_environments)
              OR (affinity.environment_id IS NULL AND NOT EXISTS (
                  SELECT 1
                  FROM systems system
                  JOIN commits commit ON commit.flake_id = system.flake_id
                  JOIN derivations scoped_derivation
                    ON scoped_derivation.commit_id = commit.id
                   AND scoped_derivation.derivation_type = 'nixos'
                   AND scoped_derivation.derivation_name = COALESCE(
                       NULLIF(BTRIM(system.system_configuration_name), ''),
                       system.hostname
                   )
                  WHERE scoped_derivation.id = scan.derivation_id
                    AND system.environment_id NOT IN (
                        SELECT environment_id FROM builder_environments
                    )
              ))
          )
          AND affinity.builder_id = $1
          AND affinity.builder_session_id = $3
          AND affinity.status = 'success'
          AND ($2::uuid IS NULL OR scan.completed_build_job_id = $2)
        ORDER BY
          CASE WHEN scan.completed_build_job_id = $2 THEN 0 ELSE 1 END,
          scan.created_at, scan.id
        LIMIT 1
        "#,
    )
    .bind(builder_id)
    .bind(completed_build_job_id)
    .bind(session_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((scan_id, derivation_id, source_trigger)) = candidate else {
        tx.rollback().await?;
        return Ok(None);
    };

    // CONCURRENCY: This is the build derivation lock namespace used by
    // build_jobs::lock_build_derivation. Do not wait for it while holding the
    // builder row, since admission can hold it while waiting for that row.
    if source_trigger == "post_build" {
        let build_uncontended: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1::integer, $2::integer)")
                .bind(crate::queries::build_jobs::BUILD_DERIVATION_LOCK_NAMESPACE)
                .bind(derivation_id)
                .fetch_one(&mut *tx)
                .await?;
        if !build_uncontended {
            tx.rollback().await?;
            return Ok(None);
        }
    }

    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        &mut tx,
        derivation_id,
        &[],
    )
    .await?;
    let execution_id = Uuid::new_v4();
    let lease_expires_at = Utc::now() + chrono::Duration::seconds(LEASE_SECONDS);
    let policy = CveScanPolicy {
        max_body_bytes: CVE_SCAN_MAX_BODY_BYTES,
        max_entries: CVE_SCAN_MAX_ENTRIES,
        max_observations: CVE_SCAN_MAX_OBSERVATIONS,
        timeout_seconds: 3600,
        scanner_args: vec!["--json".to_string()],
    };
    let policy_json = serde_json::to_value(&policy)?;
    let row = sqlx::query(
        r#"
        UPDATE cve_scans scan
        SET status = 'in_progress', attempts = attempts + 1,
            completed_at = NULL, execution_outcome = NULL,
            failure_class = NULL,
            execution_id = $2, lease_builder_id = $3,
            lease_builder_session_id = $4, lease_started_at = NOW(),
            lease_heartbeat_at = NOW(), lease_expires_at = $5,
            scanner_policy = $6,
            scanner_name = builder.cve_scanner_name,
            scanner_version = builder.cve_scanner_version,
            target_drv_path = derivation.derivation_path,
            target_outputs = jsonb_build_array(jsonb_build_object(
                'name', 'out', 'store_path', derivation.store_path
            )),
            scan_metadata = COALESCE(scan.scan_metadata, '{}'::jsonb)
                || jsonb_build_object('remote_execution', true)
        FROM derivations derivation, builders builder
        WHERE scan.id = $1 AND scan.status = 'pending'
          AND (scan.source_trigger IS DISTINCT FROM 'post_build' OR EXISTS (
              SELECT 1 FROM build_jobs prerequisite
              JOIN scan_schedule_policy schedule ON schedule.id = 1
              WHERE prerequisite.id = scan.completed_build_job_id
                AND prerequisite.derivation_id = scan.derivation_id
                 AND prerequisite.status = 'success'
                 AND prerequisite.completed_at > NOW()
                     - schedule.post_build_recovery_window::interval
                 AND NOT EXISTS (
                     SELECT 1 FROM build_jobs newer
                     WHERE newer.derivation_id = prerequisite.derivation_id
                       AND (newer.created_at, newer.id) >
                           (prerequisite.created_at, prerequisite.id)
                 )
          ))
          AND derivation.id = scan.derivation_id
          AND builder.id = $3
          AND builder.enabled AND builder.registered AND builder.status = 'active'
          AND builder.current_session_id = $4
          AND builder.cve_scanning_enabled
          AND builder.cve_scan_schema_version = 1
          AND derivation.derivation_path ~ '^/nix/store/[^/]+[.]drv$'
          AND derivation.store_path ~ '^/nix/store/[^/]+$'
        RETURNING scan.derivation_id, derivation.derivation_name,
                  derivation.derivation_path, derivation.store_path,
                   scan.scanner_name, scan.scanner_version
        "#,
    )
    .bind(scan_id)
    .bind(execution_id)
    .bind(builder_id)
    .bind(session_id)
    .bind(lease_expires_at)
    .bind(policy_json)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(None);
    };
    let drv_path: String = row.get("derivation_path");
    let store_path: String = row.get("store_path");
    if !is_canonical_nix_store_path(&drv_path, true)
        || !is_canonical_nix_store_path(&store_path, false)
    {
        tx.rollback().await?;
        bail!("persisted scan target has a non-canonical Nix store path");
    }
    crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, scan_id).await?;
    tx.commit().await?;

    let output = CveDerivationOutput {
        name: "out".to_string(),
        store_path,
    };
    Ok(Some(CveScanClaim {
        lease: CveScanLease {
            scan_id,
            execution_id,
            builder_id,
            builder_session_id: session_id,
        },
        derivation: CveScanDerivation {
            derivation_id: row.get("derivation_id"),
            derivation_name: row.get("derivation_name"),
            drv_path,
            outputs: vec![output],
        },
        schema_version: CveScanSchemaVersion::V1,
        scanner: CveScannerIdentity {
            name: row.get("scanner_name"),
            version: row.get("scanner_version"),
        },
        policy,
        cache_source: None,
        lease_expires_at,
    }))
}

/// Renews an owned remote lease and returns its new expiration.
///
/// # Errors
///
/// Returns an error when the database update fails. A stale lease returns
/// `Ok(None)` and must stop scanning.
pub async fn heartbeat_remote_cve_scan(
    pool: &PgPool,
    lease: CveScanLease,
    entries: usize,
    observations: usize,
    diagnostics: &[cf_protocol::builder::CveScanDiagnostic],
) -> Result<Option<DateTime<Utc>>> {
    if entries > CVE_SCAN_MAX_ENTRIES || observations > CVE_SCAN_MAX_OBSERVATIONS {
        return Ok(None);
    }
    let expires = Utc::now() + chrono::Duration::seconds(LEASE_SECONDS);
    let diagnostics = crate::queries::cve_scan_diagnostics::prepare_diagnostics(diagnostics);
    let mut tx = pool.begin().await?;
    // CONCURRENCY: The guarded lease update locks the scan row before diagnostics
    // are appended. The transaction acknowledges a heartbeat only after both the
    // lease renewal and its fenced phase events commit.
    let updated = sqlx::query_scalar::<_, DateTime<Utc>>(
        r#"
        UPDATE cve_scans scan SET lease_heartbeat_at = NOW(), lease_expires_at = $5
        FROM builders builder
        WHERE scan.id = $1 AND scan.execution_id = $2
          AND scan.lease_builder_id = $3 AND scan.lease_builder_session_id = $4
          AND scan.status = 'in_progress' AND scan.lease_expires_at > NOW()
          AND builder.id = $3 AND builder.current_session_id = $4
          AND builder.enabled AND builder.registered AND builder.status = 'active'
          AND builder.cve_scanning_enabled AND builder.cve_scan_schema_version = 1
        RETURNING scan.lease_expires_at
        "#,
    )
    .bind(lease.scan_id)
    .bind(lease.execution_id)
    .bind(lease.builder_id)
    .bind(lease.builder_session_id)
    .bind(expires)
    .fetch_optional(&mut *tx)
    .await?;
    if updated.is_some() && !diagnostics.is_empty() {
        crate::queries::cve_scan_diagnostics::append_remote_diagnostics_tx(
            &mut tx,
            lease,
            &diagnostics,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(updated)
}

/// Requeues expired or superseded remote leases for another scanner.
///
/// CONCURRENCY: Each candidate takes the POA&M derivation lock before the
/// guarded scan mutation. The exact execution token and expired/replaced
/// session predicate fence a heartbeat or successor claim that wins the race.
/// Clearing remote ownership and sealed claim inputs makes the row eligible for
/// the local fallback. The prior remote identity remains in audit metadata.
///
/// # Errors
///
/// Returns an error when recovery or composite persistence fails.
pub async fn requeue_expired_remote_cve_scans(pool: &PgPool, limit: i64) -> Result<i64> {
    let candidates: Vec<(Uuid, i32, Uuid)> = sqlx::query_as(
        r#"
        SELECT scan.id, scan.derivation_id, scan.execution_id
        FROM cve_scans scan
        WHERE scan.status = 'in_progress' AND scan.execution_id IS NOT NULL
          AND (scan.lease_builder_id IS NULL
               OR scan.lease_expires_at <= NOW()
               OR NOT EXISTS (
                    SELECT 1 FROM builders builder
                    WHERE builder.id = scan.lease_builder_id
                      AND builder.current_session_id = scan.lease_builder_session_id
                      AND builder.enabled AND builder.registered
                      AND builder.status = 'active'
                      AND builder.cve_scanning_enabled
                      AND builder.cve_scan_schema_version = 1
               ))
        ORDER BY scan.lease_expires_at, scan.id LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let mut recovered = 0;
    for (scan_id, derivation_id, execution_id) in candidates {
        let mut tx = pool.begin().await?;
        crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
            &mut tx,
            derivation_id,
            &[],
        )
        .await?;
        let changed = sqlx::query(
            r#"
            UPDATE cve_scans scan
            SET status = 'pending', execution_id = NULL,
                execution_outcome = NULL, failure_class = NULL,
                lease_builder_id = NULL, lease_builder_session_id = NULL,
                lease_started_at = NULL, lease_heartbeat_at = NULL,
                lease_expires_at = NULL, scanner_policy = NULL,
                target_drv_path = NULL, target_outputs = NULL,
                scanner_version = NULL,
                scan_metadata = (
                    COALESCE(scan.scan_metadata, '{}'::jsonb)
                    - 'remote_execution'
                    - 'execution_id'
                    - 'execution_started_at'
                    - 'execution_heartbeat_at'
                    - 'execution_revoked_at'
                ) || jsonb_build_object(
                    'requeue_reason', 'remote-lease-expired-or-session-replaced',
                    'last_remote_execution', jsonb_build_object(
                        'execution_id', scan.execution_id,
                        'builder_id', scan.lease_builder_id,
                        'builder_session_id', scan.lease_builder_session_id,
                        'scanner_name', scan.scanner_name,
                        'scanner_version', scan.scanner_version,
                        'requeued_at', NOW()
                    )
                )
            WHERE scan.id = $1 AND scan.execution_id = $2
              AND scan.status = 'in_progress'
              AND (scan.lease_builder_id IS NULL
                   OR scan.lease_expires_at <= NOW()
                   OR NOT EXISTS (
                        SELECT 1 FROM builders builder
                        WHERE builder.id = scan.lease_builder_id
                          AND builder.current_session_id = scan.lease_builder_session_id
                          AND builder.enabled AND builder.registered
                          AND builder.status = 'active'
                          AND builder.cve_scanning_enabled
                          AND builder.cve_scan_schema_version = 1
                   ))
            "#,
        )
        .bind(scan_id)
        .bind(execution_id)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() == 1 {
            crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, scan_id)
                .await?;
            tx.commit().await?;
            recovered += 1;
        } else {
            tx.rollback().await?;
        }
    }
    Ok(recovered)
}

/// Validates, canonicalizes, hashes, and persists a remote schema-1 result.
///
/// Invalid semantic payloads return [`RemoteCompletion::Invalid`] without any
/// database mutation, so the active lease can be corrected and retried.
///
/// # Errors
///
/// Returns an error only for database or canonical persistence failures.
pub async fn complete_remote_cve_scan(
    pool: &PgPool,
    request: CveScanCompleteRequest,
) -> Result<RemoteCompletion> {
    let lease = request.lease;
    let diagnostics =
        crate::queries::cve_scan_diagnostics::prepare_diagnostics(&request.diagnostics);
    let existing: Option<String> = sqlx::query_scalar(
        r#"
        SELECT result_digest_sha256
        FROM cve_scans
        WHERE id = $1 AND status = 'completed'
          AND execution_id = $2 AND lease_builder_id = $3
          AND lease_builder_session_id = $4
          AND result_digest_sha256 IS NOT NULL
          AND EXISTS (
              SELECT 1 FROM builders builder
              WHERE builder.id = $3 AND builder.current_session_id = $4
                AND builder.enabled AND builder.registered
                AND builder.status = 'active'
                AND builder.cve_scanning_enabled
                AND builder.cve_scan_schema_version = 1
          )
        "#,
    )
    .bind(lease.scan_id)
    .bind(lease.execution_id)
    .bind(lease.builder_id)
    .bind(lease.builder_session_id)
    .fetch_optional(pool)
    .await?;
    if let Some(digest) = existing {
        return Ok(if digest == request.result_digest_sha256 {
            RemoteCompletion::AlreadyCompleted(digest)
        } else {
            RemoteCompletion::DigestConflict
        });
    }

    let claim = load_owned_claim(pool, lease).await?;
    let Some(claim) = claim else {
        return Ok(RemoteCompletion::Stale);
    };
    if request.scan_duration_ms > MAX_DURATION_MS {
        return Ok(RemoteCompletion::Invalid(
            "scan duration exceeds limit".into(),
        ));
    }
    let canonical = match canonicalize_result(request.result, &claim) {
        Ok(result) => result,
        Err(error) => return Ok(RemoteCompletion::Invalid(error.to_string())),
    };
    let closure_provenance = match validate_available_target_closure(&canonical).await {
        Ok(provenance) => provenance,
        Err(error) => return Ok(RemoteCompletion::Invalid(error.to_string())),
    };
    let digest = canonical_cve_result_digest(&canonical)?;
    if request.result_digest_sha256 != digest {
        return Ok(RemoteCompletion::DigestMismatch);
    }
    let (entries, store_paths) = result_to_vulnix(&canonical)?;
    if let Err(error) = crate::queries::cve_scans::save_remote_scan_results_for_execution(
        pool,
        lease.scan_id,
        &entries,
        i32::try_from(request.scan_duration_ms).ok(),
        &store_paths,
        lease,
        &digest,
        closure_provenance.as_str(),
        &diagnostics,
    )
    .await
    {
        if error.to_string().contains("lost execution ownership") {
            return Ok(RemoteCompletion::Stale);
        }
        return Err(error);
    }
    Ok(RemoteCompletion::Completed(digest))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClosureProvenance {
    ServerLocalVerified,
    UnverifiedRemote,
}

impl ClosureProvenance {
    fn as_str(self) -> &'static str {
        match self {
            Self::ServerLocalVerified => "server_local_verified",
            Self::UnverifiedRemote => "unverified_remote",
        }
    }
}

/// Binds package output and deriver evidence to a server-materialized target.
///
/// SECURITY: A locally available target is verified against its Nix closure.
/// Every submitted package output that is locally available must report the
/// submitted package `.drv` as its exact deriver, even when target closure
/// membership is unavailable. If any authorized target output is unavailable,
/// the server records `unverified_remote`; it does not represent signed builder
/// evidence as equivalent to server-local closure verification.
async fn validate_available_target_closure(result: &CveScanResult) -> Result<ClosureProvenance> {
    let targets_available = result
        .derivation
        .outputs
        .iter()
        .all(|output| std::path::Path::new(&output.store_path).exists());
    let locally_available_package_outputs = result
        .entries
        .iter()
        .flat_map(|entry| &entry.outputs)
        .filter(|output| std::path::Path::new(&output.store_path).exists())
        .map(|output| output.store_path.clone())
        .collect::<HashSet<_>>();
    validate_target_closure_with_program(
        result,
        std::ffi::OsStr::new("nix-store"),
        targets_available,
        &locally_available_package_outputs,
    )
    .await
}

async fn validate_target_closure_with_program(
    result: &CveScanResult,
    nix_store_program: &std::ffi::OsStr,
    targets_available: bool,
    locally_available_package_outputs: &HashSet<String>,
) -> Result<ClosureProvenance> {
    let package_outputs = result
        .entries
        .iter()
        .flat_map(|entry| {
            entry
                .outputs
                .iter()
                .map(move |output| (output.store_path.as_str(), entry.drv_path.as_str()))
        })
        .collect::<Vec<_>>();
    if targets_available {
        let mut args = vec!["--query".to_string(), "--requisites".to_string()];
        args.extend(
            result
                .derivation
                .outputs
                .iter()
                .map(|output| output.store_path.clone()),
        );
        let output = run_nix_store(nix_store_program, &args).await?;
        if !output.status.success() {
            bail!("failed to validate the authorized target closure");
        }
        let closure_output =
            String::from_utf8(output.stdout).context("Nix returned non-UTF-8 closure evidence")?;
        let closure = closure_output
            .lines()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .collect::<HashSet<_>>();
        if package_outputs
            .iter()
            .any(|(output, _)| !closure.contains(output))
        {
            bail!("package output is not in the authorized target closure");
        }
    }
    let locally_available = package_outputs
        .into_iter()
        .filter(|(output, _)| locally_available_package_outputs.contains(*output))
        .collect::<Vec<_>>();
    for chunk in locally_available.chunks(128) {
        let mut args = vec!["--query".to_string(), "--deriver".to_string()];
        args.extend(chunk.iter().map(|(output, _)| (*output).to_string()));
        let output = run_nix_store(nix_store_program, &args).await?;
        if !output.status.success() {
            bail!("failed to validate package output derivers");
        }
        let reported = String::from_utf8(output.stdout)
            .context("Nix returned non-UTF-8 package deriver evidence")?;
        let reported = reported.lines().map(str::trim).collect::<Vec<_>>();
        if reported.len() != chunk.len()
            || reported
                .iter()
                .zip(chunk)
                .any(|(actual, (_, expected))| actual != expected)
        {
            bail!("package output deriver does not match submitted package drv path");
        }
    }
    Ok(if targets_available {
        ClosureProvenance::ServerLocalVerified
    } else {
        ClosureProvenance::UnverifiedRemote
    })
}

async fn run_nix_store(program: &std::ffi::OsStr, args: &[String]) -> Result<std::process::Output> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate(&mut command);
    let child = command
        .spawn()
        .context("failed to start Nix closure validation")?;
    let mut group = ScannerProcessGroup::new(child, "CVE closure validation")?;
    let stdout = group
        .child_mut()
        .stdout
        .take()
        .context("Nix closure validation stdout was not piped")?;
    let stderr = group
        .child_mut()
        .stderr
        .take()
        .context("Nix closure validation stderr was not piped")?;
    let mut stdout_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout
            .take(CVE_SCAN_MAX_BODY_BYTES + 1)
            .read_to_end(&mut bytes)
            .await
            .map(|_| bytes)
    });
    let mut stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr
            .take(64 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .map(|_| bytes)
    });
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(CLOSURE_QUERY_SECONDS));
    tokio::pin!(deadline);
    let status = tokio::select! {
        status = group.wait() => status.context("failed to wait for Nix closure validation")?,
        _ = &mut deadline => {
            group.terminate().await;
            stdout_task.abort();
            stderr_task.abort();
            bail!("timed out while validating the authorized target closure");
        }
    };
    let stdout = tokio::select! {
        result = &mut stdout_task => result.context("Nix stdout reader failed")??,
        _ = &mut deadline => {
            group.terminate().await;
            stdout_task.abort();
            stderr_task.abort();
            bail!("timed out while draining Nix closure validation output");
        }
    };
    let stderr = tokio::select! {
        result = &mut stderr_task => result.context("Nix stderr reader failed")??,
        _ = &mut deadline => {
            group.terminate().await;
            stderr_task.abort();
            bail!("timed out while draining Nix closure validation output");
        }
    };
    if stdout.len() > CVE_SCAN_MAX_BODY_BYTES as usize || stderr.len() > 64 * 1024 {
        group.terminate().await;
        bail!("Nix closure validation output exceeded its bound");
    }
    group.disarm();
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/// Applies a fenced remote failure report and optionally requeues the same row.
///
/// Failure is independent from build and cache state. Transient,
/// authorization, and cancelled failures requeue for another remote scanner or
/// the local fallback. Deterministic failures terminate the scan.
///
/// # Errors
///
/// Returns an error when ownership is stale or persistence fails.
pub async fn fail_remote_cve_scan(
    pool: &PgPool,
    lease: CveScanLease,
    class: CveScanFailureClass,
    message: &str,
    diagnostics: &[cf_protocol::builder::CveScanDiagnostic],
) -> Result<bool> {
    let requeue = !matches!(class, CveScanFailureClass::Deterministic);
    let sanitized = sanitize_failure(message);
    let mut diagnostics = crate::queries::cve_scan_diagnostics::prepare_diagnostics(diagnostics);
    let diagnostics_were_capped =
        diagnostics.len() >= crate::queries::cve_scan_diagnostics::MAX_DIAGNOSTIC_EVENTS;
    if diagnostics_were_capped {
        diagnostics.truncate(crate::queries::cve_scan_diagnostics::MAX_DIAGNOSTIC_EVENTS - 1);
    }
    diagnostics.push(
        crate::queries::cve_scan_diagnostics::PreparedScanDiagnostic {
            occurred_at: Utc::now(),
            level: "error".to_string(),
            source: "server".to_string(),
            event_type: if requeue {
                "attempt_requeued"
            } else {
                "attempt_failed"
            }
            .to_string(),
            message: sanitized.clone(),
            truncated: diagnostics_were_capped || message.chars().count() > MAX_FAILURE_CHARS,
        },
    );
    let class_name = match class {
        CveScanFailureClass::Transient => "transient",
        CveScanFailureClass::Deterministic => "deterministic",
        CveScanFailureClass::Authorization => "authorization",
        CveScanFailureClass::Cancelled => "cancelled",
    };
    let status = if requeue { "pending" } else { "failed" };
    let outcome = if requeue { "requeued" } else { "failed" };
    let completed = if requeue { None } else { Some(Utc::now()) };
    let derivation_id: Option<i32> = sqlx::query_scalar(
        "SELECT derivation_id FROM cve_scans WHERE id = $1 AND execution_id = $2",
    )
    .bind(lease.scan_id)
    .bind(lease.execution_id)
    .fetch_optional(pool)
    .await?;
    let Some(derivation_id) = derivation_id else {
        bail!("stale_cve_scan_execution");
    };
    let mut tx = pool.begin().await?;
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        &mut tx,
        derivation_id,
        &[],
    )
    .await?;
    crate::queries::cve_scan_diagnostics::append_remote_diagnostics_tx(
        &mut tx,
        lease,
        &diagnostics,
    )
    .await?;
    let result = sqlx::query(
        r#"
        UPDATE cve_scans scan
        SET status = $5, completed_at = $6,
            execution_id = CASE WHEN $10 THEN NULL ELSE scan.execution_id END,
            execution_outcome = CASE WHEN $10 THEN NULL ELSE $7 END,
            failure_class = CASE WHEN $10 THEN NULL ELSE $8 END,
            scan_metadata = (
                COALESCE(scan.scan_metadata, '{}'::jsonb)
                - CASE WHEN $10 THEN 'remote_execution' ELSE '__keep__' END
                - CASE WHEN $10 THEN 'execution_id' ELSE '__keep__' END
                - CASE WHEN $10 THEN 'execution_started_at' ELSE '__keep__' END
                - CASE WHEN $10 THEN 'execution_heartbeat_at' ELSE '__keep__' END
                - CASE WHEN $10 THEN 'execution_revoked_at' ELSE '__keep__' END
            ) || jsonb_build_object('error', $9::text)
              || CASE WHEN $10 THEN jsonb_build_object(
                    'last_remote_execution', jsonb_build_object(
                        'execution_id', scan.execution_id,
                        'builder_id', scan.lease_builder_id,
                        'builder_session_id', scan.lease_builder_session_id,
                        'scanner_name', scan.scanner_name,
                        'scanner_version', scan.scanner_version,
                        'requeued_at', NOW()
                    )
                 ) ELSE '{}'::jsonb END,
            lease_builder_id = NULL, lease_builder_session_id = NULL,
            lease_started_at = NULL, lease_heartbeat_at = NULL,
            lease_expires_at = NULL,
            scanner_policy = CASE WHEN $10 THEN NULL ELSE scan.scanner_policy END,
            target_drv_path = CASE WHEN $10 THEN NULL ELSE scan.target_drv_path END,
            target_outputs = CASE WHEN $10 THEN NULL ELSE scan.target_outputs END,
            scanner_version = CASE WHEN $10 THEN NULL ELSE scan.scanner_version END
        FROM builders builder
        WHERE scan.id = $1 AND scan.execution_id = $2
          AND scan.lease_builder_id = $3 AND scan.lease_builder_session_id = $4
          AND scan.status = 'in_progress' AND scan.lease_expires_at > NOW()
          AND builder.id = $3 AND builder.current_session_id = $4
          AND builder.enabled AND builder.registered AND builder.status = 'active'
          AND builder.cve_scanning_enabled AND builder.cve_scan_schema_version = 1
        "#,
    )
    .bind(lease.scan_id)
    .bind(lease.execution_id)
    .bind(lease.builder_id)
    .bind(lease.builder_session_id)
    .bind(status)
    .bind(completed)
    .bind(outcome)
    .bind(class_name)
    .bind(sanitized)
    .bind(requeue)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        tx.rollback().await?;
        bail!("stale_cve_scan_execution");
    }
    crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, lease.scan_id)
        .await?;
    tx.commit().await?;
    Ok(requeue)
}

async fn load_owned_claim(pool: &PgPool, lease: CveScanLease) -> Result<Option<CveScanClaim>> {
    let row = sqlx::query(
        r#"
        SELECT scan.derivation_id, derivation.derivation_name,
               scan.target_drv_path, scan.target_outputs, scan.scanner_policy,
               scan.scanner_name,
               COALESCE(scan.scanner_version, 'unknown') AS scanner_version,
               scan.lease_expires_at
        FROM cve_scans scan
        JOIN derivations derivation ON derivation.id = scan.derivation_id
        JOIN builders builder ON builder.id = scan.lease_builder_id
        WHERE scan.id = $1 AND scan.execution_id = $2
          AND scan.lease_builder_id = $3 AND scan.lease_builder_session_id = $4
          AND scan.status = 'in_progress' AND scan.lease_expires_at > NOW()
          AND builder.current_session_id = $4
          AND builder.enabled AND builder.registered AND builder.status = 'active'
          AND builder.cve_scanning_enabled AND builder.cve_scan_schema_version = 1
        "#,
    )
    .bind(lease.scan_id)
    .bind(lease.execution_id)
    .bind(lease.builder_id)
    .bind(lease.builder_session_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else { return Ok(None) };
    Ok(Some(CveScanClaim {
        lease,
        derivation: CveScanDerivation {
            derivation_id: row.get("derivation_id"),
            derivation_name: row.get("derivation_name"),
            drv_path: row.get("target_drv_path"),
            outputs: serde_json::from_value(row.get("target_outputs"))?,
        },
        schema_version: CveScanSchemaVersion::V1,
        scanner: CveScannerIdentity {
            name: row.get("scanner_name"),
            version: row.get("scanner_version"),
        },
        policy: serde_json::from_value(row.get("scanner_policy"))?,
        cache_source: None,
        lease_expires_at: row.get("lease_expires_at"),
    }))
}

fn canonicalize_result(mut result: CveScanResult, claim: &CveScanClaim) -> Result<CveScanResult> {
    if result.schema_version != claim.schema_version
        || result.scanner.name != claim.scanner.name
        || result.derivation != claim.derivation
    {
        bail!("result does not match the claimed schema, scanner, or derivation outputs");
    }
    if result.entries.len() > claim.policy.max_entries
        || result.observations.len() > claim.policy.max_observations
        || claim.policy.scanner_args.len() > MAX_SCANNER_ARGS
    {
        bail!("result exceeds claimed policy limits");
    }
    validate_text(&result.scanner.name, 50, false)?;
    validate_text(&result.scanner.version, 50, false)?;
    validate_store_path(&result.derivation.drv_path, true)?;
    if result.derivation.outputs.is_empty() {
        bail!("claimed derivation has no outputs");
    }
    canonicalize_outputs(&mut result.derivation.outputs)?;
    let mut ids = HashSet::with_capacity(result.entries.len());
    for entry in &mut result.entries {
        if !ids.insert(entry.entry_id) {
            bail!("duplicate entry_id {}", entry.entry_id);
        }
        validate_text(&entry.package_name, MAX_STRING_CHARS, false)?;
        if let Some(version) = &entry.package_version {
            validate_text(version, MAX_STRING_CHARS, true)?;
        }
        validate_store_path(&entry.drv_path, true)?;
        if entry.outputs.is_empty() {
            bail!("package entry has no outputs");
        }
        canonicalize_outputs(&mut entry.outputs)?;
    }
    for observation in &mut result.observations {
        if !ids.contains(&observation.entry_id) {
            bail!(
                "observation references unknown entry_id {}",
                observation.entry_id
            );
        }
        observation.cve_id = canonical_cve(&observation.cve_id)?;
        if let Some(score) = observation.cvss_score
            && (!score.is_finite() || !(0.0..=10.0).contains(&score))
        {
            bail!("CVSS score must be finite and between 0 and 10");
        }
        if let Some(severity) = &mut observation.severity {
            *severity = severity.trim().to_ascii_lowercase();
            if !matches!(
                severity.as_str(),
                "unknown" | "low" | "medium" | "high" | "critical"
            ) {
                bail!("invalid severity");
            }
        }
        if let Some(version) = &observation.fixed_version {
            validate_text(version, MAX_STRING_CHARS, true)?;
        }
    }
    result.entries.sort_by(|a, b| {
        a.drv_path
            .cmp(&b.drv_path)
            .then(a.entry_id.cmp(&b.entry_id))
    });
    result
        .observations
        .sort_by(|a, b| a.entry_id.cmp(&b.entry_id).then(a.cve_id.cmp(&b.cve_id)));
    let mut unique = HashSet::new();
    for observation in &result.observations {
        if !unique.insert((observation.entry_id, observation.cve_id.as_str())) {
            bail!("duplicate package CVE observation");
        }
    }
    Ok(result)
}

fn canonicalize_outputs(outputs: &mut Vec<CveDerivationOutput>) -> Result<()> {
    for output in outputs.iter_mut() {
        output.name = output.name.trim().to_string();
        validate_text(&output.name, 128, false)?;
        validate_store_path(&output.store_path, false)?;
    }
    outputs.sort_by(|a, b| a.name.cmp(&b.name).then(a.store_path.cmp(&b.store_path)));
    if outputs.windows(2).any(|pair| pair[0].name == pair[1].name) {
        bail!("duplicate output name");
    }
    Ok(())
}

fn result_to_vulnix(result: &CveScanResult) -> Result<(Vec<VulnixEntry>, HashMap<String, String>)> {
    let observations = result.observations.iter().fold(
        HashMap::<u32, Vec<&CveObservation>>::new(),
        |mut grouped, observation| {
            grouped
                .entry(observation.entry_id)
                .or_default()
                .push(observation);
            grouped
        },
    );
    let mut paths = HashMap::new();
    let entries = result
        .entries
        .iter()
        .map(|entry| {
            let output = entry
                .outputs
                .iter()
                .find(|output| output.name == "out")
                .or_else(|| entry.outputs.first())
                .context("package entry has no output")?;
            paths.insert(entry.drv_path.clone(), output.store_path.clone());
            let related = observations
                .get(&entry.entry_id)
                .cloned()
                .unwrap_or_default();
            Ok(VulnixEntry {
                name: match &entry.package_version {
                    Some(version) if !version.is_empty() => {
                        format!("{}-{version}", entry.package_name)
                    }
                    _ => entry.package_name.clone(),
                },
                pname: entry.package_name.clone(),
                version: entry.package_version.clone().unwrap_or_default(),
                affected_by: related
                    .iter()
                    .filter(|item| item.affected)
                    .map(|item| item.cve_id.clone())
                    .collect(),
                whitelisted: related
                    .iter()
                    .filter(|item| item.whitelisted)
                    .map(|item| item.cve_id.clone())
                    .collect(),
                derivation: entry.drv_path.clone(),
                cvssv3_basescore: related
                    .iter()
                    .filter_map(|item| item.cvss_score.map(|score| (item.cve_id.clone(), score)))
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((entries, paths))
}

fn canonical_cve(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_uppercase();
    let mut parts = value.split('-');
    let valid = value.len() <= 20
        && parts.next() == Some("CVE")
        && parts
            .next()
            .is_some_and(|part| part.len() == 4 && part.bytes().all(|b| b.is_ascii_digit()))
        && parts
            .next()
            .is_some_and(|part| part.len() >= 4 && part.bytes().all(|b| b.is_ascii_digit()))
        && parts.next().is_none();
    if !valid {
        bail!("invalid CVE identifier")
    }
    Ok(value)
}

fn validate_store_path(value: &str, drv: bool) -> Result<()> {
    validate_text(value, MAX_PATH_CHARS, false)?;
    if !is_canonical_nix_store_path(value, drv) {
        bail!("invalid Nix store path");
    }
    Ok(())
}

fn validate_text(value: &str, max: usize, allow_empty: bool) -> Result<()> {
    if value.chars().count() > max
        || value.contains('\0')
        || (!allow_empty && value.trim().is_empty())
    {
        bail!("invalid or overlong text field");
    }
    Ok(())
}

fn sanitize_failure(value: &str) -> String {
    crate::security::snapshot_redaction::redact_text(value)
        .chars()
        .filter(|character| !character.is_control() || *character == '\n')
        .take(MAX_FAILURE_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::builders::CreateBuilderRequest;
    use crate::queries::builders::{
        complete_job_atomic, create_builder, establish_builder_session, update_builder_heartbeat,
    };
    use crate::queries::derivations::insert_derivation;
    use cf_protocol::builder::CvePackageEvidence;

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn pending_intent_rebinds_to_replacement_before_old_completion(pool: PgPool) {
        let derivation = insert_derivation(&pool, None, "replacement-intent", "nixos")
            .await
            .unwrap();
        let old_job: Uuid = sqlx::query_scalar(
            "INSERT INTO build_jobs (derivation_id, status, completed_at) VALUES ($1, 'success', NOW()) RETURNING id",
        )
        .bind(derivation.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let scan: Uuid = sqlx::query_scalar(
            "INSERT INTO cve_scans (derivation_id, scanner_name, status, attempts, source_trigger, completed_build_job_id) VALUES ($1, 'vulnix', 'pending', 0, 'post_build', $2) RETURNING id",
        )
        .bind(derivation.id)
        .bind(old_job)
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        crate::queries::build_jobs::lock_build_derivation(&mut tx, derivation.id)
            .await
            .unwrap();
        let replacement: Uuid = sqlx::query_scalar(
            "INSERT INTO build_jobs (derivation_id, status) VALUES ($1, 'queued') RETURNING id",
        )
        .bind(derivation.id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            create_post_build_scan_intents_tx(&mut tx, &[derivation.id])
                .await
                .unwrap(),
            1
        );
        tx.commit().await.unwrap();
        let (status, prerequisite, attempts): (String, Option<Uuid>, i32) = sqlx::query_as(
            "SELECT status, completed_build_job_id, attempts FROM cve_scans WHERE id = $1",
        )
        .bind(scan)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (status.as_str(), prerequisite, attempts),
            ("awaiting_build", Some(replacement), 0)
        );
        assert!(
            !attach_completed_build_to_post_build_scan_tx(
                &mut pool.begin().await.unwrap(),
                old_job
            )
            .await
            .unwrap()
        );
        assert_eq!(
            crate::queries::cve_scans::promote_waiting_cve_scans(&pool, 1)
                .await
                .unwrap(),
            0
        );
        sqlx::query("UPDATE build_jobs SET status = 'success', completed_at = NOW() WHERE id = $1")
            .bind(replacement)
            .execute(&pool)
            .await
            .unwrap();
        let (status, prerequisite): (String, Option<Uuid>) =
            sqlx::query_as("SELECT status, completed_build_job_id FROM cve_scans WHERE id = $1")
                .bind(scan)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            (status.as_str(), prerequisite),
            ("awaiting_build", Some(replacement))
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn awaiting_closure_intent_rebinds_to_replacement_build(pool: PgPool) {
        let derivation = insert_derivation(&pool, None, "closure-replacement", "nixos")
            .await
            .unwrap();
        let old_job: Uuid = sqlx::query_scalar(
            "INSERT INTO build_jobs (derivation_id, status, completed_at) VALUES ($1, 'success', NOW()) RETURNING id",
        )
        .bind(derivation.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let scan: Uuid = sqlx::query_scalar(
            "INSERT INTO cve_scans (derivation_id, scanner_name, status, attempts, source_trigger, completed_build_job_id) VALUES ($1, 'vulnix', 'awaiting_closure', 0, 'post_build', $2) RETURNING id",
        )
        .bind(derivation.id)
        .bind(old_job)
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        crate::queries::build_jobs::lock_build_derivation(&mut tx, derivation.id)
            .await
            .unwrap();
        let replacement: Uuid = sqlx::query_scalar(
            "INSERT INTO build_jobs (derivation_id, status) VALUES ($1, 'queued') RETURNING id",
        )
        .bind(derivation.id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            create_post_build_scan_intents_tx(&mut tx, &[derivation.id])
                .await
                .unwrap(),
            1
        );
        tx.commit().await.unwrap();
        let actual: (String, Option<Uuid>, i32) = sqlx::query_as(
            "SELECT status, completed_build_job_id, attempts FROM cve_scans WHERE id = $1",
        )
        .bind(scan)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(actual, ("awaiting_build".into(), Some(replacement), 0));
        assert_eq!(
            crate::queries::cve_scans::promote_waiting_cve_scans(&pool, 1)
                .await
                .unwrap(),
            0,
            "old closure cannot promote while the replacement build is queued"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn remote_claim_rejects_superseded_pending_build(pool: PgPool) {
        let request = CreateBuilderRequest {
            name: "superseded-scan-builder".into(),
            host: None,
            arch: "x86_64-linux".into(),
            public_key: None,
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: Some(1),
            enabled: Some(true),
            environment_ids: vec![],
        };
        let (builder, _) = create_builder(&pool, &request).await.unwrap();
        let session = Uuid::new_v4();
        establish_builder_session(&pool, &builder.id, &session, 60, "scan test")
            .await
            .unwrap();
        record_session_cve_capabilities(
            &pool,
            builder.id,
            session,
            cf_protocol::builder::BuilderCapabilities::current_cve_scanner("vulnix test".into()),
        )
        .await
        .unwrap();
        let derivation = insert_derivation(&pool, None, "superseded-scan", "nixos")
            .await
            .unwrap();
        sqlx::query("UPDATE derivations SET derivation_path = '/nix/store/superseded.drv', store_path = '/nix/store/superseded' WHERE id = $1")
            .bind(derivation.id).execute(&pool).await.unwrap();
        let old_job: Uuid = sqlx::query_scalar(
            "INSERT INTO build_jobs (derivation_id, builder_id, builder_session_id, status, completed_at) VALUES ($1, $2, $3, 'success', NOW()) RETURNING id",
        ).bind(derivation.id).bind(builder.id).bind(session).fetch_one(&pool).await.unwrap();
        let scan: Uuid = sqlx::query_scalar(
            "INSERT INTO cve_scans (derivation_id, scanner_name, status, attempts, source_trigger, completed_build_job_id) VALUES ($1, 'vulnix', 'pending', 0, 'post_build', $2) RETURNING id",
        ).bind(derivation.id).bind(old_job).fetch_one(&pool).await.unwrap();
        let mut admission = pool.begin().await.unwrap();
        crate::queries::build_jobs::lock_build_derivation(&mut admission, derivation.id)
            .await
            .unwrap();
        assert!(
            claim_remote_cve_scan(&pool, builder.id, session, Some(old_job))
                .await
                .unwrap()
                .is_none(),
            "a remote claimant must release its builder row instead of waiting for admission"
        );
        admission.rollback().await.unwrap();
        sqlx::query("INSERT INTO build_jobs (derivation_id, status) VALUES ($1, 'queued')")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            claim_remote_cve_scan(&pool, builder.id, session, Some(old_job))
                .await
                .unwrap()
                .is_none()
        );
        let (status, attempts): (String, i32) =
            sqlx::query_as("SELECT status, attempts FROM cve_scans WHERE id = $1")
                .bind(scan)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((status.as_str(), attempts), ("pending", 0));
    }

    #[test]
    fn semantic_validation_rejects_unknown_references_and_non_finite_cvss() {
        assert!(canonical_cve(" cve-2026-1234 ").is_ok());
        assert!(canonical_cve("GHSA-1234").is_err());
        assert!(!f32::NAN.is_finite());
    }

    #[test]
    fn failure_sanitization_is_bounded_and_removes_controls() {
        let sanitized = sanitize_failure(&("x".repeat(MAX_FAILURE_CHARS + 10) + "\0secret"));
        assert_eq!(sanitized.chars().count(), MAX_FAILURE_CHARS);
        assert!(!sanitized.contains('\0'));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn local_closure_verification_binds_package_output_to_exact_deriver() {
        use std::os::unix::fs::PermissionsExt;

        let target = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-system";
        let package_output = "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package";
        let package_drv = "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package.drv";
        let result = CveScanResult {
            schema_version: CveScanSchemaVersion::V1,
            scanner: CveScannerIdentity {
                name: "vulnix".to_string(),
                version: "vulnix test".to_string(),
            },
            derivation: CveScanDerivation {
                derivation_id: 1,
                derivation_name: "system".to_string(),
                drv_path: "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-system.drv".to_string(),
                outputs: vec![CveDerivationOutput {
                    name: "out".to_string(),
                    store_path: target.to_string(),
                }],
            },
            entries: vec![CvePackageEvidence {
                entry_id: 0,
                package_name: "package".to_string(),
                package_version: Some("1".to_string()),
                drv_path: package_drv.to_string(),
                outputs: vec![CveDerivationOutput {
                    name: "out".to_string(),
                    store_path: package_output.to_string(),
                }],
            }],
            observations: vec![],
        };
        assert_eq!(
            validate_target_closure_with_program(
                &result,
                std::ffi::OsStr::new("/does/not/run"),
                false,
                &HashSet::new(),
            )
            .await
            .expect("unavailable targets should retain explicit provenance"),
            ClosureProvenance::UnverifiedRemote
        );

        let directory = tempfile::tempdir().expect("Nix query fixture directory");
        let good = directory.path().join("good-nix-store");
        let mismatch = directory.path().join("mismatch-nix-store");
        for (path, reported_drv) in [
            (&good, package_drv),
            (
                &mismatch,
                "/nix/store/cccccccccccccccccccccccccccccccc-other.drv",
            ),
        ] {
            std::fs::write(
                path,
                format!(
                    "#!/bin/sh\nif [ \"$2\" = \"--requisites\" ]; then printf '%s\\n' '{package_output}'; else printf '%s\\n' '{reported_drv}'; fi\n"
                ),
            )
            .expect("Nix query fixture");
            let mut permissions = std::fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o700);
            std::fs::set_permissions(path, permissions).unwrap();
        }

        let local_package_outputs = HashSet::from([package_output.to_string()]);

        assert_eq!(
            validate_target_closure_with_program(
                &result,
                good.as_os_str(),
                false,
                &local_package_outputs,
            )
            .await
            .expect("an unavailable target should retain verified local mappings"),
            ClosureProvenance::UnverifiedRemote
        );
        assert!(
            validate_target_closure_with_program(
                &result,
                mismatch.as_os_str(),
                false,
                &local_package_outputs,
            )
            .await
            .expect_err("a locally disproved mapping must be rejected without target membership")
            .to_string()
            .contains("does not match")
        );

        assert_eq!(
            validate_target_closure_with_program(
                &result,
                good.as_os_str(),
                true,
                &local_package_outputs,
            )
            .await
            .expect("matching local closure should verify"),
            ClosureProvenance::ServerLocalVerified
        );
        assert!(
            validate_target_closure_with_program(
                &result,
                mismatch.as_os_str(),
                true,
                &local_package_outputs,
            )
            .await
            .expect_err("mismatched package deriver must be rejected")
            .to_string()
            .contains("does not match")
        );
    }

    #[tokio::test]
    async fn remote_lease_claim_heartbeat_validation_and_clean_completion() {
        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should connect");
        let suffix = Uuid::new_v4().simple().to_string();
        let request = CreateBuilderRequest {
            name: format!("remote-cve-{suffix}"),
            host: None,
            arch: "x86_64-linux".to_string(),
            public_key: None,
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: Some(1),
            enabled: Some(true),
            environment_ids: vec![],
        };
        let (builder, _) = create_builder(&pool, &request)
            .await
            .expect("builder should be created");
        let session_id = Uuid::new_v4();
        establish_builder_session(&pool, &builder.id, &session_id, 60, "remote CVE test")
            .await
            .expect("session should be established");

        let derivation =
            insert_derivation(&pool, None, &format!("remote-cve-target-{suffix}"), "nixos")
                .await
                .expect("derivation should be created");
        let drv_path = format!("/nix/store/{suffix}-system.drv");
        let output_path = format!("/nix/store/{suffix}-system");
        sqlx::query("UPDATE derivations SET derivation_path = $2, store_path = $3 WHERE id = $1")
            .bind(derivation.id)
            .bind(&drv_path)
            .bind(&output_path)
            .execute(&pool)
            .await
            .expect("scan identity should be populated");
        let scan_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO cve_scans (
                derivation_id, scanner_name, scanner_version, status,
                attempts, source_trigger
            ) VALUES ($1, 'vulnix', 'test', 'pending', 0, 'manual')
            RETURNING id
            "#,
        )
        .bind(derivation.id)
        .fetch_one(&pool)
        .await
        .expect("scan should be queued");

        let completed_build_job_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO build_jobs (
                builder_id, builder_session_id, derivation_id, status,
                started_at
            ) VALUES ($1, $2, $3, 'building', NOW())
            RETURNING id
            "#,
        )
        .bind(builder.id)
        .bind(session_id)
        .bind(derivation.id)
        .fetch_one(&pool)
        .await
        .expect("completed build job should be created");
        sqlx::query("UPDATE scan_schedule_policy SET on_build = true WHERE id = 1")
            .execute(&pool)
            .await
            .expect("post-build policy should be enabled");
        let (_, is_new) = complete_job_atomic(
            &pool,
            &completed_build_job_id,
            &builder.id,
            Some(&session_id),
            Some(&output_path),
        )
        .await
        .expect("build completion and post-build enqueue should commit together");
        assert!(is_new);
        let (trigger, provenance): (String, Option<Uuid>) = sqlx::query_as(
            "SELECT source_trigger, completed_build_job_id FROM cve_scans WHERE id = $1",
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("reused scan provenance should be queryable");
        assert_eq!(trigger, "manual");
        assert_eq!(provenance, None);
        let (_, retry_is_new) = complete_job_atomic(
            &pool,
            &completed_build_job_id,
            &builder.id,
            Some(&session_id),
            Some("/nix/store/ignored-idempotent-output"),
        )
        .await
        .expect("completion retry should recover enqueue idempotently");
        assert!(!retry_is_new);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM cve_scans WHERE derivation_id = $1 AND status IN ('pending', 'in_progress')",
            )
            .bind(derivation.id)
            .fetch_one(&pool)
            .await
            .expect("active scan count should be queryable"),
            1
        );
        sqlx::query(
            "UPDATE cve_scans SET source_trigger = 'post_build', completed_build_job_id = $2 WHERE id = $1",
        )
            .bind(scan_id)
            .bind(completed_build_job_id)
            .execute(&pool)
            .await
            .expect("remote lifecycle fixture should become post-build work");

        let queued_derivation = insert_derivation(
            &pool,
            None,
            &format!("remote-cve-queued-build-{suffix}"),
            "nixos",
        )
        .await
        .expect("queued build derivation should be created");
        sqlx::query("UPDATE derivations SET derivation_path = $2, store_path = $3 WHERE id = $1")
            .bind(queued_derivation.id)
            .bind(format!("/nix/store/{suffix}-queued-system.drv"))
            .bind(format!("/nix/store/{suffix}-queued-system"))
            .execute(&pool)
            .await
            .expect("queued derivation scan identity should be populated");
        let queued_build_job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO build_jobs (derivation_id, status) VALUES ($1, 'queued') RETURNING id",
        )
        .bind(queued_derivation.id)
        .fetch_one(&pool)
        .await
        .expect("queued build job should be created");

        assert!(
            claim_remote_cve_scan(&pool, builder.id, session_id, None)
                .await
                .expect("incapable claim should succeed")
                .is_none(),
            "an old incapable builder must not receive scan work"
        );
        record_session_cve_capabilities(
            &pool,
            builder.id,
            session_id,
            cf_protocol::builder::BuilderCapabilities::current_cve_scanner(
                "vulnix test".to_string(),
            ),
        )
        .await
        .expect("capabilities should persist");
        sqlx::query("UPDATE builders SET status = 'offline' WHERE id = $1")
            .bind(builder.id)
            .execute(&pool)
            .await
            .expect("builder should be marked offline after a stale heartbeat");
        assert!(
            record_session_cve_capabilities(
                &pool,
                builder.id,
                session_id,
                cf_protocol::builder::BuilderCapabilities::current_cve_scanner(
                    "vulnix test".to_string(),
                ),
            )
            .await
            .expect("offline current-session capabilities should persist")
        );
        update_builder_heartbeat(&pool, &builder.id, Some(&session_id))
            .await
            .expect("authenticated current-session heartbeat should reactivate builder");
        let status: String = sqlx::query_scalar("SELECT status FROM builders WHERE id = $1")
            .bind(builder.id)
            .fetch_one(&pool)
            .await
            .expect("reactivated builder status should be queryable");
        assert_eq!(status, "active");
        assert!(
            !record_session_cve_capabilities(
                &pool,
                builder.id,
                Uuid::new_v4(),
                cf_protocol::builder::BuilderCapabilities::current_cve_scanner(
                    "vulnix test".to_string(),
                ),
            )
            .await
            .expect("stale-session capability write should execute"),
            "a stale session must not persist capabilities"
        );
        sqlx::query("UPDATE builders SET registered = false WHERE id = $1")
            .bind(builder.id)
            .execute(&pool)
            .await
            .expect("builder should be unregistered for capability fencing");
        assert!(
            !record_session_cve_capabilities(
                &pool,
                builder.id,
                session_id,
                cf_protocol::builder::BuilderCapabilities::current_cve_scanner(
                    "vulnix test".to_string(),
                ),
            )
            .await
            .expect("unregistered capability write should execute"),
            "an unregistered builder must not persist capabilities"
        );
        sqlx::query("UPDATE builders SET registered = true WHERE id = $1")
            .bind(builder.id)
            .execute(&pool)
            .await
            .expect("builder registration should be restored");

        let future_schema = cf_protocol::builder::BuilderCapabilities {
            cve_scanning: true,
            cve_scan_schema_version: 2,
            cve_scanner: Some(CveScannerIdentity {
                name: "vulnix".to_string(),
                version: "vulnix test".to_string(),
            }),
        };
        assert!(
            record_session_cve_capabilities(&pool, builder.id, session_id, future_schema)
                .await
                .expect("unknown schema should preserve the builder session")
        );
        assert!(
            claim_remote_cve_scan(&pool, builder.id, session_id, None)
                .await
                .expect("unknown-schema claim should succeed")
                .is_none(),
            "an unknown schema must not authorize scan work"
        );
        record_session_cve_capabilities(
            &pool,
            builder.id,
            session_id,
            cf_protocol::builder::BuilderCapabilities::current_cve_scanner(
                "vulnix test".to_string(),
            ),
        )
        .await
        .expect("current capabilities should be restored");

        let allowed_environment_id: Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                .bind(format!("rcve-a-{suffix}"))
                .fetch_one(&pool)
                .await
                .expect("allowed environment should be created");
        let hidden_environment_id: Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                .bind(format!("rcve-h-{suffix}"))
                .fetch_one(&pool)
                .await
                .expect("hidden environment should be created");
        sqlx::query(
            "INSERT INTO builder_environment_assignments (builder_id, environment_id) VALUES ($1, $2)",
        )
        .bind(builder.id)
        .bind(allowed_environment_id)
        .execute(&pool)
        .await
        .expect("builder environment assignment should persist");
        sqlx::query("UPDATE build_jobs SET environment_id = $2 WHERE id = $1")
            .bind(completed_build_job_id)
            .bind(hidden_environment_id)
            .execute(&pool)
            .await
            .expect("completed build environment should persist");
        assert!(
            claim_remote_cve_scan(&pool, builder.id, session_id, Some(completed_build_job_id))
                .await
                .expect("hidden environment claim should not fail")
                .is_none(),
            "candidate selection must not disclose unauthorized environment work"
        );
        sqlx::query(
            "INSERT INTO builder_environment_assignments (builder_id, environment_id) VALUES ($1, $2)",
        )
        .bind(builder.id)
        .bind(hidden_environment_id)
        .execute(&pool)
        .await
        .expect("hidden environment authorization should persist");
        assert!(
            claim_remote_cve_scan(&pool, builder.id, session_id, None)
                .await
                .expect("background claim should succeed")
                .is_none(),
            "queued build work must block background scans"
        );
        let mut claim =
            claim_remote_cve_scan(&pool, builder.id, session_id, Some(completed_build_job_id))
                .await
                .expect("capable claim should succeed")
                .expect("direct post-build scan should bypass unrelated queued build work");
        assert_eq!(claim.lease.scan_id, scan_id);
        assert_eq!(claim.scanner.version, "vulnix test");
        assert!(
            claim_remote_cve_scan(&pool, builder.id, session_id, None)
                .await
                .expect("second claim should succeed")
                .is_none(),
            "one builder must own at most one active scan"
        );
        let live_diagnostic = cf_protocol::builder::CveScanDiagnostic {
            occurred_at: Utc::now(),
            level: "info".to_string(),
            source: "vulnix".to_string(),
            event_type: "scanner_started".to_string(),
            message: "Vulnix scan started for the authorized outputs.".to_string(),
            truncated: false,
        };
        let renewed = heartbeat_remote_cve_scan(
            &pool,
            claim.lease,
            0,
            0,
            std::slice::from_ref(&live_diagnostic),
        )
        .await
        .expect("heartbeat should execute")
        .expect("heartbeat should retain ownership");
        assert!(renewed > claim.lease_expires_at);
        let live_events =
            crate::queries::cve_scan_diagnostics::get_scan_diagnostics(&pool, claim.lease.scan_id)
                .await
                .expect("live diagnostics should load")
                .expect("claimed scan should exist");
        assert_eq!(live_events.status, "in_progress");
        assert_eq!(live_events.events.len(), 1);
        assert_eq!(live_events.events[0].event_type, "scanner_started");
        assert!(live_events.completed_at.is_none());
        assert_eq!(
            crate::queries::cve_scans::recover_stale_scans(&pool, std::time::Duration::ZERO,)
                .await
                .expect("legacy recovery should execute"),
            0,
            "legacy stale recovery must not revoke a typed remote lease"
        );
        assert!(
            heartbeat_remote_cve_scan(
                &pool,
                claim.lease,
                0,
                0,
                std::slice::from_ref(&live_diagnostic),
            )
            .await
            .expect("typed heartbeat after legacy recovery should execute")
            .is_some()
        );
        let retried_events =
            crate::queries::cve_scan_diagnostics::get_scan_diagnostics(&pool, claim.lease.scan_id)
                .await
                .expect("retried diagnostics should load")
                .expect("claimed scan should exist");
        assert_eq!(
            retried_events
                .events
                .iter()
                .filter(|event| event.event_type == "scanner_started")
                .count(),
            1,
            "a retried heartbeat must not duplicate an acknowledged phase event",
        );

        let mut result = CveScanResult {
            schema_version: CveScanSchemaVersion::V1,
            scanner: claim.scanner.clone(),
            derivation: claim.derivation.clone(),
            entries: vec![CvePackageEvidence {
                entry_id: 0,
                package_name: "remote-package".to_string(),
                package_version: Some("1.0".to_string()),
                drv_path: format!("/nix/store/{suffix}-remote-package.drv"),
                outputs: vec![CveDerivationOutput {
                    name: "out".to_string(),
                    store_path: format!("/nix/store/{suffix}-remote-package"),
                }],
            }],
            observations: vec![
                CveObservation {
                    entry_id: 0,
                    cve_id: "CVE-2026-1001".to_string(),
                    cvss_score: Some(8.0),
                    severity: Some("high".to_string()),
                    fixed_version: None,
                    affected: true,
                    whitelisted: false,
                },
                CveObservation {
                    entry_id: 0,
                    cve_id: "CVE-2026-1002".to_string(),
                    cvss_score: Some(5.0),
                    severity: Some("medium".to_string()),
                    fixed_version: None,
                    affected: false,
                    whitelisted: true,
                },
            ],
        };
        sqlx::query("UPDATE builders SET enabled = false WHERE id = $1")
            .bind(builder.id)
            .execute(&pool)
            .await
            .expect("builder should be disabled");
        assert!(
            !record_session_cve_capabilities(
                &pool,
                builder.id,
                session_id,
                cf_protocol::builder::BuilderCapabilities::current_cve_scanner(
                    "vulnix test".to_string(),
                ),
            )
            .await
            .expect("disabled capability write should execute"),
            "a disabled builder must not persist capabilities"
        );
        assert!(
            claim_remote_cve_scan(&pool, builder.id, session_id, None)
                .await
                .expect_err("disabled claim should be rejected")
                .to_string()
                .contains("builder_inactive"),
            "a disabled builder must not claim scan work"
        );
        assert!(
            heartbeat_remote_cve_scan(&pool, claim.lease, 0, 0, &[])
                .await
                .expect("disabled heartbeat should execute")
                .is_none(),
            "a disabled builder must lose lease write authority"
        );
        assert!(
            fail_remote_cve_scan(
                &pool,
                claim.lease,
                CveScanFailureClass::Transient,
                "disabled builder",
                &[],
            )
            .await
            .is_err(),
            "a disabled builder must not fail or requeue its lease"
        );
        assert_eq!(
            requeue_expired_remote_cve_scans(&pool, 10)
                .await
                .expect("disabled builder lease should recover"),
            1
        );
        sqlx::query("UPDATE builders SET enabled = true, status = 'active' WHERE id = $1")
            .bind(builder.id)
            .execute(&pool)
            .await
            .expect("builder should be re-enabled");
        record_session_cve_capabilities(
            &pool,
            builder.id,
            session_id,
            cf_protocol::builder::BuilderCapabilities::current_cve_scanner(
                "vulnix test".to_string(),
            ),
        )
        .await
        .expect("re-enabled capability should persist");
        claim = claim_remote_cve_scan(&pool, builder.id, session_id, Some(completed_build_job_id))
            .await
            .expect("re-enabled claim should execute")
            .expect("re-enabled builder should reclaim work");
        result.scanner = claim.scanner.clone();
        result.derivation = claim.derivation.clone();

        let takeover_session_id = Uuid::new_v4();
        sqlx::query(
            r#"
            UPDATE builders
            SET current_session_id = $2,
                cve_scanning_enabled = false,
                cve_scan_schema_version = 0,
                cve_scanner_name = NULL,
                cve_scanner_version = NULL
            WHERE id = $1
            "#,
        )
        .bind(builder.id)
        .bind(takeover_session_id)
        .execute(&pool)
        .await
        .expect("replacement session should be installed");
        assert!(
            heartbeat_remote_cve_scan(&pool, claim.lease, 0, 0, &[])
                .await
                .expect("superseded heartbeat should execute")
                .is_none(),
            "session takeover must fence the prior process"
        );
        assert_eq!(
            requeue_expired_remote_cve_scans(&pool, 10)
                .await
                .expect("superseded lease should recover"),
            1
        );
        sqlx::query("UPDATE builders SET current_session_id = $2 WHERE id = $1")
            .bind(builder.id)
            .bind(session_id)
            .execute(&pool)
            .await
            .expect("test session should be restored");
        record_session_cve_capabilities(
            &pool,
            builder.id,
            session_id,
            cf_protocol::builder::BuilderCapabilities::current_cve_scanner(
                "vulnix test".to_string(),
            ),
        )
        .await
        .expect("restored session capability should persist");
        claim = claim_remote_cve_scan(&pool, builder.id, session_id, Some(completed_build_job_id))
            .await
            .expect("restored session claim should execute")
            .expect("restored session should reclaim work");
        result.scanner = claim.scanner.clone();
        result.derivation = claim.derivation.clone();
        let invalid = CveScanCompleteRequest {
            lease: claim.lease,
            result: CveScanResult {
                derivation: CveScanDerivation {
                    drv_path: "/nix/store/unauthorized.drv".to_string(),
                    ..result.derivation.clone()
                },
                ..result.clone()
            },
            result_digest_sha256: "0".repeat(64),
            scan_duration_ms: 1,
            diagnostics: Vec::new(),
        };
        assert!(matches!(
            complete_remote_cve_scan(&pool, invalid)
                .await
                .expect("invalid completion should be classified"),
            RemoteCompletion::Invalid(_)
        ));
        assert!(
            heartbeat_remote_cve_scan(&pool, claim.lease, 0, 0, &[])
                .await
                .expect("post-validation heartbeat should execute")
                .is_some(),
            "semantic rejection must retain the lease"
        );

        let canonical = canonicalize_result(result.clone(), &claim)
            .expect("clean stale result should canonicalize");
        let digest =
            canonical_cve_result_digest(&canonical).expect("canonical result digest should encode");
        let stale_completion = CveScanCompleteRequest {
            lease: claim.lease,
            result: result.clone(),
            result_digest_sha256: digest,
            scan_duration_ms: 1,
            diagnostics: vec![cf_protocol::builder::CveScanDiagnostic {
                occurred_at: Utc::now(),
                level: "error".to_string(),
                source: "builder".to_string(),
                event_type: "output".to_string(),
                message: "stale remote diagnostic".to_string(),
                truncated: false,
            }],
        };
        sqlx::query(
            "UPDATE cve_scans SET lease_expires_at = NOW() - INTERVAL '1 second' WHERE id = $1",
        )
        .bind(scan_id)
        .execute(&pool)
        .await
        .expect("lease should be expired");
        assert_eq!(
            complete_remote_cve_scan(&pool, stale_completion)
                .await
                .expect("expired completion should be classified"),
            RemoteCompletion::Stale
        );
        let diagnostic_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cve_scan_diagnostic_events WHERE scan_id = $1",
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("stale remote diagnostics should be countable");
        assert_eq!(
            diagnostic_count, 0,
            "an expired remote lease must not append diagnostics"
        );
        assert_eq!(
            requeue_expired_remote_cve_scans(&pool, 10)
                .await
                .expect("expired lease should be recovered"),
            1
        );
        let prior_execution_id = claim.lease.execution_id;
        let cleared: (bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
            r#"
            SELECT execution_id IS NULL,
                   lease_builder_id IS NULL,
                   lease_builder_session_id IS NULL,
                   scanner_policy IS NULL,
                   target_drv_path IS NULL,
                   target_outputs IS NULL,
                   scan_metadata ? 'last_remote_execution'
            FROM cve_scans WHERE id = $1
            "#,
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("requeued remote ownership should be queryable");
        assert_eq!(cleared, (true, true, true, true, true, true, true));
        sqlx::query("UPDATE cve_scans SET created_at = NOW() - INTERVAL '2 minutes' WHERE id = $1")
            .bind(scan_id)
            .execute(&pool)
            .await
            .expect("local fallback delay should elapse");
        let local_claim = crate::queries::cve_scans::claim_queued_cve_scans(&pool, 1)
            .await
            .expect("server-local fallback should claim recovered remote work")
            .into_iter()
            .find(|local_claim| local_claim.scan_id == scan_id)
            .expect("recovered scan should be locally claimable");
        assert_eq!(
            requeue_expired_remote_cve_scans(&pool, 10)
                .await
                .expect("typed recovery should ignore a server-local successor"),
            0,
            "remote recovery must not cancel the server-local fallback"
        );
        assert!(
            crate::queries::cve_scans::heartbeat_cve_scan_execution(
                &pool,
                scan_id,
                local_claim.execution_id,
            )
            .await
            .expect("server-local successor heartbeat should execute")
        );
        assert!(
            crate::queries::cve_scans::requeue_cve_scan_execution(
                &pool,
                scan_id,
                local_claim.execution_id,
                "remote-to-local fallback race regression",
            )
            .await
            .expect("server-local successor should return test work to the queue")
        );
        claim = claim_remote_cve_scan(&pool, builder.id, session_id, Some(completed_build_job_id))
            .await
            .expect("recovered claim should succeed")
            .expect("producing builder should reclaim the recovered scan");
        assert_ne!(claim.lease.execution_id, prior_execution_id);

        let canonical =
            canonicalize_result(result.clone(), &claim).expect("clean result should canonicalize");
        let digest =
            canonical_cve_result_digest(&canonical).expect("canonical result digest should encode");
        let completion = CveScanCompleteRequest {
            lease: claim.lease,
            result,
            result_digest_sha256: digest.clone(),
            scan_duration_ms: 1,
            diagnostics: vec![cf_protocol::builder::CveScanDiagnostic {
                occurred_at: Utc::now(),
                level: "warning".to_string(),
                source: "vulnix".to_string(),
                event_type: "output".to_string(),
                message: "immutable completion diagnostic".to_string(),
                truncated: false,
            }],
        };
        assert_eq!(
            complete_remote_cve_scan(&pool, completion.clone())
                .await
                .expect("clean completion should persist"),
            RemoteCompletion::Completed(digest.clone())
        );
        assert_eq!(
            complete_remote_cve_scan(&pool, completion.clone())
                .await
                .expect("completion retry should resolve"),
            RemoteCompletion::AlreadyCompleted(digest)
        );
        let mut conflicting_completion = completion;
        conflicting_completion.result_digest_sha256 = "f".repeat(64);
        assert_eq!(
            complete_remote_cve_scan(&pool, conflicting_completion)
                .await
                .expect("conflicting completion should be classified"),
            RemoteCompletion::DigestConflict
        );
        let (status, schema, total, closure_provenance): (String, i32, i32, String) = sqlx::query_as(
            "SELECT status, evidence_schema_version, total_vulnerabilities, closure_provenance FROM cve_scans WHERE id = $1",
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("completed scan should be queryable");
        assert_eq!((status.as_str(), schema, total), ("completed", 1, 1));
        assert_eq!(closure_provenance, "unverified_remote");
        let diagnostic_id: i64 = sqlx::query_scalar(
            "SELECT id FROM cve_scan_diagnostic_events WHERE scan_id = $1 LIMIT 1",
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("completion diagnostic should persist");
        assert!(
            sqlx::query("UPDATE cve_scan_diagnostic_events SET message = 'mutated' WHERE id = $1")
                .bind(diagnostic_id)
                .execute(&pool)
                .await
                .is_err(),
            "persisted diagnostics must reject direct updates"
        );
        assert!(
            sqlx::query("DELETE FROM cve_scan_diagnostic_events WHERE id = $1")
                .bind(diagnostic_id)
                .execute(&pool)
                .await
                .is_err(),
            "persisted diagnostics must reject direct deletes"
        );
        let dispositions: Vec<(String, bool, bool)> = sqlx::query_as(
            r#"
            SELECT canonical_cve_id, is_affected, is_whitelisted
            FROM cve_scan_vulnerability_observations
            WHERE scan_id = $1
            ORDER BY canonical_cve_id
            "#,
        )
        .bind(scan_id)
        .fetch_all(&pool)
        .await
        .expect("exact remote dispositions should be queryable");
        assert_eq!(
            dispositions,
            vec![
                ("CVE-2026-1001".to_string(), true, false),
                ("CVE-2026-1002".to_string(), false, true),
            ]
        );

        sqlx::query("DELETE FROM build_jobs WHERE id = $1")
            .bind(queued_build_job_id)
            .execute(&pool)
            .await
            .expect("queued build cleanup should succeed");

        let competing_request = CreateBuilderRequest {
            name: format!("remote-cve-competing-{suffix}"),
            host: None,
            arch: "x86_64-linux".to_string(),
            public_key: None,
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: Some(1),
            enabled: Some(true),
            environment_ids: vec![],
        };
        let (competing_builder, _) = create_builder(&pool, &competing_request)
            .await
            .expect("competing builder should be created");
        let competing_session_id = Uuid::new_v4();
        establish_builder_session(
            &pool,
            &competing_builder.id,
            &competing_session_id,
            60,
            "remote CVE race test",
        )
        .await
        .expect("competing session should be established");
        record_session_cve_capabilities(
            &pool,
            competing_builder.id,
            competing_session_id,
            cf_protocol::builder::BuilderCapabilities::current_cve_scanner(
                "vulnix test".to_string(),
            ),
        )
        .await
        .expect("competing capabilities should persist");
        let race_derivation =
            insert_derivation(&pool, None, &format!("remote-cve-race-{suffix}"), "nixos")
                .await
                .expect("race derivation should be created");
        sqlx::query("UPDATE derivations SET derivation_path = $2, store_path = $3 WHERE id = $1")
            .bind(race_derivation.id)
            .bind(format!("/nix/store/{suffix}-race-system.drv"))
            .bind(format!("/nix/store/{suffix}-race-system"))
            .execute(&pool)
            .await
            .expect("race derivation scan identity should be populated");
        let race_scan_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO cve_scans (
                derivation_id, scanner_name, scanner_version, status,
                attempts, source_trigger, created_at
            ) VALUES (
                $1, 'vulnix', 'test', 'pending', 0, 'manual',
                NOW() - INTERVAL '61 seconds'
            )
            RETURNING id
            "#,
        )
        .bind(race_derivation.id)
        .fetch_one(&pool)
        .await
        .expect("race scan should be queued");
        sqlx::query(
            r#"
            INSERT INTO cache_push_jobs (
                derivation_id, status, completed_at, cache_destination
            ) VALUES ($1, 'completed', NOW(), 'remote-cve-test')
            "#,
        )
        .bind(race_derivation.id)
        .execute(&pool)
        .await
        .expect("race output cache provenance should persist");
        let (first_race, second_race) = tokio::join!(
            claim_remote_cve_scan(&pool, builder.id, session_id, None),
            claim_remote_cve_scan(&pool, competing_builder.id, competing_session_id, None,)
        );
        let race_claims = [
            first_race.expect("first racing claim should execute"),
            second_race.expect("second racing claim should execute"),
        ];
        assert!(
            race_claims.iter().all(Option::is_none),
            "manual work must remain server-local even after a completed cache push"
        );
        sqlx::query("DELETE FROM cve_scans WHERE id = $1")
            .bind(race_scan_id)
            .execute(&pool)
            .await
            .expect("race scan cleanup should succeed");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(race_derivation.id)
            .execute(&pool)
            .await
            .expect("race derivation cleanup should succeed");
        sqlx::query("DELETE FROM builders WHERE id = $1")
            .bind(competing_builder.id)
            .execute(&pool)
            .await
            .expect("competing builder cleanup should succeed");

        let failed_scan_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO cve_scans (
                derivation_id, scanner_name, scanner_version, status,
                attempts, source_trigger, completed_build_job_id, created_at
            ) VALUES (
                $1, 'vulnix', 'test', 'pending', 0, 'post_build', $2,
                NOW() - INTERVAL '61 seconds'
            )
            RETURNING id
            "#,
        )
        .bind(derivation.id)
        .bind(completed_build_job_id)
        .fetch_one(&pool)
        .await
        .expect("failure scan should be queued");
        let failed_claim = claim_remote_cve_scan(&pool, builder.id, session_id, None)
            .await
            .expect("failure scan claim should succeed")
            .expect("failure scan should be claimed");
        assert!(
            fail_remote_cve_scan(
                &pool,
                failed_claim.lease,
                CveScanFailureClass::Transient,
                "temporary scanner failure\0credential",
                &[],
            )
            .await
            .expect("transient failure should be recorded")
        );
        let (failed_status, build_status, error, attempts): (String, String, String, i32) =
            sqlx::query_as(
                r#"
            SELECT scan.status, job.status, scan.scan_metadata ->> 'error', scan.attempts
            FROM cve_scans scan
            CROSS JOIN build_jobs job
            WHERE scan.id = $1 AND job.id = $2
            "#,
            )
            .bind(failed_scan_id)
            .bind(completed_build_job_id)
            .fetch_one(&pool)
            .await
            .expect("independent scan and build outcomes should be queryable");
        assert_eq!(
            (failed_status.as_str(), build_status.as_str()),
            ("pending", "success")
        );
        assert_eq!(error, "temporary scanner failurecredential");
        assert_eq!(
            attempts, 1,
            "remote terminal reporting must not add an attempt"
        );

        let orphaned_claim = claim_remote_cve_scan(&pool, builder.id, session_id, None)
            .await
            .expect("requeued failure claim should succeed")
            .expect("requeued failure should be claimable");
        sqlx::query("DELETE FROM builders WHERE id = $1")
            .bind(builder.id)
            .execute(&pool)
            .await
            .expect("builder deletion should succeed");
        assert_eq!(
            requeue_expired_remote_cve_scans(&pool, 10)
                .await
                .expect("orphaned lease should be recovered"),
            1
        );
        let recovered_status: String =
            sqlx::query_scalar("SELECT status FROM cve_scans WHERE id = $1")
                .bind(orphaned_claim.lease.scan_id)
                .fetch_one(&pool)
                .await
                .expect("recovered orphan should be queryable");
        assert_eq!(recovered_status, "pending");

        sqlx::query("DELETE FROM cve_scans WHERE id = ANY($1)")
            .bind(&[scan_id, failed_scan_id])
            .execute(&pool)
            .await
            .expect("scan cleanup should succeed");
        sqlx::query("DELETE FROM build_jobs WHERE id = $1")
            .bind(completed_build_job_id)
            .execute(&pool)
            .await
            .expect("completed build cleanup should succeed");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(queued_derivation.id)
            .execute(&pool)
            .await
            .expect("queued derivation cleanup should succeed");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("derivation cleanup should succeed");
        sqlx::query("DELETE FROM builders WHERE id = $1")
            .bind(builder.id)
            .execute(&pool)
            .await
            .expect("builder cleanup should succeed");
        sqlx::query("DELETE FROM environments WHERE id = ANY($1)")
            .bind(&[allowed_environment_id, hidden_environment_id])
            .execute(&pool)
            .await
            .expect("environment cleanup should succeed");
    }
}
