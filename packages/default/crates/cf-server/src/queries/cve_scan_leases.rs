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
use sqlx::{PgPool, Row};
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
const MAX_FAILURE_CHARS: usize = 2048;
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

/// Queues post-build CVE work for the successful build's exact output.
///
/// The partial active-scan index makes this idempotent. The producing builder
/// can claim the row immediately by supplying `completed_build_job_id`. The
/// same builder process can also recover its affinity work during background
/// polling. After the affinity interval, only the server-local worker can claim
/// the row as fallback. Cache publication does not prove another builder has
/// configured access to or materialized the output.
///
/// # Errors
///
/// Returns an error when policy lookup or enqueue persistence fails.
pub async fn enqueue_post_build_scan(pool: &PgPool, build_job_id: Uuid) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let queued = enqueue_post_build_scan_tx(&mut tx, build_job_id).await?;
    tx.commit().await?;
    Ok(queued)
}

/// Queues post-build CVE work in the caller's build-completion transaction.
///
/// Existing active manual or fleet work retains its trigger and original
/// provenance. A successful completion retry repairs a missing post-build row
/// before it returns success.
///
/// # Errors
///
/// Returns an error when policy lookup or enqueue persistence fails.
pub async fn enqueue_post_build_scan_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    build_job_id: Uuid,
) -> Result<bool> {
    let inserted = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO cve_scans AS scan (
            id, derivation_id, scanner_name, status, attempts, source_trigger,
            completed_build_job_id
        )
        SELECT gen_random_uuid(), job.derivation_id, 'vulnix', 'pending', 0,
               'post_build', job.id
        FROM build_jobs job
        JOIN scan_schedule_policy policy ON policy.id = 1 AND policy.on_build
        WHERE job.id = $1 AND job.status = 'success'
        ON CONFLICT (derivation_id) WHERE status IN ('pending', 'in_progress')
        DO UPDATE SET source_trigger = scan.source_trigger,
                      completed_build_job_id = COALESCE(
                          scan.completed_build_job_id,
                          EXCLUDED.completed_build_job_id
                      )
        WHERE scan.status = 'pending'
          AND scan.source_trigger = 'post_build'
        RETURNING id
        "#,
    )
    .bind(build_job_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(inserted.is_some())
}

/// Claims one queued scan for an authenticated scanner-capable builder.
///
/// CONCURRENCY: The transaction locks the builder row first, then acquires the
/// established POA&M derivation lock before mutating `cve_scans`. No path takes
/// those locks in reverse order. The guarded pending-to-in-progress update and
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
    let candidate: Option<(Uuid, i32)> = sqlx::query_as(
        r#"
        WITH builder_environments AS (
            SELECT environment_id
            FROM builder_environment_assignments
            WHERE builder_id = $1
        )
        SELECT scan.id, scan.derivation_id
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
    let Some((scan_id, derivation_id)) = candidate else {
        tx.rollback().await?;
        return Ok(None);
    };

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
) -> Result<Option<DateTime<Utc>>> {
    if entries > CVE_SCAN_MAX_ENTRIES || observations > CVE_SCAN_MAX_OBSERVATIONS {
        return Ok(None);
    }
    let expires = Utc::now() + chrono::Duration::seconds(LEASE_SECONDS);
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
    .fetch_optional(pool)
    .await?;
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
        let renewed = heartbeat_remote_cve_scan(&pool, claim.lease, 0, 0)
            .await
            .expect("heartbeat should execute")
            .expect("heartbeat should retain ownership");
        assert!(renewed > claim.lease_expires_at);
        assert_eq!(
            crate::queries::cve_scans::recover_stale_scans(&pool, std::time::Duration::ZERO,)
                .await
                .expect("legacy recovery should execute"),
            0,
            "legacy stale recovery must not revoke a typed remote lease"
        );
        assert!(
            heartbeat_remote_cve_scan(&pool, claim.lease, 0, 0)
                .await
                .expect("typed heartbeat after legacy recovery should execute")
                .is_some()
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
            heartbeat_remote_cve_scan(&pool, claim.lease, 0, 0)
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
            heartbeat_remote_cve_scan(&pool, claim.lease, 0, 0)
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
            heartbeat_remote_cve_scan(&pool, claim.lease, 0, 0)
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
