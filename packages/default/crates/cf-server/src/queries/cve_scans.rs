use crate::derivations::utils::get_store_path_from_drv;
use crate::derivations::{Derivation, DerivationType};
use crate::models::cve_scans::{CveScan, ScanStatus};
use crate::queries::attention;
use crate::vulnix::vulnix_parser::{VulnixParser, VulnixScanOutput};
use anyhow::Result;
use bigdecimal::BigDecimal;
use bigdecimal::FromPrimitive;
#[cfg(test)]
use chrono::Utc;
use sqlx::PgPool;
use sqlx::Row;
use tracing::debug;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CveScanEligibleTarget {
    pub derivation_id: i32,
    pub config_name: String,
    pub hostname: Option<String>,
    pub blocked_reason: Option<String>,
}

/// Durable ownership for an active CVE scan execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CveScanExecutionClaim {
    pub scan_id: Uuid,
    pub derivation_id: i32,
    pub execution_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateCveScanOutcome {
    Created(CveScanExecutionClaim),
    Existing(Uuid),
}

impl CreateCveScanOutcome {
    pub fn id(self) -> Uuid {
        match self {
            Self::Created(claim) => claim.scan_id,
            Self::Existing(id) => id,
        }
    }

    pub fn was_created(self) -> bool {
        matches!(self, Self::Created(_))
    }
}

fn truncate_for_varchar(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

/// Convert an execution UUID to a stable PostgreSQL advisory lock ID.
///
/// Uses the first 8 bytes of the UUID as a signed 64-bit integer.
/// This ensures the same execution UUID always produces the same lock ID.
fn execution_lock_id(execution_id: Uuid) -> i64 {
    let bytes = execution_id.as_bytes();
    i64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

/// Check if an execution lock is currently held by a live PostgreSQL backend.
///
/// Returns `true` if the lock exists and is held by an active session, `false` otherwise.
/// This is used by recovery to distinguish a paused/stalled live process (lock still held)
/// from a crashed process (lock released when its session ended).
///
/// # Advisory lock key reconstruction
///
/// [`acquire_execution_lock`] and [`release_execution_lock`] use the
/// single-bigint form `pg_advisory_lock(key bigint)`. PostgreSQL does not
/// store that 64-bit key directly in `pg_locks`; it splits it into two
/// 32-bit `oid` columns: `classid` holds the upper 32 bits and `objid` holds
/// the lower 32 bits, with `objsubid = 1` identifying the single-bigint form
/// (as opposed to `objsubid = 2` for the two-`int4`-key form). A lookup that
/// compares `objid` directly against the full 64-bit key is a false-negative
/// bug: it silently never matches, so recovery can no longer distinguish a
/// live paused process from a crashed one and can revoke an active execution
/// out from under it. The key must be reconstructed with
/// `(classid::bigint << 32) | objid::bigint` and scoped to `objsubid = 1` and
/// the current database, mirroring exactly how PostgreSQL stores it.
pub async fn execution_lock_is_held(pool: &PgPool, execution_id: Uuid) -> Result<bool> {
    let lock_id = execution_lock_id(execution_id);
    let is_held = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM pg_locks
            WHERE locktype = 'advisory'
              AND objsubid = 1
              AND granted = TRUE
              AND database = (
                  SELECT oid
                  FROM pg_database
                  WHERE datname = current_database()
              )
              AND ((classid::bigint << 32) | objid::bigint) = $1::bigint
        )
        "#,
    )
    .bind(lock_id)
    .fetch_one(pool)
    .await?;
    Ok(is_held)
}

/// Acquire a PostgreSQL session-level advisory lock for an execution.
///
/// This must be called on the same connection that will execute the scan.
/// The lock is held for the lifetime of the connection and automatically released
/// when the connection is returned to the pool or closed.
///
/// Returns `true` if the lock was acquired, `false` if it could not be acquired.
pub async fn try_acquire_execution_lock(conn: &mut sqlx::PgConnection, execution_id: Uuid) -> Result<bool> {
    let lock_id = execution_lock_id(execution_id);
    let acquired = sqlx::query_scalar::<_, bool>(
        "SELECT pg_try_advisory_lock($1::bigint)"
    )
    .bind(lock_id)
    .fetch_one(conn)
    .await?;
    Ok(acquired)
}

/// Acquire a PostgreSQL session-level advisory lock for an execution (blocking).
///
/// This must be called on the same connection that will execute the scan.
/// Blocks until the lock is acquired. The lock is held for the lifetime of the connection
/// and automatically released when the connection is returned to the pool or closed.
pub async fn acquire_execution_lock(conn: &mut sqlx::PgConnection, execution_id: Uuid) -> Result<()> {
    let lock_id = execution_lock_id(execution_id);
    sqlx::query("SELECT pg_advisory_lock($1::bigint)")
        .bind(lock_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Release a PostgreSQL session-level advisory lock for an execution.
///
/// Must be called on the same connection that acquired the lock. The full
/// 64-bit lock_id is bound to $1; PostgreSQL internally splits it.
///
/// # Return value
///
/// Returns `Ok(true)` when `pg_advisory_unlock` confirms this session held
/// and released the lock. Returns `Ok(false)` when the lock was not held by
/// this session (for example, it was already released or never acquired).
///
/// A caller MUST NOT treat `Ok(false)` as equivalent to a successful release:
/// the underlying session may still hold the lock through the state this
/// function could not confirm. Callers holding a pooled connection MUST
/// discard that connection (see [`release_execution_lock_or_close`]) rather
/// than return it to the pool when this function returns `Ok(false)` or an
/// error, because a pooled connection that still owns the lock could be
/// handed to an unrelated execution and silently violate active-scan
/// uniqueness.
pub async fn release_execution_lock(
    conn: &mut sqlx::PgConnection,
    execution_id: Uuid,
) -> Result<bool> {
    let lock_id = execution_lock_id(execution_id);
    let released = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1::bigint)")
        .bind(lock_id)
        .fetch_one(conn)
        .await?;
    Ok(released)
}

/// Releases an execution's advisory lock and disposes of the connection that
/// held it, either by returning it to the pool or by closing it outright.
///
/// # Connection lifecycle
///
/// A PostgreSQL session-level advisory lock is tied to the backend session
/// that acquired it, not to the pooled connection handle. Returning a
/// [`sqlx::pool::PoolConnection`] to the pool via `Drop` does **not**
/// terminate that backend session, so an unconfirmed or failed unlock must
/// not be followed by an ordinary drop: the pool could later hand the same
/// still-locked session to an unrelated execution, letting two callers
/// believe they each hold the lock.
///
/// When [`release_execution_lock`] confirms the lock was released
/// (`Ok(true)`), the connection is returned to the pool normally. Otherwise
/// the connection is closed explicitly via
/// [`sqlx::pool::PoolConnection::close`] so its session — and any lock it
/// might still hold — cannot be reused.
pub async fn release_execution_lock_or_close(
    mut conn: sqlx::pool::PoolConnection<sqlx::Postgres>,
    execution_id: Uuid,
) {
    match release_execution_lock(&mut conn, execution_id).await {
        Ok(true) => {
            // CONCURRENCY: Unlock confirmed by PostgreSQL; the session no
            // longer holds the lock, so returning it to the pool is safe.
        }
        Ok(false) => {
            tracing::error!(
                "Advisory lock for CVE scan execution {execution_id} was not held at \
                 release time; closing the connection instead of returning it to the pool"
            );
            if let Err(err) = conn.close().await {
                tracing::error!(
                    "Failed to close CVE scan execution lock connection for {execution_id}: {err:#}"
                );
            }
            return;
        }
        Err(err) => {
            tracing::error!(
                "Failed to release advisory lock for CVE scan execution {execution_id}: {err:#}; \
                 closing the connection instead of returning it to the pool"
            );
            if let Err(close_err) = conn.close().await {
                tracing::error!(
                    "Failed to close CVE scan execution lock connection for {execution_id}: {close_err:#}"
                );
            }
            return;
        }
    }
    drop(conn);
}

/// Get derivations that need CVE scanning
pub async fn get_targets_needing_cve_scan(
    pool: &PgPool,
    limit: Option<i64>,
    excluded_derivation_ids: &[i32],
    completed_before: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<Derivation>> {
    let limit = limit.unwrap_or(10);
    let completed_before = completed_before.unwrap_or(chrono::DateTime::<chrono::Utc>::MAX_UTC);
    let targets = sqlx::query_as!(
        Derivation,
        r#"
        SELECT 
            d.id, d.commit_id, d.derivation_type as "derivation_type: DerivationType",
            d.derivation_name, d.derivation_path, d.derivation_target,
            d.scheduled_at, d.completed_at, d.started_at, d.attempt_count,
            d.evaluation_duration_ms, d.error_message, d.pname, d.version,
            d.status_id, d.build_elapsed_seconds, d.build_current_target,
            d.build_last_activity_seconds, d.build_last_heartbeat,
            d.cf_agent_enabled, d.store_path
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE ds.name IN ('build-complete', 'complete')
            AND d.derivation_type = 'nixos'
            AND d.store_path IS NOT NULL
            AND NOT (d.id = ANY($2))
            AND COALESCE(d.completed_at, d.scheduled_at, d.started_at) <= $3
            -- Exclude derivations that already have a completed scan
            AND NOT EXISTS (
                SELECT 1 FROM cve_scans cs
                WHERE cs.derivation_id = d.id
                AND cs.status = 'completed'
            )
            -- Exclude derivations with an active (pending/in_progress) scan
            AND NOT EXISTS (
                SELECT 1 FROM cve_scans cs
                WHERE cs.derivation_id = d.id
                AND cs.status IN ('pending', 'in_progress')
            )
            -- Exclude derivations only when there are 5+ consecutive failures
            -- AND the backoff window hasn't elapsed yet (min(30min × failures,
            -- 24h)).  Once the backoff expires the derivation becomes eligible
            -- again — there is no permanent exclusion.
            AND NOT (
                (SELECT COUNT(*) FROM cve_scans cs
                 WHERE cs.derivation_id = d.id
                   AND cs.status = 'failed'
                   AND (cs.created_at > COALESCE(
                       (SELECT MAX(created_at) FROM cve_scans
                        WHERE derivation_id = d.id AND status = 'completed'),
                       '1970-01-01'::timestamp
                   ))
                ) >= 5
                AND NOW() - (
                    SELECT MAX(created_at) FROM cve_scans
                    WHERE derivation_id = d.id AND status = 'failed'
                ) < LEAST(
                    (
                        SELECT COUNT(*) FROM cve_scans cs
                        WHERE cs.derivation_id = d.id
                          AND cs.status = 'failed'
                          AND (cs.created_at > COALESCE(
                              (SELECT MAX(created_at) FROM cve_scans
                               WHERE derivation_id = d.id AND status = 'completed'),
                              '1970-01-01'::timestamp
                          ))
                    ) * INTERVAL '30 minutes',
                    INTERVAL '24 hours'
                )
            )
        ORDER BY d.completed_at ASC NULLS LAST
        LIMIT $1
        "#,
        limit,
        excluded_derivation_ids,
        completed_before
    )
    .fetch_all(pool)
    .await?;
    Ok(targets)
}

/// Create a new CVE scan record.
///
/// Uses `ON CONFLICT DO NOTHING` with the partial unique index
/// `idx_cve_scans_unique_active` to make the claim atomic across concurrent
/// callers (background loop vs. on-demand request). New claims start in
/// `in_progress` immediately so crashes cannot strand a derivation behind a
/// never-recovered `pending` row. If a pending or in-progress scan already
/// exists for the same derivation, this returns the existing scan's ID instead
/// of creating a duplicate.
pub async fn create_cve_scan(
    pool: &PgPool,
    derivation_id: i32,
    scanner_name: &str,
    scanner_version: Option<String>,
) -> Result<CreateCveScanOutcome> {
    let scan_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        &mut tx,
        derivation_id,
    )
    .await?;

    let inserted = sqlx::query(
        r#"
        INSERT INTO cve_scans (
            id, derivation_id, scanner_name, scanner_version,
            status, total_packages, total_vulnerabilities,
            critical_count, high_count, medium_count, low_count,
            attempts, scan_metadata
        ) VALUES (
            $1, $2, $3, $4,
            'in_progress', 0, 0,
            0, 0, 0, 0,
            1,
            jsonb_build_object(
                'execution_id', gen_random_uuid(),
                'execution_started_at', NOW(),
                'execution_heartbeat_at', NOW()
            )
        )
        ON CONFLICT (derivation_id) WHERE status IN ('pending', 'in_progress')
        DO NOTHING
        RETURNING
            id AS scan_id,
            derivation_id,
            (scan_metadata ->> 'execution_id')::uuid AS execution_id
        "#,
    )
    .bind(scan_id)
    .bind(derivation_id)
    .bind(scanner_name)
    .bind(scanner_version)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(inserted) = inserted else {
        // Another caller already claimed this derivation. Prefer an active scan
        // if it still exists; if the winner completed between the conflicting
        // INSERT and this follow-up read, fall back to the newest scan row for
        // the derivation so the loser does not surface a spurious internal
        // error.
        let existing = sqlx::query_scalar!(
            r#"
            SELECT id FROM cve_scans
            WHERE derivation_id = $1
            ORDER BY
              CASE WHEN status IN ('pending', 'in_progress') THEN 0 ELSE 1 END,
              created_at DESC,
              id DESC
            LIMIT 1
            "#,
            derivation_id
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "ON CONFLICT returned 0 rows but no active scan found for derivation {}",
                derivation_id
            )
        })?;
        tx.commit().await?;
        return Ok(CreateCveScanOutcome::Existing(existing));
    };

    let claim = CveScanExecutionClaim {
        scan_id: inserted.get("scan_id"),
        derivation_id: inserted.get("derivation_id"),
        execution_id: inserted.get("execution_id"),
    };
    crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, claim.scan_id)
        .await?;
    tx.commit().await?;
    Ok(CreateCveScanOutcome::Created(claim))
}

/// Complete a CVE scan with results
pub async fn complete_cve_scan(
    pool: &PgPool,
    scan_id: Uuid,
    total_packages: i32,
    total_vulnerabilities: i32,
    critical_count: i32,
    high_count: i32,
    medium_count: i32,
    low_count: i32,
    scan_duration_ms: Option<i32>,
    scan_metadata: Option<serde_json::Value>,
) -> Result<()> {
    complete_cve_scan_for_owner(
        pool,
        scan_id,
        total_packages,
        total_vulnerabilities,
        critical_count,
        high_count,
        medium_count,
        low_count,
        scan_duration_ms,
        scan_metadata,
        None,
    )
    .await
}

/// Complete a token-owned CVE scan. The transition is rejected after the
/// execution lease is revoked or replaced.
#[allow(clippy::too_many_arguments)]
pub async fn complete_cve_scan_for_execution(
    pool: &PgPool,
    scan_id: Uuid,
    total_packages: i32,
    total_vulnerabilities: i32,
    critical_count: i32,
    high_count: i32,
    medium_count: i32,
    low_count: i32,
    scan_duration_ms: Option<i32>,
    scan_metadata: Option<serde_json::Value>,
    execution_id: Uuid,
) -> Result<()> {
    complete_cve_scan_for_owner(
        pool,
        scan_id,
        total_packages,
        total_vulnerabilities,
        critical_count,
        high_count,
        medium_count,
        low_count,
        scan_duration_ms,
        scan_metadata,
        Some(execution_id),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn complete_cve_scan_for_owner(
    pool: &PgPool,
    scan_id: Uuid,
    total_packages: i32,
    total_vulnerabilities: i32,
    critical_count: i32,
    high_count: i32,
    medium_count: i32,
    low_count: i32,
    scan_duration_ms: Option<i32>,
    scan_metadata: Option<serde_json::Value>,
    execution_id: Option<Uuid>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let derivation_id: i32 = sqlx::query_scalar("SELECT derivation_id FROM cve_scans WHERE id=$1")
        .bind(scan_id)
        .fetch_one(&mut *tx)
        .await?;
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        &mut tx,
        derivation_id,
    )
    .await?;
    let result = sqlx::query(
        r#"
        UPDATE cve_scans
        SET
            status = 'completed',
            completed_at = NOW(),
            total_packages = $2,
            total_vulnerabilities = $3,
            critical_count = $4,
            high_count = $5,
            medium_count = $6,
            low_count = $7,
            scan_duration_ms = $8,
            scan_metadata = $9
        WHERE id = $1
          AND status = 'in_progress'
          AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_revoked_at')
          AND (
              ($10::uuid IS NULL AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_id'))
              OR scan_metadata ->> 'execution_id' = $10::uuid::text
          )
        "#,
    )
    .bind(scan_id)
    .bind(total_packages)
    .bind(total_vulnerabilities)
    .bind(critical_count)
    .bind(high_count)
    .bind(medium_count)
    .bind(low_count)
    .bind(scan_duration_ms)
    .bind(scan_metadata)
    .bind(execution_id)
    .execute(&mut *tx)
    .await?;

    require_owned_transition(result.rows_affected(), scan_id, "complete")?;
    crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, scan_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Mark a specific CVE scan as failed.
pub async fn mark_cve_scan_failed(
    pool: &PgPool,
    scan_id: Uuid,
    target: &Derivation,
    error_message: &str,
) -> Result<()> {
    mark_cve_scan_failed_for_owner(pool, scan_id, target, error_message, None).await
}

pub async fn mark_cve_scan_failed_for_execution(
    pool: &PgPool,
    scan_id: Uuid,
    target: &Derivation,
    error_message: &str,
    execution_id: Uuid,
) -> Result<()> {
    mark_cve_scan_failed_for_owner(pool, scan_id, target, error_message, Some(execution_id)).await
}

async fn mark_cve_scan_failed_for_owner(
    pool: &PgPool,
    scan_id: Uuid,
    target: &Derivation,
    error_message: &str,
    execution_id: Option<Uuid>,
) -> Result<()> {
    // Create metadata with error details
    let metadata = serde_json::json!({
        "error": error_message,
        "target_name": target.derivation_name,
        "derivation_path": target.derivation_path
    });

    let mut tx = pool.begin().await?;
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        &mut tx, target.id,
    )
    .await?;
    let result = sqlx::query(
        r#"
        UPDATE cve_scans
        SET
            status = 'failed',
            completed_at = NOW(),
            attempts = attempts + 1,
            scan_metadata = COALESCE(scan_metadata, '{}'::jsonb) || $2
        WHERE id = $1
          AND status = 'in_progress'
          AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_revoked_at')
          AND (
              ($3::uuid IS NULL AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_id'))
              OR scan_metadata ->> 'execution_id' = $3::uuid::text
          )
        "#,
    )
    .bind(scan_id)
    .bind(metadata)
    .bind(execution_id)
    .execute(&mut *tx)
    .await?;

    require_owned_transition(result.rows_affected(), scan_id, "fail")?;
    crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, scan_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Mark a specific CVE scan as failed when the full derivation row is not
/// available (for example, if loading the derivation itself failed).
pub async fn mark_cve_scan_failed_by_id(
    pool: &PgPool,
    scan_id: Uuid,
    derivation_id: i32,
    error_message: &str,
) -> Result<()> {
    mark_cve_scan_failed_by_id_for_owner(pool, scan_id, derivation_id, error_message, None).await
}

pub async fn mark_cve_scan_failed_by_id_for_execution(
    pool: &PgPool,
    scan_id: Uuid,
    derivation_id: i32,
    error_message: &str,
    execution_id: Uuid,
) -> Result<()> {
    mark_cve_scan_failed_by_id_for_owner(
        pool,
        scan_id,
        derivation_id,
        error_message,
        Some(execution_id),
    )
    .await
}

async fn mark_cve_scan_failed_by_id_for_owner(
    pool: &PgPool,
    scan_id: Uuid,
    derivation_id: i32,
    error_message: &str,
    execution_id: Option<Uuid>,
) -> Result<()> {
    let metadata = serde_json::json!({
        "error": error_message,
        "derivation_id": derivation_id,
    });

    let mut tx = pool.begin().await?;
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        &mut tx,
        derivation_id,
    )
    .await?;
    let result = sqlx::query(
        r#"
        UPDATE cve_scans
        SET
            status = 'failed',
            completed_at = NOW(),
            scan_metadata = COALESCE(scan_metadata, '{}'::jsonb) || $2
        WHERE id = $1
          AND status = 'in_progress'
          AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_revoked_at')
          AND (
              ($3::uuid IS NULL AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_id'))
              OR scan_metadata ->> 'execution_id' = $3::uuid::text
          )
        "#,
    )
    .bind(scan_id)
    .bind(metadata)
    .bind(execution_id)
    .execute(&mut *tx)
    .await?;

    require_owned_transition(result.rows_affected(), scan_id, "fail")?;
    crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, scan_id).await?;
    tx.commit().await?;
    Ok(())
}

fn require_owned_transition(rows_affected: u64, scan_id: Uuid, transition: &str) -> Result<()> {
    if rows_affected == 1 {
        return Ok(());
    }

    anyhow::bail!("CVE scan {scan_id} lost execution ownership before it could {transition}")
}

/// Save complete scan results to database
pub async fn save_scan_results(
    pool: &PgPool,
    scan_id: Uuid,
    vulnix_results: &VulnixScanOutput,
    scan_duration_ms: Option<i32>,
) -> Result<()> {
    save_scan_results_for_owner(pool, scan_id, vulnix_results, scan_duration_ms, None, None).await
}

pub async fn save_scan_results_for_execution(
    pool: &PgPool,
    scan_id: Uuid,
    vulnix_results: &VulnixScanOutput,
    scan_duration_ms: Option<i32>,
    execution_id: Uuid,
) -> Result<()> {
    save_scan_results_for_owner(
        pool,
        scan_id,
        vulnix_results,
        scan_duration_ms,
        None,
        Some(execution_id),
    )
    .await
}

pub(crate) async fn save_scan_results_with_store_path_override(
    pool: &PgPool,
    scan_id: Uuid,
    vulnix_results: &VulnixScanOutput,
    scan_duration_ms: Option<i32>,
    store_path_override: Option<&str>,
    execution_id: Uuid,
) -> Result<()> {
    save_scan_results_for_owner(
        pool,
        scan_id,
        vulnix_results,
        scan_duration_ms,
        store_path_override,
        Some(execution_id),
    )
    .await
}

async fn save_scan_results_for_owner(
    pool: &PgPool,
    scan_id: Uuid,
    vulnix_results: &VulnixScanOutput,
    scan_duration_ms: Option<i32>,
    store_path_override: Option<&str>,
    execution_id: Option<Uuid>,
) -> Result<()> {
    // Calculate statistics from vulnix results
    let stats = VulnixParser::calculate_stats(vulnix_results);

    // --- Step 1: resolve all store paths outside the transaction ---
    // `nix-store --query --outputs` can be slow; holding a connection
    // while iterating them starves latency-sensitive API requests.
    let mut resolved: Vec<(&crate::vulnix::vulnix_parser::VulnixEntry, String)> =
        Vec::with_capacity(vulnix_results.len());
    for entry in vulnix_results {
        let store_path = match store_path_override {
            Some(path) => path.to_string(),
            None => get_store_path_from_drv(&entry.derivation).await?,
        };
        debug!(
            "CVE Scan Entry - name: '{}', pname: {:?}, version: {:?}, derivation: '{}', affected_by: {:?}",
            entry.name, entry.pname, entry.version, entry.derivation, entry.affected_by
        );
        resolved.push((entry, store_path));
    }

    // --- Step 2: build in-memory deduplicated arrays for bulk SQL ---

    // Package derivation arrays (one row per vulnix entry)
    let pkg_names: Vec<&str> = resolved.iter().map(|(e, _)| e.name.as_str()).collect();
    let pkg_drv_paths: Vec<&str> = resolved
        .iter()
        .map(|(e, _)| e.derivation.as_str())
        .collect();
    let pkg_pnames: Vec<Option<&str>> = resolved
        .iter()
        .map(|(e, _)| Some(e.pname.as_str()))
        .collect();
    let pkg_versions: Vec<String> = resolved
        .iter()
        .map(|(e, _)| truncate_for_varchar(&e.version, 100))
        .collect();
    let pkg_store_paths: Vec<&str> = resolved.iter().map(|(_, sp)| sp.as_str()).collect();

    // Collect all (cve_id, cvss_score, is_whitelisted, whitelist_reason) tuples
    // deduplicated by cve_id — whitelisted status from any entry wins.
    use std::collections::HashMap;
    #[derive(Debug)]
    struct CveRecord {
        cvss: Option<BigDecimal>,
        is_whitelisted: bool,
        whitelist_reason: Option<String>,
    }
    let mut cve_map: HashMap<String, CveRecord> = HashMap::new();

    // (pkg_name, cve_id, is_whitelisted, whitelist_reason) tuples for package_vulnerabilities
    // We'll resolve derivation_id after the bulk package upsert.
    struct PkgVuln {
        pkg_name: String,
        cve_id: String,
        is_whitelisted: bool,
        whitelist_reason: Option<String>,
    }
    let mut pkg_vulns: Vec<PkgVuln> = Vec::new();

    for (entry, _) in &resolved {
        for cve_id in &entry.affected_by {
            let cvss = entry
                .cvssv3_basescore
                .get(cve_id)
                .copied()
                .and_then(BigDecimal::from_f32);
            cve_map
                .entry(cve_id.clone())
                .and_modify(|r| {
                    if r.cvss.is_none() {
                        r.cvss = cvss.clone();
                    }
                })
                .or_insert(CveRecord {
                    cvss,
                    is_whitelisted: false,
                    whitelist_reason: None,
                });
            pkg_vulns.push(PkgVuln {
                pkg_name: entry.name.clone(),
                cve_id: cve_id.clone(),
                is_whitelisted: false,
                whitelist_reason: None,
            });
        }
        for cve_id in &entry.whitelisted {
            let cvss = entry
                .cvssv3_basescore
                .get(cve_id)
                .copied()
                .and_then(BigDecimal::from_f32);
            cve_map
                .entry(cve_id.clone())
                .and_modify(|r| {
                    if r.cvss.is_none() {
                        r.cvss = cvss.clone();
                    }
                    // Whitelisted status wins on conflict
                    r.is_whitelisted = true;
                    r.whitelist_reason = Some("vulnix whitelist".to_string());
                })
                .or_insert(CveRecord {
                    cvss,
                    is_whitelisted: true,
                    whitelist_reason: Some("vulnix whitelist".to_string()),
                });
            pkg_vulns.push(PkgVuln {
                pkg_name: entry.name.clone(),
                cve_id: cve_id.clone(),
                is_whitelisted: true,
                whitelist_reason: Some("vulnix whitelist".to_string()),
            });
        }
    }

    // PostgreSQL must receive at most one row for each
    // `(package derivation, CVE)` conflict key. A vulnix result can repeat an
    // affected CVE or list the same CVE as both affected and whitelisted. Keep
    // the distinct package/CVE evidence while merging duplicate observations;
    // a whitelist is the authoritative state when observations disagree.
    let mut unique_pkg_vulns: HashMap<(String, String), PkgVuln> = HashMap::new();
    for pkg_vuln in pkg_vulns {
        unique_pkg_vulns
            .entry((pkg_vuln.pkg_name.clone(), pkg_vuln.cve_id.clone()))
            .and_modify(|existing| {
                if pkg_vuln.is_whitelisted {
                    existing.is_whitelisted = true;
                    existing.whitelist_reason = pkg_vuln.whitelist_reason.clone();
                }
            })
            .or_insert(pkg_vuln);
    }
    let pkg_vulns: Vec<PkgVuln> = unique_pkg_vulns.into_values().collect();

    // Collect critical CVE IDs before the transaction so we can open attention
    // occurrences after the commit.  A CVE is considered critical when its CVSS
    // v3 base score is >= 9.0.
    let critical_cve_ids: Vec<String> = cve_map
        .iter()
        .filter(|(_, rec)| {
            rec.cvss
                .as_ref()
                .is_some_and(|s| s >= &BigDecimal::from(9u64))
        })
        .map(|(id, _)| id.clone())
        .collect();

    // --- Step 3: single transaction with ~5 bulk statements ---
    let mut tx = pool.begin().await?;

    // 3a. Mark scan complete
    let completion = sqlx::query(
        r#"
        UPDATE cve_scans
        SET
            status = 'completed',
            completed_at = NOW(),
            total_packages = $2,
            total_vulnerabilities = $3,
            critical_count = $4,
            high_count = $5,
            medium_count = $6,
            low_count = $7,
            scan_duration_ms = $8
        WHERE id = $1
          AND status = 'in_progress'
          AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_revoked_at')
          AND (
              ($9::uuid IS NULL AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_id'))
              OR scan_metadata ->> 'execution_id' = $9::uuid::text
          )
        "#,
    )
    .bind(scan_id)
    .bind(stats.total_packages as i32)
    .bind(stats.total_vulnerabilities as i32)
    .bind(stats.critical_count as i32)
    .bind(stats.high_count as i32)
    .bind(stats.medium_count as i32)
    .bind(stats.low_count as i32)
    .bind(scan_duration_ms)
    .bind(execution_id)
    .execute(&mut *tx)
    .await?;
    require_owned_transition(completion.rows_affected(), scan_id, "persist results")?;

    // 3b. Bulk-insert new package derivations without rewriting unchanged rows.
    // Uses sqlx::query (not sqlx::query!) because UNNEST with multi-column SELECT
    // cannot be represented in the offline SQLx metadata cache.
    sqlx::query(
        r#"
        INSERT INTO derivations (
            commit_id,
            derivation_type,
            derivation_name,
            derivation_path,
            derivation_target,
            pname,
            version,
            status_id,
            store_path,
            attempt_count
        )
        SELECT
            NULL::int,
            'package',
            name,
            drv_path,
            NULL::text,
            pname,
            version,
            11,
            store_path,
            0
        FROM UNNEST(
            $1::text[],
            $2::text[],
            $3::text[],
            $4::text[],
            $5::text[]
        ) AS t(name, drv_path, pname, version, store_path)
        ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type) DO NOTHING
        "#,
    )
    .bind(&pkg_names as &[&str])
    .bind(&pkg_drv_paths as &[&str])
    .bind(&pkg_pnames as &[Option<&str>])
    .bind(&pkg_versions as &[String])
    .bind(&pkg_store_paths as &[&str])
    .execute(&mut *tx)
    .await?;

    // Fetch IDs for both newly inserted and pre-existing packages in one query.
    let pkg_rows = sqlx::query(
        r#"
        SELECT id, derivation_name
        FROM derivations
        WHERE commit_id IS NULL
          AND derivation_type = 'package'
          AND derivation_name = ANY($1::text[])
        "#,
    )
    .bind(&pkg_names as &[&str])
    .fetch_all(&mut *tx)
    .await?;

    let mut name_to_id: HashMap<String, i32> = HashMap::with_capacity(pkg_rows.len());
    let mut pkg_drv_ids: Vec<i32> = Vec::with_capacity(pkg_rows.len());
    for row in pkg_rows {
        let id: i32 = row.get("id");
        let name: String = row.get("derivation_name");
        name_to_id.insert(name, id);
        pkg_drv_ids.push(id);
    }

    // 3c. Bulk-insert scan_packages (ignore duplicates).
    sqlx::query(
        r#"
        INSERT INTO scan_packages (scan_id, derivation_id, is_runtime_dependency, dependency_depth)
        SELECT $1, id, true, 0
        FROM UNNEST($2::int[]) AS t(id)
        ON CONFLICT (scan_id, derivation_id) DO NOTHING
        "#,
    )
    .bind(scan_id)
    .bind(&pkg_drv_ids as &[i32])
    .execute(&mut *tx)
    .await?;

    // 3d. Bulk-upsert CVEs (skip unchanged rows with WHERE clause).
    if !cve_map.is_empty() {
        let cve_ids: Vec<String> = cve_map.keys().cloned().collect();
        let cve_scores: Vec<Option<BigDecimal>> =
            cve_ids.iter().map(|k| cve_map[k].cvss.clone()).collect();
        sqlx::query(
            r#"
            INSERT INTO cves (id, cvss_v3_score)
            SELECT id, score
            FROM UNNEST($1::text[], $2::numeric[]) AS t(id, score)
            ON CONFLICT (id) DO UPDATE SET
                cvss_v3_score = COALESCE(EXCLUDED.cvss_v3_score, cves.cvss_v3_score),
                updated_at    = NOW()
            WHERE cves.cvss_v3_score IS DISTINCT FROM COALESCE(EXCLUDED.cvss_v3_score, cves.cvss_v3_score)
            "#,
        )
        .bind(&cve_ids as &[String])
        .bind(&cve_scores as &[Option<BigDecimal>])
        .execute(&mut *tx)
        .await?;
    }

    // 3e. Bulk-upsert package_vulnerabilities.
    if !pkg_vulns.is_empty() {
        let mut pv_drv_ids: Vec<i32> = Vec::with_capacity(pkg_vulns.len());
        let mut pv_cve_ids: Vec<String> = Vec::with_capacity(pkg_vulns.len());
        let mut pv_whitelisted: Vec<bool> = Vec::with_capacity(pkg_vulns.len());
        let mut pv_reasons: Vec<Option<String>> = Vec::with_capacity(pkg_vulns.len());

        for pv in &pkg_vulns {
            if let Some(&drv_id) = name_to_id.get(&pv.pkg_name) {
                pv_drv_ids.push(drv_id);
                pv_cve_ids.push(pv.cve_id.clone());
                pv_whitelisted.push(pv.is_whitelisted);
                pv_reasons.push(pv.whitelist_reason.clone());
            }
        }

        if !pv_drv_ids.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO package_vulnerabilities (
                    derivation_id, cve_id, detection_method,
                    is_whitelisted, whitelist_reason
                )
                SELECT drv_id, cve_id, 'vulnix', is_wl, reason
                FROM UNNEST(
                    $1::int[],
                    $2::text[],
                    $3::bool[],
                    $4::text[]
                ) AS t(drv_id, cve_id, is_wl, reason)
                ON CONFLICT (derivation_id, cve_id) DO UPDATE SET
                    detection_method  = EXCLUDED.detection_method,
                    is_whitelisted    = EXCLUDED.is_whitelisted,
                    whitelist_reason  = EXCLUDED.whitelist_reason,
                    updated_at        = NOW()
                WHERE package_vulnerabilities.is_whitelisted IS DISTINCT FROM EXCLUDED.is_whitelisted
                   OR package_vulnerabilities.whitelist_reason IS DISTINCT FROM EXCLUDED.whitelist_reason
                "#,
            )
            .bind(&pv_drv_ids as &[i32])
            .bind(&pv_cve_ids as &[String])
            .bind(&pv_whitelisted as &[bool])
            .bind(&pv_reasons as &[Option<String>])
            .execute(&mut *tx)
            .await?;
        }
    }

    // Reconcile CVE attention for every critical CVE found in this scan,
    // AND any currently-open CVE that may have become stale due to this
    // scan's changes.  Doing this inside the same transaction ensures that
    // the cves.fleet_relevant_since episode timestamp is durably recorded
    // atomically with the scan state transition that made the CVE relevant
    // (round 17 review).
    //
    // Sort CVE IDs before acquiring multiple advisory locks so concurrent
    // scan transactions use a deterministic lock order.
    let stale_cve_ids: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT ao.subject_id
        FROM attention_occurrences ao
        WHERE ao.category = 'cves'
          AND ao.resolved_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM view_cve_list_with_metadata v
              WHERE v.cve_id = ao.subject_id
                AND v.severity = 'CRITICAL'
                AND v.affected_count > 0
          )
        "#,
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut affected_cve_ids = critical_cve_ids.clone();
    affected_cve_ids.extend(stale_cve_ids);
    affected_cve_ids.sort();
    affected_cve_ids.dedup();

    for cve_id in &affected_cve_ids {
        attention::reconcile_cve_attention_subject_tx(&mut tx, cve_id).await?;
    }

    crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, scan_id).await?;

    tx.commit().await?;

    Ok(())
}

/// Get latest CVE scan for a derivation
pub async fn get_latest_scan(pool: &PgPool, derivation_id: i32) -> Result<Option<CveScan>> {
    let scan = sqlx::query_as!(
        CveScan,
        r#"
        SELECT 
            id,
            derivation_id as "derivation_id!",
            scheduled_at,
            completed_at,
            status as "status!: ScanStatus",
            attempts as "attempts!",
            scanner_name as "scanner_name!",
            scanner_version,
            total_packages as "total_packages!",
            total_vulnerabilities as "total_vulnerabilities!",
            critical_count as "critical_count!",
            high_count as "high_count!",
            medium_count as "medium_count!",
            low_count as "low_count!",
            scan_duration_ms,
            scan_metadata,
            created_at
        FROM cve_scans
        WHERE derivation_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        derivation_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(scan)
}

/// Count high CVEs (CVSS 7.0–8.9) without whitelist justification for a
/// derivation's most recent completed scan.
///
/// Returns `None` if no completed scan exists for this derivation.
/// Returns `Some(0)` if the latest scan has no unjustified high CVEs.
///
/// The query:
/// 1. Identifies the single most-recent completed scan for the derivation.
/// 2. Counts only `package_vulnerabilities` rows joined through `scan_packages`
///    for that scan — scoping to the latest scan rather than all historical data.
/// 3. Filters for high severity (CVSS 7.0 ≤ score < 9.0) and not whitelisted.
pub async fn count_unjustified_high_cves(pool: &PgPool, derivation_id: i32) -> Result<Option<i64>> {
    // First check whether any completed scan exists; if not, return None so
    // the caller can distinguish "no scan" from "scan with zero findings".
    let latest_scan_id: Option<uuid::Uuid> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM cve_scans
        WHERE derivation_id = $1
          AND status = 'completed'
        ORDER BY completed_at DESC
        LIMIT 1
        "#,
    )
    .bind(derivation_id)
    .fetch_optional(pool)
    .await?;

    let Some(scan_id) = latest_scan_id else {
        return Ok(None);
    };

    // Count unjustified high-severity CVEs scoped to that scan via scan_packages.
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM scan_packages sp
        JOIN package_vulnerabilities pv ON sp.derivation_id = pv.derivation_id
        JOIN cves c ON pv.cve_id = c.id
        WHERE sp.scan_id = $1
          AND c.cvss_v3_score >= 7.0
          AND c.cvss_v3_score < 9.0
          AND (pv.is_whitelisted = false
               OR pv.whitelist_reason IS NULL
               OR pv.whitelist_reason = '')
        "#,
    )
    .bind(scan_id)
    .fetch_one(pool)
    .await?;

    Ok(Some(count))
}

pub async fn get_scan_by_id(pool: &PgPool, scan_id: Uuid) -> Result<Option<CveScan>> {
    let scan = sqlx::query_as::<_, CveScan>(
        r#"
        SELECT
            id,
            derivation_id,
            scheduled_at,
            completed_at,
            status,
            attempts,
            scanner_name,
            scanner_version,
            total_packages,
            total_vulnerabilities,
            critical_count,
            high_count,
            medium_count,
            low_count,
            scan_duration_ms,
            scan_metadata,
            created_at
        FROM cve_scans
        WHERE id = $1
        "#,
    )
    .bind(scan_id)
    .fetch_optional(pool)
    .await?;

    Ok(scan)
}

pub async fn get_active_scan_for_derivation(
    pool: &PgPool,
    derivation_id: i32,
) -> Result<Option<Uuid>> {
    let row = sqlx::query(
        r#"
        SELECT id
        FROM cve_scans
        WHERE derivation_id = $1
          AND status IN ('pending', 'in_progress')
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(derivation_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| r.get::<Uuid, _>("id")))
}

pub async fn resolve_flake_config_cve_scan_target(
    pool: &PgPool,
    flake_id: i32,
    config_name: &str,
) -> Result<Option<CveScanEligibleTarget>> {
    let row = sqlx::query(
        r#"
        WITH latest_host AS (
            SELECT s.hostname
            FROM systems s
            WHERE s.flake_id = $1
              AND s.is_active = TRUE
              AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) = $2
            ORDER BY s.updated_at DESC, s.created_at DESC
            LIMIT 1
        )
        SELECT
            d.id AS derivation_id,
            d.derivation_name AS config_name,
            (SELECT hostname FROM latest_host) AS hostname,
            CASE
                WHEN d.store_path IS NULL THEN 'Build output is unavailable for this configuration.'
                WHEN NOT EXISTS (
                    SELECT 1
                    FROM cache_push_jobs cpj
                    WHERE cpj.derivation_id = d.id
                      AND cpj.status = 'completed'
                ) THEN 'CVE scan requires a completed cache push for this configuration.'
                ELSE NULL
            END AS blocked_reason
        FROM derivations d
        JOIN commits c ON c.id = d.commit_id
        WHERE c.flake_id = $1
          AND d.derivation_type = 'nixos'
          AND d.derivation_name = $2
        ORDER BY
            (
                SELECT MAX(cpj.completed_at)
                FROM cache_push_jobs cpj
                WHERE cpj.derivation_id = d.id
                  AND cpj.status = 'completed'
            ) DESC NULLS LAST,
            d.completed_at DESC NULLS LAST,
            d.id DESC
        LIMIT 1
        "#,
    )
    .bind(flake_id)
    .bind(config_name)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| CveScanEligibleTarget {
        derivation_id: r.get("derivation_id"),
        config_name: r.get("config_name"),
        hostname: r.get("hostname"),
        blocked_reason: r.get("blocked_reason"),
    }))
}

pub async fn resolve_system_cve_scan_target(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<CveScanEligibleTarget>> {
    let row = sqlx::query(
        r#"
        WITH selected_system AS (
            SELECT
                s.id,
                s.hostname,
                s.flake_id,
                COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) AS config_name
            FROM systems s
            WHERE s.id = $1
              AND s.is_active = TRUE
        )
        SELECT
            d.id AS derivation_id,
            ss.config_name,
            ss.hostname,
            CASE
                WHEN d.store_path IS NULL THEN 'Build output is unavailable for this system configuration.'
                WHEN NOT EXISTS (
                    SELECT 1
                    FROM cache_push_jobs cpj
                    WHERE cpj.derivation_id = d.id
                      AND cpj.status = 'completed'
                ) THEN 'CVE scan requires a completed cache push for this system configuration.'
                ELSE NULL
            END AS blocked_reason
        FROM selected_system ss
        JOIN commits c ON c.flake_id = ss.flake_id
        JOIN derivations d ON d.commit_id = c.id
        WHERE d.derivation_type = 'nixos'
          AND d.derivation_name = ss.config_name
        ORDER BY
            (
                SELECT MAX(cpj.completed_at)
                FROM cache_push_jobs cpj
                WHERE cpj.derivation_id = d.id
                  AND cpj.status = 'completed'
            ) DESC NULLS LAST,
            d.completed_at DESC NULLS LAST,
            d.id DESC
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| CveScanEligibleTarget {
        derivation_id: r.get("derivation_id"),
        config_name: r.get("config_name"),
        hostname: r.get("hostname"),
        blocked_reason: r.get("blocked_reason"),
    }))
}

/// Selects the derivation currently *running* on each active system.
///
/// "Deployed" here means the same thing it means in
/// [`get_targets_needing_cve_rescan`]: the derivation's `store_path` equals the
/// latest `system_states.store_path` reported for that hostname. This
/// deliberately excludes newer builds that have not been activated and older
/// historical generations, so a fleet rescan reports on what is actually
/// running rather than on what would ship next.
///
/// Systems sharing a derivation are collapsed to a single row so a fleet
/// request cannot enqueue duplicate work for the same store path.
const FLEET_TARGET_SELECT: &str = r#"
    SELECT DISTINCT ON (d.id)
        d.id AS derivation_id,
        d.derivation_name AS config_name,
        s.hostname
    FROM systems s
    JOIN commits c ON c.flake_id = s.flake_id
    JOIN derivations d ON d.commit_id = c.id
    WHERE s.is_active = TRUE
      AND d.derivation_type = 'nixos'
      AND d.store_path IS NOT NULL
      AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
          = d.derivation_name
      AND d.store_path = (
          SELECT ss.store_path
          FROM system_states ss
          WHERE ss.hostname = s.hostname
            AND ss.store_path IS NOT NULL
            AND BTRIM(ss.store_path) <> ''
          ORDER BY ss.timestamp DESC
          LIMIT 1
      )
    ORDER BY d.id, s.hostname
"#;

/// Outcome of an atomic fleet enqueue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FleetEnqueueOutcome {
    /// Distinct derivations currently running across active systems.
    pub eligible: i64,
    /// Rows newly inserted as `pending` by this request.
    pub created: i64,
}

impl FleetEnqueueOutcome {
    /// Targets skipped because a pending or in-progress scan already existed.
    pub fn reused(self) -> i64 {
        (self.eligible - self.created).max(0)
    }
}

/// Resolve the distinct derivations currently running on active systems.
pub async fn get_fleet_cve_scan_targets(pool: &PgPool) -> Result<Vec<CveScanEligibleTarget>> {
    let rows = sqlx::query(FLEET_TARGET_SELECT).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|row| CveScanEligibleTarget {
            derivation_id: row.get("derivation_id"),
            config_name: row.get("config_name"),
            hostname: row.get("hostname"),
            blocked_reason: None,
        })
        .collect())
}

/// Atomically enqueue `pending` CVE scans for every currently running
/// derivation across the active fleet.
///
/// This performs the whole operation in a single statement so the request has
/// no partial-failure window: either the enqueue succeeds and the returned
/// counts describe exactly what happened, or nothing is written at all. That is
/// why the handler can report an accurate count instead of failing after having
/// already started work.
///
/// Deduplication is provided by the partial unique index
/// `idx_cve_scans_unique_active`, which covers `pending` and `in_progress`, so
/// a derivation with an active scan is skipped rather than duplicated. No
/// scan is executed here; the worker drains queued rows at its own bounded
/// rate.
pub async fn enqueue_fleet_cve_scans(
    pool: &PgPool,
    scanner_name: &str,
    scanner_version: Option<String>,
) -> Result<FleetEnqueueOutcome> {
    let sql = format!(
        r#"
        WITH targets AS (
            {FLEET_TARGET_SELECT}
        ),
        inserted AS (
            INSERT INTO cve_scans (
                id, derivation_id, scanner_name, scanner_version,
                status, total_packages, total_vulnerabilities,
                critical_count, high_count, medium_count, low_count,
                attempts
            )
            SELECT
                gen_random_uuid(), t.derivation_id, $1, $2,
                'pending', 0, 0,
                0, 0, 0, 0,
                0
            FROM targets t
            ON CONFLICT (derivation_id) WHERE status IN ('pending', 'in_progress')
            DO NOTHING
            RETURNING 1
        )
        SELECT
            (SELECT COUNT(*) FROM targets)::bigint  AS eligible,
            (SELECT COUNT(*) FROM inserted)::bigint AS created
        "#
    );

    let row = sqlx::query(&sql)
        .bind(scanner_name)
        .bind(scanner_version)
        .fetch_one(pool)
        .await?;

    Ok(FleetEnqueueOutcome {
        eligible: row.get("eligible"),
        created: row.get("created"),
    })
}

/// Atomically claim queued CVE scans for execution.
///
/// # Composite-assessment atomicity
///
/// Claiming `pending -> in_progress` and invalidating any composite
/// assessment's prior `cve_block` Pass for the same derivation commit in the
/// same transaction. [`scan_outcome`][crate::services::composite_enforcement]
/// treats an `in_progress` scan as `NotChecked`/blocking, so a Pass persisted
/// while the derivation had only a completed scan MUST stop authorizing
/// deployment as soon as a retry actually starts, not merely when it is
/// queued as `pending`. Persisting the claim and the composite recomputation
/// separately would leave a window where a reader could observe the scan as
/// `in_progress` while the composite assessment still reports the old Pass.
/// This function calls [`persist_scan_phase_in_tx`][composite_persist] inside
/// the exact transaction that performs the claim so no such window is ever
/// committed.
///
/// [composite_persist]: crate::services::composite_enforcement::persist_scan_phase_in_tx
///
/// # Claim procedure and lock order
///
/// Candidate rows are first identified with a plain, unlocked `SELECT`. The
/// actual exclusivity comes from the per-candidate guarded `UPDATE ... WHERE
/// status = 'pending'` below, so two callers that both see the same candidate
/// cannot both win it: PostgreSQL's row lock serializes their `UPDATE`
/// statements, and the loser's `WHERE status = 'pending'` predicate no longer
/// matches once the winner commits.
///
/// Each candidate is claimed in its own short transaction that:
///
/// 1. Acquires the established POA&M/composite derivation lock via
///    [`lock_poam_findings_for_derivation_tx`][lock_poam] — the same lock
///    every other CVE scan transition (create/complete/fail/revoke) acquires
///    before mutating `cve_scans`, preserving the repository's writer lock
///    order.
/// 2. Performs the guarded claim UPDATE for that one row.
/// 3. Calls `persist_scan_phase_in_tx` for the newly claimed scan.
/// 4. Commits.
///
/// A claim is only appended to the returned vector after its transaction
/// commits, so a caller never receives a claim for a scan whose composite
/// invalidation did not durably commit alongside it.
///
/// The claim timestamp is recorded in `scan_metadata.execution_started_at` so
/// stale recovery can age a scan from when execution started rather than from
/// when it entered the queue. Those are the same instant for worker-created
/// scans, but can be far apart for operator-queued fleet scans.
///
/// [lock_poam]: crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx
pub async fn claim_queued_cve_scans(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<CveScanExecutionClaim>> {
    let candidates: Vec<(Uuid, i32)> = sqlx::query_as(
        r#"
        SELECT id, derivation_id
        FROM cve_scans
        WHERE status = 'pending'
        ORDER BY created_at ASC, id ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut claims = Vec::with_capacity(candidates.len());
    for (scan_id, derivation_id) in candidates {
        let mut tx = pool.begin().await?;
        // CONCURRENCY: Acquire the derivation's POA&M/composite lock before
        // the claim mutation, matching every other CVE scan writer. This
        // also serializes against a concurrent caller that identified the
        // same candidate: the loser blocks here until the winner commits,
        // then its guarded UPDATE below affects zero rows.
        crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
            &mut tx,
            derivation_id,
        )
        .await?;

        let claimed = sqlx::query(
            r#"
            UPDATE cve_scans
            SET status = 'in_progress',
                attempts = attempts + 1,
                scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
                    || jsonb_build_object(
                        'execution_id', gen_random_uuid(),
                        'execution_started_at', NOW(),
                        'execution_heartbeat_at', NOW()
                    )
            WHERE id = $1
              AND status = 'pending'
            RETURNING
                id AS scan_id,
                derivation_id,
                (scan_metadata ->> 'execution_id')::uuid AS execution_id
            "#,
        )
        .bind(scan_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = claimed else {
            // Another caller already claimed this row between the unlocked
            // candidate read above and this transaction's guarded UPDATE.
            tx.rollback().await?;
            continue;
        };

        let claim = CveScanExecutionClaim {
            scan_id: row.get("scan_id"),
            derivation_id: row.get("derivation_id"),
            execution_id: row.get("execution_id"),
        };
        // INVARIANT: The claim transition and the composite recomputation
        // that invalidates the prior Pass commit together. No observer can
        // see this scan `in_progress` while its composite assessment still
        // reports stale Pass evidence.
        crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, claim.scan_id)
            .await?;
        tx.commit().await?;
        claims.push(claim);
    }

    Ok(claims)
}

/// Refresh a queued execution lease. A zero-row result means this worker no
/// longer owns an active claim and must stop before persisting scan results.
pub async fn heartbeat_cve_scan_execution(
    pool: &PgPool,
    scan_id: Uuid,
    execution_id: Uuid,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE cve_scans
        SET scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
            || jsonb_build_object('execution_heartbeat_at', NOW())
        WHERE id = $1
          AND status = 'in_progress'
          AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_revoked_at')
          AND scan_metadata ->> 'execution_id' = $2::uuid::text
        "#,
    )
    .bind(scan_id)
    .bind(execution_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// Finalizes a stale execution after its scanner future has been dropped.
///
/// Recovery first records `execution_revoked_at` while retaining active-scan
/// uniqueness. The former owner calls this function after it cancels its live
/// process. The scan failure and the corresponding composite assessment update
/// commit in one transaction, so durable enforcement state cannot retain the
/// prior in-progress scan outcome.
///
/// # Errors
///
/// Returns an error when the transaction, finding lock, scan transition, or
/// composite assessment update fails.
pub async fn acknowledge_revoked_cve_scan_execution(
    pool: &PgPool,
    scan_id: Uuid,
    execution_id: Uuid,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let Some(derivation_id) =
        sqlx::query_scalar::<_, i32>("SELECT derivation_id FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .fetch_optional(&mut *tx)
            .await?
    else {
        tx.rollback().await?;
        return Ok(false);
    };
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        &mut tx,
        derivation_id,
    )
    .await?;
    let result = sqlx::query(
        r#"
        UPDATE cve_scans
        SET status = 'failed',
            completed_at = NOW(),
            attempts = attempts + 1,
            scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
                || jsonb_build_object(
                    'stale_recovered_at', NOW(),
                    'stale_recovery_reason', 'execution-revocation-acknowledged'
                )
        WHERE id = $1
          AND status = 'in_progress'
          AND scan_metadata ->> 'execution_id' = $2::uuid::text
          AND scan_metadata ? 'execution_revoked_at'
        "#,
    )
    .bind(scan_id)
    .bind(execution_id)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() != 1 {
        tx.rollback().await?;
        return Ok(false);
    }

    crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, scan_id).await?;
    tx.commit().await?;
    Ok(true)
}

/// Return an owned but not-yet-started execution to the queue.
///
/// The execution metadata is removed so a future claim receives a new token.
/// The attempt count remains as audit history for the aborted claim.
pub async fn requeue_cve_scan_execution(
    pool: &PgPool,
    scan_id: Uuid,
    execution_id: Uuid,
    reason: &str,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE cve_scans
        SET status = 'pending',
            scan_metadata = (
                COALESCE(scan_metadata, '{}'::jsonb)
                - 'execution_id'
                - 'execution_started_at'
                - 'execution_heartbeat_at'
            ) || jsonb_build_object(
                'requeued_at', NOW(),
                'requeue_reason', $3::text
            )
        WHERE id = $1
          AND status = 'in_progress'
          AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_revoked_at')
          AND scan_metadata ->> 'execution_id' = $2::uuid::text
        "#,
    )
    .bind(scan_id)
    .bind(execution_id)
    .bind(reason)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// Revoke scans stuck `in_progress` beyond the threshold without immediately
/// releasing active-scan uniqueness. A live owner observes the revocation on
/// its next heartbeat, cancels its scanner, and acknowledges the revocation as
/// failed. If the owner crashed, recovery finalizes revocations only when the
/// execution lock is no longer held by any PostgreSQL backend session.
///
/// # Ownership Fencing
///
/// Each execution holds a PostgreSQL session-level advisory lock for its entire
/// lifetime. Recovery revokes a scan only if:
///
/// 1. The heartbeat is stale (older than `stale_threshold`), AND
/// 2. Either the scan has no execution token (legacy row), OR the advisory lock
///    is no longer held by any live backend.
///
/// This prevents a paused Tokio runtime or delayed server process from being
/// replaced while its scanner process still exists and holds the lock.
///
/// Returns the number of scans that were recovered so callers can log the event.
pub async fn recover_stale_scans(
    pool: &PgPool,
    stale_threshold: std::time::Duration,
) -> Result<i64> {
    // Age from when execution actually started, not from when the row was
    // created. Operator-queued fleet scans can sit in the queue for a long time
    // before a worker claims them, so aging from `created_at` would let a
    // second process classify a scan that just began as already stale and fail
    // it out from under the worker running it. `execution_started_at` is written
    // by every current execution claim path. The `created_at` fallback remains
    // only for legacy in-progress rows created before execution leases existed.
    let mut tx = pool.begin().await?;
    let candidates = sqlx::query_as::<_, (Uuid, Option<String>)>(
        r#"
        SELECT id, scan_metadata ->> 'execution_id' AS execution_id
        FROM cve_scans
        WHERE status = 'in_progress'
          AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_revoked_at')
          AND COALESCE(
                  (scan_metadata ->> 'execution_heartbeat_at')::timestamptz,
                  (scan_metadata ->> 'execution_started_at')::timestamptz,
                  created_at
              ) < NOW() - ($1::bigint) * INTERVAL '1 second'
        "#,
    )
    .bind(stale_threshold.as_secs() as i64)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    // Check each candidate to see if its execution lock is still held by a live backend.
    // Revoke only candidates whose locks are not held.
    let mut revoked_ids = Vec::new();
    for (scan_id, execution_id_opt) in candidates {
        let should_revoke = if let Some(execution_id_str) = execution_id_opt {
            // Parse the execution UUID and check if its lock is held.
            match Uuid::parse_str(&execution_id_str) {
                Ok(execution_id) => {
                    // Lock is not held means the session crashed or ended.
                    // If we cannot determine lock status, do NOT revoke —
                    // fail closed to preserve ownership fencing (P1-3 fix).
                    let lock_is_held = execution_lock_is_held(pool, execution_id)
                        .await
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "failed to inspect execution lock for scan {}: {}",
                                scan_id, e
                            )
                        })?;
                    !lock_is_held
                }
                Err(_) => {
                    // Invalid UUID in metadata; revoke it.
                    true
                }
            }
        } else {
            // Legacy row with no execution token; revoke if old enough.
            true
        };

        if should_revoke {
            let mut tx = pool.begin().await?;
            let result = sqlx::query(
                r#"
                UPDATE cve_scans
                SET scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
                    || jsonb_build_object(
                        'execution_revoked_at', NOW(),
                        'stale_recovery_reason', 'stale-execution-revoked'
                    )
                WHERE id = $1
                  AND status = 'in_progress'
                  AND NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_revoked_at')
                "#,
            )
            .bind(scan_id)
            .execute(&mut *tx)
            .await?;

            if result.rows_affected() > 0 {
                revoked_ids.push(scan_id);
            }
            tx.commit().await?;
        }
    }

    // Second pass: finalize revocations that are old enough to be confident
    // the owner has observed them and either acknowledged or crashed.
    const REVOCATION_ACKNOWLEDGMENT_GRACE_SECONDS: i64 = 60;
    let mut tx = pool.begin().await?;
    let failed_ids = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE cve_scans
        SET status = 'failed',
            completed_at = NOW(),
            attempts = attempts + 1,
            scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
                || jsonb_build_object(
                    'stale_recovered_at', NOW(),
                    'stale_recovery_reason', 'server-crash-recovery'
                )
        WHERE status = 'in_progress'
          AND (
              (
                  NOT (COALESCE(scan_metadata, '{}'::jsonb) ? 'execution_id')
                  AND created_at < NOW() - ($1::bigint) * INTERVAL '1 second'
              )
              OR (scan_metadata ->> 'execution_revoked_at')::timestamptz
                  < NOW() - ($2::bigint) * INTERVAL '1 second'
          )
        RETURNING id
        "#,
    )
    .bind(stale_threshold.as_secs() as i64)
    .bind(REVOCATION_ACKNOWLEDGMENT_GRACE_SECONDS)
    .fetch_all(&mut *tx)
    .await?;

    for scan_id in &failed_ids {
        crate::services::composite_enforcement::persist_scan_phase_in_tx(&mut tx, *scan_id).await?;
    }

    tx.commit().await?;
    Ok((revoked_ids.len() + failed_ids.len()) as i64)
}

/// Get derivations whose most recent completed CVE scan is stale according to
/// the configured `scan_schedule_policy` intervals.
///
/// The lifecycle class (deployed / recent / archived) is derived from actual
/// system and build state, **not** from scan age:
///
/// - **deployed** — the derivation name matches an active system row
///   (`systems.is_active = TRUE`).  Uses `deployed_interval`.
/// - **recent** — the derivation was built within the last 30 days but is not
///   currently deployed.  Uses `recent_interval`.
/// - **archived** — built more than 30 days ago and not deployed.  Uses
///   `archived_interval` (only when `archived_enabled = TRUE`).
///
/// Each class’s interval can be set to `never` in the UI, in which case that
/// class is never selected for rescan.
///
/// Derivations with an active pending/in_progress scan or with ≥ 5 total failed
/// scan rows are excluded to avoid double-scanning or hammering
/// permanently-failing targets.
///
/// Uses dynamic SQL (not `query!`) because interval strings are stored as text
/// in the `scan_schedule_policy` singleton row.
pub async fn get_targets_needing_cve_rescan(
    pool: &PgPool,
    limit: Option<i64>,
) -> anyhow::Result<Vec<Derivation>> {
    let limit = limit.unwrap_or(10);

    let rows = sqlx::query(
        r#"
        WITH policy AS (
            SELECT
                deployed_interval,
                recent_interval,
                archived_interval,
                archived_enabled
            FROM scan_schedule_policy
            WHERE id = 1
        ),
        -- Lifecycle class derived from system deployment + build freshness,
        -- NOT from scan age.
        --
        -- "deployed" uses an EXISTS check against active systems, matching on:
        --   - flake (via commits → systems.flake_id)
        --   - effective configuration name (system_configuration_name or hostname)
        --   - the derivation's store path matches the latest system_states
        --     store_path for that hostname (ensures only the currently deployed
        --     generation is classified as deployed, not every historical build).
        lifecycle AS (
            SELECT
                d.id AS derivation_id,
                CASE
                    WHEN EXISTS (
                        SELECT 1
                        FROM systems s
                        JOIN commits c ON c.flake_id = s.flake_id
                        WHERE c.id = d.commit_id
                          AND s.is_active = TRUE
                          AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) = d.derivation_name
                          AND d.store_path IS NOT NULL
                          AND d.store_path = (
                              SELECT ss.store_path
                              FROM system_states ss
                              WHERE ss.hostname = s.hostname
                                AND ss.store_path IS NOT NULL
                                AND BTRIM(ss.store_path) <> ''
                              ORDER BY ss.timestamp DESC
                              LIMIT 1
                          )
                    ) THEN 'deployed'
                    WHEN d.completed_at >= NOW() - INTERVAL '30 days' THEN 'recent'
                    ELSE 'archived'
                END AS lifecycle_class
            FROM derivations d
            WHERE d.derivation_type = 'nixos'
              AND d.store_path IS NOT NULL
        ),
        latest_completed AS (
            SELECT DISTINCT ON (derivation_id)
                derivation_id,
                completed_at
            FROM cve_scans
            WHERE status = 'completed'
            ORDER BY derivation_id, completed_at DESC
        )
        SELECT
            d.id,
            d.commit_id,
            d.derivation_type,
            d.derivation_name,
            d.derivation_path,
            d.derivation_target,
            d.scheduled_at,
            d.completed_at,
            d.started_at,
            d.attempt_count,
            d.evaluation_duration_ms,
            d.error_message,
            d.pname,
            d.version,
            d.status_id,
            d.build_elapsed_seconds,
            d.build_current_target,
            d.build_last_activity_seconds,
            d.build_last_heartbeat,
            d.cf_agent_enabled,
            d.store_path
        FROM derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        JOIN lifecycle lc ON lc.derivation_id = d.id
        JOIN latest_completed lcs ON lcs.derivation_id = d.id
        CROSS JOIN policy p
        WHERE ds.name IN ('build-complete', 'complete')
            AND d.derivation_type = 'nixos'
            AND d.store_path IS NOT NULL
            -- Exclude derivations with an active scan already under way
            AND NOT EXISTS (
                SELECT 1 FROM cve_scans cs
                WHERE cs.derivation_id = d.id
                  AND cs.status IN ('pending', 'in_progress')
            )
            -- Exclude derivations only when there are 5+ consecutive failures
            -- AND the backoff window hasn't elapsed yet (min(30min × failures,
            -- 24h)).  Once the backoff expires the derivation becomes eligible
            -- again — there is no permanent exclusion.
            AND NOT (
                (SELECT COUNT(*) FROM cve_scans cs
                 WHERE cs.derivation_id = d.id
                   AND cs.status = 'failed'
                   AND (cs.created_at > COALESCE(
                       (SELECT MAX(created_at) FROM cve_scans
                        WHERE derivation_id = d.id AND status = 'completed'),
                       '1970-01-01'::timestamp
                   ))
                ) >= 5
                AND NOW() - (
                    SELECT MAX(created_at) FROM cve_scans
                    WHERE derivation_id = d.id AND status = 'failed'
                ) < LEAST(
                    (
                        SELECT COUNT(*) FROM cve_scans cs
                        WHERE cs.derivation_id = d.id
                          AND cs.status = 'failed'
                          AND (cs.created_at > COALESCE(
                              (SELECT MAX(created_at) FROM cve_scans
                               WHERE derivation_id = d.id AND status = 'completed'),
                              '1970-01-01'::timestamp
                          ))
                    ) * INTERVAL '30 minutes',
                    INTERVAL '24 hours'
                )
            )
            -- Staleness check per lifecycle class, skipping 'never' intervals
            AND (
                (lc.lifecycle_class = 'deployed'
                    AND p.deployed_interval != 'never'
                    AND NOW() - lcs.completed_at > p.deployed_interval::INTERVAL)
                OR
                (lc.lifecycle_class = 'recent'
                    AND p.recent_interval != 'never'
                    AND NOW() - lcs.completed_at > p.recent_interval::INTERVAL)
                OR
                (lc.lifecycle_class = 'archived'
                    AND p.archived_enabled = TRUE
                    AND p.archived_interval != 'never'
                    AND NOW() - lcs.completed_at > p.archived_interval::INTERVAL)
            )
        ORDER BY lcs.completed_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let derivations = rows
        .into_iter()
        .map(|row| {
            let derivation_type_str: String = row.try_get("derivation_type").unwrap_or_default();
            let derivation_type = match derivation_type_str.as_str() {
                "package" => crate::derivations::DerivationType::Package,
                _ => crate::derivations::DerivationType::NixOS,
            };
            Derivation {
                id: row.try_get("id").unwrap_or(0),
                commit_id: row.try_get("commit_id").ok(),
                derivation_type,
                derivation_name: row.try_get("derivation_name").unwrap_or_default(),
                derivation_path: row.try_get("derivation_path").ok(),
                derivation_target: row.try_get("derivation_target").ok(),
                scheduled_at: row.try_get("scheduled_at").ok(),
                completed_at: row.try_get("completed_at").ok(),
                started_at: row.try_get("started_at").ok(),
                attempt_count: row.try_get("attempt_count").unwrap_or(0),
                evaluation_duration_ms: row.try_get("evaluation_duration_ms").ok(),
                error_message: row.try_get("error_message").ok(),
                pname: row.try_get("pname").ok(),
                version: row.try_get("version").ok(),
                status_id: row.try_get("status_id").unwrap_or(0),
                build_elapsed_seconds: row.try_get("build_elapsed_seconds").ok(),
                build_current_target: row.try_get("build_current_target").ok(),
                build_last_activity_seconds: row.try_get("build_last_activity_seconds").ok(),
                build_last_heartbeat: row.try_get("build_last_heartbeat").ok(),
                cf_agent_enabled: row.try_get("cf_agent_enabled").ok(),
                store_path: row.try_get("store_path").ok(),
            }
        })
        .collect();

    Ok(derivations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::commits::{get_commit_by_id, insert_commit};
    use crate::queries::derivations::{EvaluationStatus, insert_derivation};
    use crate::queries::flakes::insert_flake;

    struct FleetFixture {
        flake_id: i32,
        environment_id: Uuid,
        derivation_ids: Vec<i32>,
        hosts: Vec<String>,
        running_id: i32,
        newer_id: i32,
    }

    impl FleetFixture {
        async fn cleanup(&self, pool: &PgPool) {
            sqlx::query("DELETE FROM cve_scans WHERE derivation_id = ANY($1)")
                .bind(&self.derivation_ids)
                .execute(pool)
                .await
                .expect("fixture CVE scans should be deleted");
            sqlx::query("DELETE FROM system_states WHERE hostname = ANY($1)")
                .bind(&self.hosts)
                .execute(pool)
                .await
                .expect("fixture system states should be deleted");
            sqlx::query("DELETE FROM systems WHERE environment_id = $1")
                .bind(self.environment_id)
                .execute(pool)
                .await
                .expect("fixture systems should be deleted");
            sqlx::query("DELETE FROM derivations WHERE id = ANY($1)")
                .bind(&self.derivation_ids)
                .execute(pool)
                .await
                .expect("fixture derivations should be deleted");
            sqlx::query("DELETE FROM commits WHERE flake_id = $1")
                .bind(self.flake_id)
                .execute(pool)
                .await
                .expect("fixture commits should be deleted");
            sqlx::query("DELETE FROM flakes WHERE id = $1")
                .bind(self.flake_id)
                .execute(pool)
                .await
                .expect("fixture flake should be deleted");
            sqlx::query("DELETE FROM environments WHERE id = $1")
                .bind(self.environment_id)
                .execute(pool)
                .await
                .expect("fixture environment should be deleted");
        }
    }

    async fn setup_fleet_fixture(pool: &PgPool) -> FleetFixture {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.com/task-325-{suffix}.git");
        let flake = insert_flake(
            pool,
            &format!("task-325-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake should be inserted");
        insert_commit(pool, &format!("{suffix}00000000"), &repo_url, Utc::now())
            .await
            .expect("commit should be inserted");
        let commit_id: i32 = sqlx::query_scalar(
            "SELECT id FROM commits WHERE flake_id = $1 ORDER BY id DESC LIMIT 1",
        )
        .bind(flake.id)
        .fetch_one(pool)
        .await
        .expect("commit should resolve");
        let commit = get_commit_by_id(pool, commit_id)
            .await
            .expect("commit model should resolve");

        let config_name = format!("shared-config-{suffix}");
        let running_path = format!("/nix/store/{suffix}-running");
        let newer_path = format!("/nix/store/{suffix}-newer");
        let running = insert_derivation(pool, Some(&commit), &config_name, "nixos")
            .await
            .expect("running derivation should be inserted");
        sqlx::query(
            "UPDATE derivations SET status_id = $2, completed_at = NOW() - INTERVAL '1 day', store_path = $3 WHERE id = $1",
        )
        .bind(running.id)
        .bind(EvaluationStatus::BuildComplete.as_id())
        .bind(&running_path)
        .execute(pool)
        .await
        .expect("running derivation should be build-complete");

        // A second commit is required because derivations are unique per
        // commit/configuration. Reusing the first commit would make
        // `insert_derivation` return the running row and overwrite its path,
        // invalidating the fixture this test is meant to prove.
        insert_commit(
            pool,
            &format!("{suffix}11111111"),
            &repo_url,
            Utc::now() + chrono::Duration::seconds(1),
        )
        .await
        .expect("newer commit should be inserted");
        let newer_commit_id: i32 = sqlx::query_scalar(
            "SELECT id FROM commits WHERE flake_id = $1 ORDER BY id DESC LIMIT 1",
        )
        .bind(flake.id)
        .fetch_one(pool)
        .await
        .expect("newer commit should resolve");
        let newer_commit = get_commit_by_id(pool, newer_commit_id)
            .await
            .expect("newer commit model should resolve");
        let newer = insert_derivation(pool, Some(&newer_commit), &config_name, "nixos")
            .await
            .expect("newer derivation should be inserted");
        sqlx::query(
            "UPDATE derivations SET status_id = $2, completed_at = NOW(), store_path = $3 WHERE id = $1",
        )
        .bind(newer.id)
        .bind(EvaluationStatus::BuildComplete.as_id())
        .bind(&newer_path)
        .execute(pool)
        .await
        .expect("newer derivation should be build-complete");

        let environment_id = Uuid::new_v4();
        sqlx::query("INSERT INTO environments (id, name, is_active) VALUES ($1, $2, TRUE)")
            .bind(environment_id)
            .bind(format!("task-325-env-{suffix}"))
            .execute(pool)
            .await
            .expect("environment should be inserted");

        let hosts = vec![
            format!("active-a-{suffix}"),
            format!("active-b-{suffix}"),
            format!("inactive-{suffix}"),
            format!("nostate-{suffix}"),
        ];
        for (index, hostname) in hosts.iter().enumerate() {
            let is_active = index != 2;
            let has_state = index != 3;
            sqlx::query(
                r#"
                INSERT INTO systems (
                    id, hostname, environment_id, is_active, public_key, flake_id,
                    derivation, system_configuration_name
                )
                VALUES ($1, $2, $3, $4, 'test-key', $5, 'test-derivation', $6)
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(hostname)
            .bind(environment_id)
            .bind(is_active)
            .bind(flake.id)
            .bind(&config_name)
            .execute(pool)
            .await
            .expect("system should be inserted");

            if has_state {
                sqlx::query(
                    "INSERT INTO system_states (hostname, store_path, change_reason, timestamp)
                     VALUES ($1, $2, 'config_change', NOW() - INTERVAL '2 days')",
                )
                .bind(hostname)
                .bind(&newer_path)
                .execute(pool)
                .await
                .expect("older system state should be inserted");
                sqlx::query(
                    "INSERT INTO system_states (hostname, store_path, change_reason, timestamp)
                     VALUES ($1, $2, 'config_change', NOW())",
                )
                .bind(hostname)
                .bind(&running_path)
                .execute(pool)
                .await
                .expect("latest system state should be inserted");
            }
        }

        FleetFixture {
            flake_id: flake.id,
            environment_id,
            derivation_ids: vec![running.id, newer.id],
            hosts,
            running_id: running.id,
            newer_id: newer.id,
        }
    }

    /// Helper to create a unique build-complete derivation for testing.
    async fn setup_test_derivation(pool: &PgPool) -> (i32, String) {
        let suffix = Uuid::new_v4().simple().to_string();
        let derivation_name = format!("test-host-{suffix}");
        let store_path = format!("/nix/store/{suffix}-test");
        let derivation = insert_derivation(pool, None, &derivation_name, "nixos")
            .await
            .expect("derivation should be created");

        // Update to build-complete with a store path so the scan queries pick it up
        sqlx::query(
            r#"
            UPDATE derivations
            SET status_id = $1,
                store_path = $3,
                completed_at = NOW(),
                derivation_path = $4
            WHERE id = $2
            "#,
        )
        .bind(EvaluationStatus::BuildComplete.as_id())
        .bind(derivation.id)
        .bind(&store_path)
        .bind(format!("{store_path}.drv"))
        .execute(pool)
        .await
        .expect("derivation status should be updated");
        // Insert a scan_schedule_policy row so the rescan query doesn't fail.
        sqlx::query(
            r#"
            INSERT INTO scan_schedule_policy (id, on_build, deployed_interval, recent_interval, archived_interval, archived_enabled)
            VALUES (1, true, '24h', '24h', '168h', true)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .execute(pool)
        .await
        .expect("scan schedule policy should be inserted");

        (derivation.id, derivation.derivation_name)
    }

    async fn insert_pending_test_scan(pool: &PgPool, derivation_id: i32) -> Uuid {
        let scan_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO cve_scans (
                id, derivation_id, scanner_name, status, attempts,
                total_packages, total_vulnerabilities,
                critical_count, high_count, medium_count, low_count
            )
            VALUES ($1, $2, 'vulnix', 'pending', 0, 0, 0, 0, 0, 0, 0)
            "#,
        )
        .bind(scan_id)
        .bind(derivation_id)
        .execute(pool)
        .await
        .expect("pending scan should be inserted");
        scan_id
    }

    /// An unscanned derivation with build-complete status should be selected
    /// for post-build scanning.
    #[sqlx::test]
    #[ignore = "requires test database creation privileges"]
    async fn get_targets_needing_cve_scan_selects_unscanned_derivation(pool: PgPool) {
        let (_, derivation_name) = setup_test_derivation(&pool).await;

        let targets = get_targets_needing_cve_scan(&pool, Some(10), &[], None)
            .await
            .expect("should fetch targets");

        assert!(
            !targets.is_empty(),
            "unscanned derivation should be selected"
        );
        assert!(
            targets.iter().any(|d| d.derivation_name == derivation_name),
            "the fixture derivation should be among targets"
        );
    }

    /// A derivation with an in_progress scan should NOT be selected.
    #[sqlx::test]
    #[ignore = "requires test database creation privileges"]
    async fn get_targets_needing_cve_scan_excludes_in_progress(pool: PgPool) {
        let (derivation_id, _) = setup_test_derivation(&pool).await;

        let _scan_id = create_cve_scan(&pool, derivation_id, "vulnix", None)
            .await
            .expect("scan should be created")
            .id();

        let targets = get_targets_needing_cve_scan(&pool, Some(10), &[], None)
            .await
            .expect("should fetch targets");

        assert!(
            !targets.iter().any(|d| d.id == derivation_id),
            "derivation with in_progress scan should be excluded"
        );
    }

    /// Fleet targets must track the generation each active system is actually
    /// running, taken from the latest `system_states` row — not merely the
    /// newest successful build for that system's configuration.
    #[tokio::test]
    async fn fleet_targets_track_running_generation_and_deduplicate() {
        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should be reachable");
        let fixture = setup_fleet_fixture(&pool).await;

        let targets = get_fleet_cve_scan_targets(&pool)
            .await
            .expect("fleet targets should resolve");
        let selected: Vec<i32> = targets.iter().map(|t| t.derivation_id).collect();

        assert!(
            selected.contains(&fixture.running_id),
            "the currently running generation must be selected"
        );
        assert!(
            !selected.contains(&fixture.newer_id),
            "a newer but unactivated build must not be selected"
        );
        assert_eq!(
            selected
                .iter()
                .filter(|id| **id == fixture.running_id)
                .count(),
            1,
            "systems sharing a running generation must collapse to one target"
        );

        fixture.cleanup(&pool).await;
    }

    /// The fleet enqueue must be atomic and must not double-claim derivations
    /// that already have an active scan.
    #[tokio::test]
    async fn fleet_enqueue_creates_pending_rows_and_skips_active_scans() {
        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should be reachable");
        let already_active = setup_fleet_fixture(&pool).await;
        let available = setup_fleet_fixture(&pool).await;

        create_cve_scan(&pool, already_active.running_id, "vulnix", None)
            .await
            .expect("one eligible target should already have an active scan");

        // An active scan unrelated to the fleet must not affect either outcome
        // count. This guards against the old test's broad assumption that the
        // entire cve_scans table was empty.
        let unrelated = insert_derivation(
            &pool,
            None,
            &format!("unrelated-{}", Uuid::new_v4().simple()),
            "nixos",
        )
        .await
        .expect("unrelated derivation should be inserted");
        create_cve_scan(&pool, unrelated.id, "vulnix", None)
            .await
            .expect("unrelated active scan should be inserted");

        let before = get_fleet_cve_scan_targets(&pool)
            .await
            .expect("fleet targets should resolve");
        assert_eq!(before.len(), 2, "the test owns exactly two fleet targets");

        let first = enqueue_fleet_cve_scans(&pool, "vulnix", None)
            .await
            .expect("fleet enqueue should succeed");
        assert_eq!(
            first.eligible,
            before.len() as i64,
            "enqueue must consider exactly the resolved fleet targets"
        );
        assert_eq!(
            first.created, 1,
            "only the eligible target without an active scan should be queued"
        );
        assert_eq!(first.reused(), 1);

        // Immediately re-running must be a no-op: every target now has a
        // pending scan, and the partial unique index prevents duplicates.
        let second = enqueue_fleet_cve_scans(&pool, "vulnix", None)
            .await
            .expect("second fleet enqueue should succeed");
        assert_eq!(second.eligible, first.eligible);
        assert_eq!(
            second.created, 0,
            "a second fleet request must not duplicate active scans"
        );
        assert_eq!(second.reused(), second.eligible);

        // Queued rows must be atomically claimable by the worker.
        let claimed = claim_queued_cve_scans(&pool, 1000)
            .await
            .expect("queued scans should be claimable");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].derivation_id, available.running_id);

        sqlx::query("DELETE FROM cve_scans WHERE derivation_id = $1")
            .bind(unrelated.id)
            .execute(&pool)
            .await
            .expect("unrelated scan should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(unrelated.id)
            .execute(&pool)
            .await
            .expect("unrelated derivation should be deleted");
        already_active.cleanup(&pool).await;
        available.cleanup(&pool).await;
    }

    /// Two server processes racing to drain the same queue must not both own
    /// the same row. The atomic claim returns it to exactly one caller.
    #[tokio::test]
    async fn concurrent_queue_claim_has_exactly_one_winner() {
        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let setup_pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should be reachable");
        let derivation = insert_derivation(
            &setup_pool,
            None,
            &format!("claim-race-{}", Uuid::new_v4().simple()),
            "nixos",
        )
        .await
        .expect("claim-race derivation should be inserted");
        let scan_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO cve_scans (
                id, derivation_id, scanner_name, status, attempts,
                total_packages, total_vulnerabilities,
                critical_count, high_count, medium_count, low_count,
                created_at
            )
            VALUES ($1, $2, 'vulnix', 'pending', 0, 0, 0, 0, 0, 0, 0,
                    NOW() - INTERVAL '1 day')
            "#,
        )
        .bind(scan_id)
        .bind(derivation.id)
        .execute(&setup_pool)
        .await
        .expect("pending scan should be inserted");

        let worker_a = PgPool::connect(&database_url)
            .await
            .expect("first worker pool should connect");
        let worker_b = PgPool::connect(&database_url)
            .await
            .expect("second worker pool should connect");
        let (claimed_a, claimed_b) = tokio::join!(
            claim_queued_cve_scans(&worker_a, 1),
            claim_queued_cve_scans(&worker_b, 1)
        );
        let claimed_a = claimed_a.expect("first worker claim should succeed");
        let claimed_b = claimed_b.expect("second worker claim should succeed");
        let winners = claimed_a
            .iter()
            .chain(&claimed_b)
            .filter(|claim| claim.scan_id == scan_id)
            .count();
        assert_eq!(winners, 1, "a pending row must have one execution owner");

        let winner = claimed_a
            .iter()
            .chain(&claimed_b)
            .find(|claim| claim.scan_id == scan_id)
            .copied()
            .expect("one worker should own the scan");
        let (status, attempts, execution_id, execution_started, heartbeat_started): (
            String,
            i32,
            Uuid,
            bool,
            bool,
        ) = sqlx::query_as(
            r#"
            SELECT status, attempts,
                   (scan_metadata ->> 'execution_id')::uuid AS execution_id,
                   scan_metadata ? 'execution_started_at' AS execution_started,
                   scan_metadata ? 'execution_heartbeat_at' AS heartbeat_started
            FROM cve_scans
            WHERE id = $1
            "#,
        )
        .bind(scan_id)
        .fetch_one(&setup_pool)
        .await
        .expect("claimed scan should resolve");
        assert_eq!(status, "in_progress");
        assert_eq!(attempts, 1, "the winning claim increments attempts once");
        assert_eq!(execution_id, winner.execution_id);
        assert!(execution_started, "the claim records execution start time");
        assert!(heartbeat_started, "the claim initializes its heartbeat");

        sqlx::query("DELETE FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .execute(&setup_pool)
            .await
            .expect("claim-race scan should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&setup_pool)
            .await
            .expect("claim-race derivation should be deleted");
    }

    /// Directly proves the PostgreSQL advisory-lock detection query against a
    /// real session: [`execution_lock_is_held`] must observe a lock acquired
    /// by [`acquire_execution_lock`] on a separate connection, and must stop
    /// observing it the moment [`release_execution_lock`] confirms release.
    ///
    /// This is a regression against comparing `objid` directly to the full
    /// 64-bit lock key: that comparison never matches a real PostgreSQL
    /// single-bigint advisory lock, so the assertion below would fail with
    /// that bug reintroduced.
    #[tokio::test]
    async fn execution_lock_detects_real_acquire_and_confirmed_release() {
        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let observer_pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should be reachable");
        let holder_pool = PgPool::connect(&database_url)
            .await
            .expect("lock-holder pool should connect");

        let execution_id = Uuid::new_v4();

        assert!(
            !execution_lock_is_held(&observer_pool, execution_id)
                .await
                .expect("lock status should be queryable before acquisition"),
            "an unacquired execution lock must not be reported as held"
        );

        let mut holder_conn = holder_pool
            .acquire()
            .await
            .expect("lock-holder connection should be acquired");
        acquire_execution_lock(&mut holder_conn, execution_id)
            .await
            .expect("advisory lock should be acquired");

        assert!(
            execution_lock_is_held(&observer_pool, execution_id)
                .await
                .expect("lock status should be queryable while held"),
            "a real PostgreSQL advisory lock acquired on another session must be detected"
        );

        assert!(
            release_execution_lock(&mut holder_conn, execution_id)
                .await
                .expect("unlock should execute"),
            "pg_advisory_unlock must confirm this session held and released the lock"
        );

        assert!(
            !execution_lock_is_held(&observer_pool, execution_id)
                .await
                .expect("lock status should be queryable after release"),
            "the lock must no longer be observed as held once explicitly released"
        );
    }

    /// Queue age is not execution age: a row may wait longer than the stale
    /// threshold and still be fresh immediately after a worker claims it.
    #[tokio::test]
    async fn stale_recovery_uses_execution_start_not_queue_time() {
        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should be reachable");
        let derivation = insert_derivation(
            &pool,
            None,
            &format!("stale-claim-{}", Uuid::new_v4().simple()),
            "nixos",
        )
        .await
        .expect("stale-claim derivation should be inserted");
        let scan_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO cve_scans (
                id, derivation_id, scanner_name, status, attempts,
                total_packages, total_vulnerabilities,
                critical_count, high_count, medium_count, low_count,
                created_at
            )
            VALUES ($1, $2, 'vulnix', 'pending', 0, 0, 0, 0, 0, 0, 0,
                    NOW() - INTERVAL '2 hours')
            "#,
        )
        .bind(scan_id)
        .bind(derivation.id)
        .execute(&pool)
        .await
        .expect("old pending scan should be inserted");

        let claimed = claim_queued_cve_scans(&pool, 1)
            .await
            .expect("old pending scan should be claimed");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].scan_id, scan_id);
        assert_eq!(claimed[0].derivation_id, derivation.id);

        let recovered = recover_stale_scans(&pool, std::time::Duration::from_secs(1800))
            .await
            .expect("freshly claimed scan recovery should succeed");
        assert_eq!(
            recovered, 0,
            "an old queue entry must not be stale immediately after execution starts"
        );

        sqlx::query(
            r#"
            UPDATE cve_scans
            SET scan_metadata = jsonb_set(
                    scan_metadata,
                    '{execution_started_at}',
                    to_jsonb((NOW() - INTERVAL '2 hours')::text)
                ) || jsonb_build_object(
                    'execution_heartbeat_at', NOW() - INTERVAL '2 hours'
                )
            WHERE id = $1
            "#,
        )
        .bind(scan_id)
        .execute(&pool)
        .await
        .expect("execution start should be aged");
        let recovered = recover_stale_scans(&pool, std::time::Duration::from_secs(1800))
            .await
            .expect("genuinely stale scan recovery should succeed");
        assert_eq!(recovered, 1, "a genuinely stale execution is revoked");
        let (status, revoked): (String, bool) = sqlx::query_as(
            "SELECT status, scan_metadata ? 'execution_revoked_at' FROM cve_scans WHERE id = $1",
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("revoked stale execution should resolve");
        assert_eq!(status, "in_progress");
        assert!(revoked, "revocation must retain active-scan uniqueness");
        assert!(matches!(
            create_cve_scan(&pool, derivation.id, "vulnix", Some("test".to_string()))
                .await
                .expect("replacement claim should observe the active revocation"),
            CreateCveScanOutcome::Existing(id) if id == scan_id
        ));

        sqlx::query(
            "UPDATE cve_scans SET scan_metadata = scan_metadata || jsonb_build_object('execution_revoked_at', NOW() - INTERVAL '2 minutes') WHERE id = $1",
        )
        .bind(scan_id)
        .execute(&pool)
        .await
        .expect("crashed-owner revocation should be aged beyond its grace period");
        assert_eq!(
            recover_stale_scans(&pool, std::time::Duration::from_secs(1800))
                .await
                .expect("expired revocation should be finalized"),
            1
        );
        let status: String = sqlx::query_scalar("SELECT status FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .fetch_one(&pool)
            .await
            .expect("finalized stale execution should resolve");
        assert_eq!(status, "failed");

        sqlx::query("DELETE FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .execute(&pool)
            .await
            .expect("stale-claim scan should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("stale-claim derivation should be deleted");
    }

    /// A live heartbeat keeps a long-running execution out of stale recovery.
    /// Once recovery wins, the former owner cannot mutate the terminal row or
    /// persist any package/CVE side effects with its obsolete token.
    #[tokio::test]
    async fn execution_heartbeat_and_owner_guards_survive_stale_recovery() {
        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should be reachable");
        let derivation = insert_derivation(
            &pool,
            None,
            &format!("ownership-recovery-{}", Uuid::new_v4().simple()),
            "nixos",
        )
        .await
        .expect("ownership-recovery derivation should be inserted");
        let scan_id = insert_pending_test_scan(&pool, derivation.id).await;
        let claim = claim_queued_cve_scans(&pool, 1)
            .await
            .expect("pending scan should be claimable")
            .into_iter()
            .find(|claim| claim.scan_id == scan_id)
            .expect("worker should own the test scan");
        let mut lock_holder = pool
            .acquire()
            .await
            .expect("live execution should acquire a dedicated lock connection");
        acquire_execution_lock(&mut lock_holder, claim.execution_id)
            .await
            .expect("live execution should acquire its advisory lock");
        assert!(
            execution_lock_is_held(&pool, claim.execution_id)
                .await
                .expect("live execution lock should be queryable"),
            "a token-owned live execution must hold the production advisory lock"
        );

        let wrong_execution_id = Uuid::new_v4();
        assert!(
            !heartbeat_cve_scan_execution(&pool, scan_id, wrong_execution_id)
                .await
                .expect("wrong-owner heartbeat should execute")
        );
        assert!(
            complete_cve_scan_for_owner(
                &pool,
                scan_id,
                0,
                0,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(wrong_execution_id),
            )
            .await
            .is_err(),
            "a non-owner must not complete an active execution"
        );
        assert!(
            mark_cve_scan_failed_for_owner(
                &pool,
                scan_id,
                &derivation,
                "wrong owner",
                Some(wrong_execution_id),
            )
            .await
            .is_err(),
            "a non-owner must not fail an active execution"
        );

        sqlx::query(
            r#"
            UPDATE cve_scans
            SET scan_metadata = scan_metadata || jsonb_build_object(
                'execution_started_at', NOW() - INTERVAL '2 hours',
                'execution_heartbeat_at', NOW() - INTERVAL '2 hours'
            )
            WHERE id = $1
            "#,
        )
        .bind(scan_id)
        .execute(&pool)
        .await
        .expect("execution timestamps should be aged");
        assert!(
            heartbeat_cve_scan_execution(&pool, scan_id, claim.execution_id)
                .await
                .expect("owner heartbeat should succeed")
        );
        assert_eq!(
            recover_stale_scans(&pool, std::time::Duration::from_secs(1800))
                .await
                .expect("heartbeat-aware recovery should succeed"),
            0,
            "a fresh heartbeat must protect a healthy long-running execution"
        );

        sqlx::query(
            r#"
            UPDATE cve_scans
            SET scan_metadata = scan_metadata || jsonb_build_object(
                'execution_started_at', NOW() - INTERVAL '2 hours',
                'execution_heartbeat_at', NOW() - INTERVAL '2 hours'
            )
            WHERE id = $1
            "#,
        )
        .bind(scan_id)
        .execute(&pool)
        .await
        .expect("heartbeat should be aged");
        assert_eq!(
            recover_stale_scans(&pool, std::time::Duration::from_secs(1800))
                .await
                .expect("live stale recovery should execute"),
            0,
            "recovery must not revoke a heartbeat-stale execution while its production lock is held"
        );
        let (status, revoked): (String, bool) = sqlx::query_as(
            "SELECT status, scan_metadata ? 'execution_revoked_at' FROM cve_scans WHERE id = $1",
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("live scan status should resolve");
        assert_eq!(status, "in_progress");
        assert!(
            !revoked,
            "recovery must leave a locked live execution unrevoked"
        );

        assert!(
            release_execution_lock(&mut lock_holder, claim.execution_id)
                .await
                .expect("live execution unlock should execute"),
            "the production unlock path must confirm release"
        );
        assert!(
            !execution_lock_is_held(&pool, claim.execution_id)
                .await
                .expect("released execution lock should be queryable"),
            "recovery may only treat the execution as crashed after its lock is released"
        );
        assert_eq!(
            recover_stale_scans(&pool, std::time::Duration::from_secs(1800))
                .await
                .expect("unlocked stale execution should be recovered"),
            1
        );

        let suffix = Uuid::new_v4().simple().to_string();
        let package_name = format!("lost-owner-package-{suffix}");
        let cve_id = format!("CVE-2099-{suffix}");
        let entries = vec![crate::vulnix::vulnix_parser::VulnixEntry {
            name: package_name.clone(),
            pname: package_name.clone(),
            version: "1.0.0".to_string(),
            affected_by: vec![cve_id.clone()],
            whitelisted: vec![],
            derivation: format!("/nix/store/{suffix}-{package_name}.drv"),
            cvssv3_basescore: std::collections::HashMap::from([(cve_id.clone(), 9.8)]),
        }];
        assert!(
            save_scan_results_for_owner(
                &pool,
                scan_id,
                &entries,
                Some(1),
                Some(&format!("/nix/store/{suffix}-{package_name}")),
                Some(claim.execution_id),
            )
            .await
            .is_err(),
            "a recovered owner must not persist results"
        );
        assert!(
            mark_cve_scan_failed_for_owner(
                &pool,
                scan_id,
                &derivation,
                "obsolete owner",
                Some(claim.execution_id),
            )
            .await
            .is_err(),
            "a recovered owner must not replace the recovery failure"
        );
        assert!(
            complete_cve_scan_for_owner(
                &pool,
                scan_id,
                1,
                1,
                1,
                0,
                0,
                0,
                Some(1),
                None,
                Some(claim.execution_id),
            )
            .await
            .is_err(),
            "a recovered owner must not complete the scan"
        );

        let package_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM derivations WHERE derivation_name = $1 AND derivation_type = 'package'",
        )
        .bind(&package_name)
        .fetch_one(&pool)
        .await
        .expect("package side effects should be countable");
        let cve_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cves WHERE id = $1")
            .bind(&cve_id)
            .fetch_one(&pool)
            .await
            .expect("CVE side effects should be countable");
        assert_eq!(package_count, 0, "lost-owner package writes must roll back");
        assert_eq!(cve_count, 0, "lost-owner CVE writes must roll back");

        sqlx::query("DELETE FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .execute(&pool)
            .await
            .expect("ownership-recovery scan should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("ownership-recovery derivation should be deleted");
    }

    /// Only the active owner may requeue a claim, and reclaiming the same row
    /// must issue a new token while preserving attempt history.
    #[tokio::test]
    async fn execution_requeue_is_owner_guarded_and_reclaim_rotates_token() {
        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should be reachable");
        let derivation = insert_derivation(
            &pool,
            None,
            &format!("ownership-requeue-{}", Uuid::new_v4().simple()),
            "nixos",
        )
        .await
        .expect("ownership-requeue derivation should be inserted");
        let scan_id = insert_pending_test_scan(&pool, derivation.id).await;
        let first = claim_queued_cve_scans(&pool, 1)
            .await
            .expect("first claim should succeed")
            .into_iter()
            .find(|claim| claim.scan_id == scan_id)
            .expect("first claim should own the test scan");

        assert!(
            !requeue_cve_scan_execution(&pool, scan_id, Uuid::new_v4(), "wrong owner")
                .await
                .expect("wrong-owner requeue should execute")
        );
        assert!(
            requeue_cve_scan_execution(&pool, scan_id, first.execution_id, "worker disabled")
                .await
                .expect("owner requeue should execute")
        );
        let (status, attempts, has_execution_id): (String, i32, bool) = sqlx::query_as(
            "SELECT status, attempts, scan_metadata ? 'execution_id' FROM cve_scans WHERE id = $1",
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("requeued scan should resolve");
        assert_eq!(status, "pending");
        assert_eq!(attempts, 1, "requeue preserves the first claim attempt");
        assert!(!has_execution_id, "requeue removes the obsolete token");

        let second = claim_queued_cve_scans(&pool, 1)
            .await
            .expect("second claim should succeed")
            .into_iter()
            .find(|claim| claim.scan_id == scan_id)
            .expect("second claim should own the test scan");
        assert_ne!(first.execution_id, second.execution_id);
        let attempts: i32 = sqlx::query_scalar("SELECT attempts FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .fetch_one(&pool)
            .await
            .expect("reclaimed scan attempts should resolve");
        assert_eq!(attempts, 2, "reclaim records a second execution attempt");

        save_scan_results_for_execution(&pool, scan_id, &vec![], Some(0), second.execution_id)
            .await
            .expect("new owner should complete the reclaimed scan");
        let status: String = sqlx::query_scalar("SELECT status FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .fetch_one(&pool)
            .await
            .expect("completed reclaimed scan should resolve");
        assert_eq!(status, "completed");

        sqlx::query("DELETE FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .execute(&pool)
            .await
            .expect("ownership-requeue scan should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("ownership-requeue derivation should be deleted");
    }

    /// recover_stale_scans should mark old in_progress scans as failed,
    /// making the derivation eligible again.
    #[sqlx::test]
    #[ignore = "requires test database creation privileges"]
    async fn recover_stale_scans_unblocks_derivation(pool: PgPool) {
        let (derivation_id, _) = setup_test_derivation(&pool).await;

        // Create a scan, then strip its lease metadata to model a legacy row
        // from before execution tokens existed.
        let scan_id = create_cve_scan(&pool, derivation_id, "vulnix", None)
            .await
            .expect("scan should be created")
            .id();

        // Artificially age the scan so it appears stale.
        sqlx::query(
            r#"
            UPDATE cve_scans
            SET created_at = NOW() - '2 hours'::INTERVAL,
                scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
                    - 'execution_id'
                    - 'execution_started_at'
                    - 'execution_heartbeat_at'
            WHERE id = $1
            "#,
        )
        .bind(scan_id)
        .execute(&pool)
        .await
        .expect("scan should be aged");

        // Derivation should be excluded while scan is in_progress.
        let targets_before = get_targets_needing_cve_scan(&pool, Some(10), &[], None)
            .await
            .expect("should fetch targets");
        assert!(
            !targets_before.iter().any(|d| d.id == derivation_id),
            "derivation should be excluded with in_progress scan"
        );

        // Recover stale scans (30-minute threshold → the 2-hour-old scan is stale).
        let recovered = recover_stale_scans(&pool, std::time::Duration::from_secs(1800))
            .await
            .expect("recovery should succeed");
        assert_eq!(recovered, 1, "one scan should be recovered");

        // Verify the scan is now marked as failed.
        let scan = get_scan_by_id(&pool, scan_id)
            .await
            .expect("should fetch scan")
            .expect("scan should exist");
        assert_eq!(
            scan.status,
            ScanStatus::Failed,
            "recovered scan should be failed"
        );

        // Derivation should now be eligible again.
        let targets_after = get_targets_needing_cve_scan(&pool, Some(10), &[], None)
            .await
            .expect("should fetch targets");
        assert!(
            targets_after.iter().any(|d| d.id == derivation_id),
            "derivation should be eligible again after recovery"
        );
    }

    /// A stale completed scan should be selected for rescan when the policy
    /// interval has elapsed.
    #[tokio::test]
    async fn get_targets_needing_cve_rescan_selects_stale_scan() {
        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should be reachable");
        let (derivation_id, _) = setup_test_derivation(&pool).await;

        // Create a completed scan that is old enough to be stale.
        let scan_id = create_cve_scan(&pool, derivation_id, "vulnix", None)
            .await
            .expect("scan should be created")
            .id();
        sqlx::query(
            r#"
            UPDATE cve_scans
            SET status = 'completed',
                completed_at = NOW() - '48 hours'::INTERVAL,
                created_at = NOW() - '48 hours'::INTERVAL
            WHERE id = $1
            "#,
        )
        .bind(scan_id)
        .execute(&pool)
        .await
        .expect("scan should be aged as completed");

        // Rescan query should pick it up (deployed_interval is 24h, scan is 48h old).
        let targets = get_targets_needing_cve_rescan(&pool, Some(10))
            .await
            .expect("should fetch rescan targets");
        assert!(
            targets.iter().any(|d| d.id == derivation_id),
            "stale completed scan should be selected for rescan"
        );

        sqlx::query("DELETE FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .execute(&pool)
            .await
            .expect("stale rescan fixture scan should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation_id)
            .execute(&pool)
            .await
            .expect("stale rescan fixture derivation should be deleted");
    }
}
