//! Database queries for hardening scans.

use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

use crate::hardening::scanner::ScanResult;
use crate::hardening::types::{
    FleetHardeningSummary, HardeningJustification, HardeningScan, RiskLevel,
    ServiceHardeningResult, SystemHardeningPosture, TopVulnerableService,
};
use crate::models::hardening_scans::ScanStatus;

/// Idempotently enqueue a hardening scan for a derivation.
///
/// The `ON CONFLICT ... DO NOTHING` clause relies on the partial unique index
/// added by migration 0188:
///
///   UNIQUE (derivation_id) WHERE status IN ('pending', 'in_progress')
///
/// This means concurrent callers can never create two active rows for the same
/// derivation.  If a row already exists, the INSERT is silently skipped and
/// the function returns the existing scan ID instead.
///
/// IMPORTANT: This function only writes a database row.  It does NOT spawn a
/// task or start a subprocess.  The actual `nix eval` is performed later by
/// `run_hardening_scan_queue` in `services/hardening_scans.rs`.
pub async fn create_hardening_scan(pool: &PgPool, derivation_id: i32) -> Result<Uuid> {
    let scan_id = Uuid::new_v4();

    let inserted = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO hardening_scans (
            id, derivation_id, status, attempts,
            total_services, well_hardened_count, moderately_hardened_count,
            poorly_hardened_count, vulnerable_count
        ) VALUES ($1, $2, 'pending', 0, 0, 0, 0, 0, 0)
        ON CONFLICT (derivation_id)
          WHERE status IN ('pending', 'in_progress')
        DO NOTHING
        RETURNING id
        "#,
    )
    .bind(scan_id)
    .bind(derivation_id)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = inserted {
        return Ok(id);
    }

    get_active_scan_for_derivation(pool, derivation_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("active hardening scan disappeared during enqueue"))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClaimedHardeningScan {
    pub id: Uuid,
    pub derivation_id: i32,
    pub config_name: String,
    pub repo_url: String,
    pub commit_hash: String,
    pub attempts: i32,
}

/// PostgreSQL advisory lock key used to serialize hardening scan claims and
/// stale recovery across multiple server processes.  Held only for the duration
/// of the claim transaction (milliseconds), NOT for the duration of the scan.
///
/// This is distinct from `HEAVY_NIX_ADVISORY_LOCK` in `evaluate_with_policies.rs`,
/// which is held for the entire `nix eval` subprocess lifetime to prevent bulk
/// evaluation and hardening from overlapping.  Using separate keys lets the
/// claim step itself remain lightweight.
const HARDENING_CLAIM_ADVISORY_LOCK: i64 = 0x4346_4841_5244;

/// Atomically claim the oldest pending hardening scan.
///
/// Uses a transaction-scoped PostgreSQL advisory lock to serialize concurrent
/// claim attempts across multiple server or worker processes.  `SKIP LOCKED`
/// makes the inner SELECT non-blocking: if another transaction already holds a
/// row lock (e.g. during stale recovery), this call simply returns `None` and
/// the caller will retry on the next poll tick.
///
/// The global `in_progress` partial unique index (migration 0188) provides an
/// additional database-level guard: even if two processes race past the advisory
/// lock, only one UPDATE can succeed.
pub async fn claim_next_hardening_scan(pool: &PgPool) -> Result<Option<ClaimedHardeningScan>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(HARDENING_CLAIM_ADVISORY_LOCK)
        .execute(&mut *tx)
        .await?;

    let claimed = sqlx::query_as::<_, ClaimedHardeningScan>(
        r#"
        WITH candidate AS (
            SELECT hs.id
            FROM hardening_scans hs
            WHERE hs.status = 'pending'
              AND NOT EXISTS (
                  SELECT 1 FROM hardening_scans active
                  WHERE active.status = 'in_progress'
              )
            ORDER BY hs.scheduled_at ASC, hs.id ASC
            LIMIT 1
            FOR UPDATE OF hs SKIP LOCKED
        ), claimed AS (
            UPDATE hardening_scans hs
            SET status = 'in_progress',
                started_at = NOW(),
                completed_at = NULL,
                attempts = hs.attempts + 1
            FROM candidate
            WHERE hs.id = candidate.id
            RETURNING hs.id, hs.derivation_id, hs.attempts
        )
        SELECT claimed.id,
               claimed.derivation_id,
               d.derivation_name AS config_name,
               f.repo_url,
               c.git_commit_hash AS commit_hash,
               claimed.attempts
        FROM claimed
        JOIN derivations d ON d.id = claimed.derivation_id
        JOIN commits c ON c.id = d.commit_id
        JOIN flakes f ON f.id = c.flake_id
        "#,
    )
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(claimed)
}

/// Recover work abandoned by a crashed worker. A five-minute subprocess
/// deadline plus a two-minute cleanup margin defines staleness.
pub async fn recover_stale_hardening_scans(pool: &PgPool) -> Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE hardening_scans
        SET status = CASE WHEN attempts < 3 THEN 'pending' ELSE 'failed' END,
            started_at = NULL,
            completed_at = CASE WHEN attempts < 3 THEN NULL ELSE NOW() END,
            scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
              || jsonb_build_object(
                  'recovered_at', NOW(),
                  'recovery_reason', 'stale hardening worker claim'
              )
        WHERE status = 'in_progress'
          AND started_at < NOW() - INTERVAL '7 minutes'
        "#,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn hardening_queue_depth(pool: &PgPool) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM hardening_scans WHERE status = 'pending'",
    )
    .fetch_one(pool)
    .await?)
}

/// Mark a scan as in progress.
pub async fn mark_scan_in_progress(pool: &PgPool, scan_id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE hardening_scans
        SET status = $1, started_at = NOW(), attempts = attempts + 1
        WHERE id = $2
        "#,
        "in_progress" as &str,
        scan_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Complete a hardening scan with results.
pub async fn complete_hardening_scan(
    pool: &PgPool,
    scan_id: Uuid,
    total_services: i32,
    well_hardened_count: i32,
    moderately_hardened_count: i32,
    poorly_hardened_count: i32,
    vulnerable_count: i32,
    overall_score: Option<i32>,
    scan_duration_ms: Option<i32>,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE hardening_scans
        SET
            status = $1,
            completed_at = NOW(),
            total_services = $2,
            well_hardened_count = $3,
            moderately_hardened_count = $4,
            poorly_hardened_count = $5,
            vulnerable_count = $6,
            overall_score = $7,
            scan_duration_ms = $8
        WHERE id = $9
        "#,
        "completed" as &str,
        total_services,
        well_hardened_count,
        moderately_hardened_count,
        poorly_hardened_count,
        vulnerable_count,
        overall_score,
        scan_duration_ms,
        scan_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark a scan as failed.
pub async fn mark_scan_failed(pool: &PgPool, scan_id: Uuid, error_message: &str) -> Result<()> {
    let metadata = serde_json::json!({ "error": error_message });

    sqlx::query!(
        r#"
        UPDATE hardening_scans
        SET
            status = $1,
            completed_at = NOW(),
            scan_metadata = $2
        WHERE id = $3
        "#,
        "failed" as &str,
        metadata,
        scan_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Persist all service rows and the completed scan summary atomically using a
/// single database connection and one batched UNNEST insert.
pub async fn persist_completed_hardening_scan(
    pool: &PgPool,
    scan_id: Uuid,
    scan: &ScanResult,
    scan_duration_ms: i32,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM service_hardening_results WHERE scan_id = $1")
        .bind(scan_id)
        .execute(&mut *tx)
        .await?;

    let mut names = Vec::with_capacity(scan.services.len());
    let mut service_types = Vec::with_capacity(scan.services.len());
    let mut scores = Vec::with_capacity(scan.services.len());
    let mut risk_levels = Vec::with_capacity(scan.services.len());
    let mut directives = Vec::with_capacity(scan.services.len());
    let mut enabled = Vec::with_capacity(scan.services.len());
    let mut disabled = Vec::with_capacity(scan.services.len());
    let mut missing = Vec::with_capacity(scan.services.len());

    for service in &scan.services {
        names.push(service.name.clone());
        service_types.push(service.service_type.clone());
        scores.push(service.score_result.score);
        risk_levels.push(match service.score_result.risk_level {
            RiskLevel::WellHardened => "well_hardened".to_string(),
            RiskLevel::ModeratelyHardened => "moderately_hardened".to_string(),
            RiskLevel::PoorlyHardened => "poorly_hardened".to_string(),
            RiskLevel::Vulnerable => "vulnerable".to_string(),
        });
        directives.push(serde_json::to_value(&service.score_result.directives)?);
        enabled.push(service.score_result.enabled_count);
        disabled.push(service.score_result.disabled_count);
        missing.push(service.score_result.missing_count);
    }

    if !names.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO service_hardening_results (
                id, scan_id, service_name, service_type, hardening_score,
                risk_level, directives_detail, enabled_directives_count,
                disabled_directives_count, missing_directives_count
            )
            SELECT gen_random_uuid(), $1, rows.*
            FROM UNNEST(
                $2::text[], $3::text[], $4::integer[], $5::text[],
                $6::jsonb[], $7::integer[], $8::integer[], $9::integer[]
            ) AS rows(
                service_name, service_type, hardening_score, risk_level,
                directives_detail, enabled_directives_count,
                disabled_directives_count, missing_directives_count
            )
            "#,
        )
        .bind(scan_id)
        .bind(&names)
        .bind(&service_types)
        .bind(&scores)
        .bind(&risk_levels)
        .bind(&directives)
        .bind(&enabled)
        .bind(&disabled)
        .bind(&missing)
        .execute(&mut *tx)
        .await?;
    }

    let updated = sqlx::query(
        r#"
        UPDATE hardening_scans
        SET status = 'completed',
            completed_at = NOW(),
            total_services = $2,
            well_hardened_count = $3,
            moderately_hardened_count = $4,
            poorly_hardened_count = $5,
            vulnerable_count = $6,
            overall_score = $7,
            scan_duration_ms = $8
        WHERE id = $1 AND status = 'in_progress'
        "#,
    )
    .bind(scan_id)
    .bind(scan.total_services)
    .bind(scan.well_hardened_count)
    .bind(scan.moderately_hardened_count)
    .bind(scan.poorly_hardened_count)
    .bind(scan.vulnerable_count)
    .bind(scan.overall_score)
    .bind(scan_duration_ms)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() != 1 {
        anyhow::bail!("hardening scan {scan_id} lost its in-progress claim before completion");
    }

    tx.commit().await?;
    Ok(())
}

/// Insert a service hardening result.
pub async fn insert_service_result(
    pool: &PgPool,
    scan_id: Uuid,
    service_name: &str,
    service_type: Option<&str>,
    hardening_score: i32,
    risk_level: RiskLevel,
    directives_detail: serde_json::Value,
    enabled_directives_count: i32,
    disabled_directives_count: i32,
    missing_directives_count: i32,
) -> Result<Uuid> {
    let result_id = Uuid::new_v4();
    let risk_level_str = match risk_level {
        RiskLevel::WellHardened => "well_hardened",
        RiskLevel::ModeratelyHardened => "moderately_hardened",
        RiskLevel::PoorlyHardened => "poorly_hardened",
        RiskLevel::Vulnerable => "vulnerable",
    };

    sqlx::query!(
        r#"
        INSERT INTO service_hardening_results (
            id, scan_id, service_name, service_type,
            hardening_score, risk_level, directives_detail,
            enabled_directives_count, disabled_directives_count, missing_directives_count
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#,
        result_id,
        scan_id,
        service_name,
        service_type,
        hardening_score,
        risk_level_str,
        directives_detail,
        enabled_directives_count,
        disabled_directives_count,
        missing_directives_count
    )
    .execute(pool)
    .await?;

    Ok(result_id)
}

/// Get a hardening scan by ID.
pub async fn get_scan_by_id(pool: &PgPool, scan_id: Uuid) -> Result<Option<HardeningScan>> {
    let scan = sqlx::query_as!(
        HardeningScan,
        r#"
        SELECT
            id,
            derivation_id as "derivation_id!",
            scheduled_at,
            started_at,
            completed_at,
            status as "status!: ScanStatus",
            attempts as "attempts!",
            total_services as "total_services!",
            well_hardened_count as "well_hardened_count!",
            moderately_hardened_count as "moderately_hardened_count!",
            poorly_hardened_count as "poorly_hardened_count!",
            vulnerable_count as "vulnerable_count!",
            overall_score,
            scan_duration_ms,
            scan_metadata,
            created_at as "created_at!"
        FROM hardening_scans
        WHERE id = $1
        "#,
        scan_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(scan)
}

/// List environment IDs for active systems associated with a hardening scan's derivation.
pub async fn list_scan_environment_ids(pool: &PgPool, scan_id: Uuid) -> Result<Vec<Option<Uuid>>> {
    let rows = sqlx::query_scalar::<_, Option<Uuid>>(
        r#"
        SELECT DISTINCT s.environment_id
        FROM hardening_scans hs
        JOIN derivations d ON d.id = hs.derivation_id
        JOIN commits c ON c.id = d.commit_id
        JOIN systems s
          ON s.flake_id = c.flake_id
         AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) = d.derivation_name
        WHERE hs.id = $1
          AND s.is_active = TRUE
        "#,
    )
    .bind(scan_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Get the latest hardening scan for a derivation.
pub async fn get_latest_scan(pool: &PgPool, derivation_id: i32) -> Result<Option<HardeningScan>> {
    let scan = sqlx::query_as!(
        HardeningScan,
        r#"
        SELECT
            id,
            derivation_id as "derivation_id!",
            scheduled_at,
            started_at,
            completed_at,
            status as "status!: ScanStatus",
            attempts as "attempts!",
            total_services as "total_services!",
            well_hardened_count as "well_hardened_count!",
            moderately_hardened_count as "moderately_hardened_count!",
            poorly_hardened_count as "poorly_hardened_count!",
            vulnerable_count as "vulnerable_count!",
            overall_score,
            scan_duration_ms,
            scan_metadata,
            created_at as "created_at!"
        FROM hardening_scans
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

/// Get service results for a scan.
pub async fn get_service_results(
    pool: &PgPool,
    scan_id: Uuid,
) -> Result<Vec<ServiceHardeningResult>> {
    let results = sqlx::query_as!(
        ServiceHardeningResult,
        r#"
        SELECT
            id,
            scan_id,
            service_name as "service_name!",
            service_type,
            hardening_score as "hardening_score!",
            risk_level as "risk_level!: RiskLevel",
            directives_detail as "directives_detail!",
            enabled_directives_count as "enabled_directives_count!",
            disabled_directives_count as "disabled_directives_count!",
            missing_directives_count as "missing_directives_count!",
            created_at as "created_at!"
        FROM service_hardening_results
        WHERE scan_id = $1
        ORDER BY hardening_score ASC, service_name ASC
        "#,
        scan_id
    )
    .fetch_all(pool)
    .await?;

    Ok(results)
}

/// Get fleet-wide hardening summary.
pub async fn get_fleet_summary(pool: &PgPool) -> Result<FleetHardeningSummary> {
    let row = sqlx::query!(
        r#"
        SELECT
            COALESCE(total_systems_scanned, 0) as "total_systems_scanned!",
            avg_fleet_score,
            COALESCE(total_well_hardened_services, 0) as "total_well_hardened_services!",
            COALESCE(total_moderately_hardened_services, 0) as "total_moderately_hardened_services!",
            COALESCE(total_poorly_hardened_services, 0) as "total_poorly_hardened_services!",
            COALESCE(total_vulnerable_services, 0) as "total_vulnerable_services!",
            COALESCE(total_services_scanned, 0) as "total_services_scanned!",
            last_scan_completed
        FROM view_hardening_fleet_summary
        "#
    )
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(r) => FleetHardeningSummary {
            total_systems_scanned: r.total_systems_scanned,
            avg_fleet_score: r
                .avg_fleet_score
                .map(|d| d.to_string().parse().unwrap_or(0.0)),
            total_well_hardened_services: r.total_well_hardened_services,
            total_moderately_hardened_services: r.total_moderately_hardened_services,
            total_poorly_hardened_services: r.total_poorly_hardened_services,
            total_vulnerable_services: r.total_vulnerable_services,
            total_services_scanned: r.total_services_scanned,
            last_scan_completed: r.last_scan_completed,
        },
        None => FleetHardeningSummary {
            total_systems_scanned: 0,
            avg_fleet_score: None,
            total_well_hardened_services: 0,
            total_moderately_hardened_services: 0,
            total_poorly_hardened_services: 0,
            total_vulnerable_services: 0,
            total_services_scanned: 0,
            last_scan_completed: None,
        },
    })
}

/// Get top vulnerable services across fleet.
pub async fn get_top_vulnerable_services(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<TopVulnerableService>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            service_name as "service_name!",
            affected_systems_count as "affected_systems_count!",
            avg_score as "avg_score!",
            min_score as "min_score!",
            max_score as "max_score!"
        FROM view_hardening_top_vulnerable_services
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| TopVulnerableService {
            service_name: r.service_name,
            affected_systems_count: r.affected_systems_count,
            avg_score: r.avg_score.to_string().parse().unwrap_or(0.0),
            min_score: r.min_score,
            max_score: r.max_score,
        })
        .collect())
}

/// List system hardening posture rows for all systems with completed scans.
pub async fn list_system_postures(pool: &PgPool) -> Result<Vec<SystemHardeningPosture>> {
    let rows = sqlx::query_as::<_, SystemHardeningPosture>(
        r#"
        SELECT
            v.derivation_id,
            v.config_name,
            v.system_id,
            v.hostname,
            e.name AS environment_name,
            v.latest_scan_id,
            v.overall_score,
            v.risk_level,
            v.total_services,
            v.well_hardened_count,
            v.moderately_hardened_count,
            v.poorly_hardened_count,
            v.vulnerable_count,
            v.last_scan_at,
            v.scan_duration_ms
        FROM view_system_hardening_posture v
        LEFT JOIN systems s ON s.id = v.system_id
        LEFT JOIN environments e ON e.id = s.environment_id
        WHERE v.latest_scan_id IS NOT NULL
        ORDER BY v.overall_score ASC NULLS LAST, v.config_name ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Get the latest hardening posture row for a single system.
pub async fn get_system_posture(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<SystemHardeningPosture>> {
    let row = sqlx::query_as::<_, SystemHardeningPosture>(
        r#"
        SELECT
            v.derivation_id,
            v.config_name,
            v.system_id,
            v.hostname,
            e.name AS environment_name,
            v.latest_scan_id,
            v.overall_score,
            v.risk_level,
            v.total_services,
            v.well_hardened_count,
            v.moderately_hardened_count,
            v.poorly_hardened_count,
            v.vulnerable_count,
            v.last_scan_at,
            v.scan_duration_ms
        FROM view_system_hardening_posture v
        LEFT JOIN systems s ON s.id = v.system_id
        LEFT JOIN environments e ON e.id = s.environment_id
        WHERE v.system_id = $1
        ORDER BY v.last_scan_at DESC NULLS LAST
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Check if there's an active (pending/in_progress) scan for a derivation.
pub async fn get_active_scan_for_derivation(
    pool: &PgPool,
    derivation_id: i32,
) -> Result<Option<Uuid>> {
    let row = sqlx::query!(
        r#"
        SELECT id
        FROM hardening_scans
        WHERE derivation_id = $1
          AND status IN ('pending', 'in_progress')
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        derivation_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| r.id))
}

/// Create or update a hardening justification.
pub async fn upsert_justification(
    pool: &PgPool,
    system_id: Uuid,
    service_name: &str,
    directive_name: Option<&str>,
    category: Option<&str>,
    reason: &str,
    user_id: Option<Uuid>,
) -> Result<Uuid> {
    let inserted_id = Uuid::new_v4();

    if let Some(directive_name) = directive_name {
        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO hardening_justifications (
                id, system_id, service_name, directive_name,
                category, reason, created_by, updated_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
            ON CONFLICT (system_id, service_name, directive_name) WHERE directive_name IS NOT NULL DO UPDATE SET
                category = EXCLUDED.category,
                reason = EXCLUDED.reason,
                updated_by = EXCLUDED.updated_by,
                updated_at = NOW()
            RETURNING id
            "#,
        )
        .bind(inserted_id)
        .bind(system_id)
        .bind(service_name)
        .bind(directive_name)
        .bind(category)
        .bind(reason)
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        Ok(id)
    } else {
        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO hardening_justifications (
                id, system_id, service_name, directive_name,
                category, reason, created_by, updated_by
            ) VALUES ($1, $2, $3, NULL, $4, $5, $6, $6)
            ON CONFLICT (system_id, service_name) WHERE directive_name IS NULL DO UPDATE SET
                category = EXCLUDED.category,
                reason = EXCLUDED.reason,
                updated_by = EXCLUDED.updated_by,
                updated_at = NOW()
            RETURNING id
            "#,
        )
        .bind(inserted_id)
        .bind(system_id)
        .bind(service_name)
        .bind(category)
        .bind(reason)
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        Ok(id)
    }
}

/// Get justifications for a system.
pub async fn get_justifications_for_system(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Vec<HardeningJustification>> {
    let justifications = sqlx::query_as!(
        HardeningJustification,
        r#"
        SELECT
            id,
            system_id,
            service_name as "service_name!",
            directive_name,
            category,
            reason as "reason!",
            created_by,
            updated_by,
            created_at as "created_at!",
            updated_at as "updated_at!",
            expires_at
        FROM hardening_justifications
        WHERE system_id = $1
        ORDER BY service_name, directive_name NULLS FIRST
        "#,
        system_id
    )
    .fetch_all(pool)
    .await?;

    Ok(justifications)
}

/// Delete a justification.
pub async fn delete_justification(pool: &PgPool, justification_id: Uuid) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        DELETE FROM hardening_justifications
        WHERE id = $1
        "#,
        justification_id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CommitHardeningTarget {
    pub derivation_id: i32,
    pub config_name: String,
}

/// List NixOS derivations for a commit that should receive hardening scans.
pub async fn list_commit_hardening_targets(
    pool: &PgPool,
    commit_id: i32,
) -> Result<Vec<CommitHardeningTarget>> {
    let rows = sqlx::query_as::<_, CommitHardeningTarget>(
        r#"
        SELECT id AS derivation_id, derivation_name AS config_name
        FROM derivations
        WHERE commit_id = $1
          AND derivation_type = 'nixos'
        ORDER BY derivation_name ASC
        "#,
    )
    .bind(commit_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Resolve a system's derivation for hardening scan (similar to CVE scan pattern).
pub async fn resolve_system_hardening_scan_target(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<HardeningScanTarget>> {
    let row = sqlx::query_as::<_, HardeningScanTarget>(
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
        ),
        latest_state AS (
            SELECT ss.store_path
            FROM selected_system s
            JOIN system_states ss ON ss.hostname = s.hostname
            ORDER BY ss.timestamp DESC
            LIMIT 1
        )
        SELECT
            d.id AS derivation_id,
            ss.config_name as config_name,
            ss.hostname as hostname,
            COALESCE(f.repo_url, '') as repo_url,
            COALESCE(c.git_commit_hash, '') as commit_hash,
            CASE
                WHEN f.repo_url IS NULL OR BTRIM(f.repo_url) = ''
                  OR c.git_commit_hash IS NULL OR BTRIM(c.git_commit_hash) = ''
                THEN 'Flake source metadata is unavailable for this system configuration.'
                ELSE NULL
            END AS blocked_reason
        FROM selected_system ss
        JOIN latest_state ls ON TRUE
        JOIN derivations d
          ON COALESCE(d.store_path, d.expected_store_path) = ls.store_path
        JOIN commits c ON c.id = d.commit_id AND c.flake_id = ss.flake_id
        JOIN flakes f ON f.id = c.flake_id
        WHERE d.derivation_type = 'nixos'
          AND d.derivation_name = ss.config_name
        ORDER BY d.completed_at DESC NULLS LAST, d.id DESC
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Target for a hardening scan.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HardeningScanTarget {
    pub derivation_id: i32,
    pub config_name: String,
    pub hostname: String,
    pub repo_url: String,
    pub commit_hash: String,
    pub blocked_reason: Option<String>,
}
