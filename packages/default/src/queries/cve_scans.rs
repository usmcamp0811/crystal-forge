use crate::derivations::utils::get_store_path_from_drv;
use crate::derivations::{Derivation, DerivationType};
use crate::models::cve_scans::{CveScan, ScanStatus};
use crate::vulnix::vulnix_parser::{VulnixParser, VulnixScanOutput};
use anyhow::Result;
use bigdecimal::BigDecimal;
use bigdecimal::FromPrimitive;
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

fn truncate_for_varchar(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

/// Get derivations that need CVE scanning
pub async fn get_targets_needing_cve_scan(
    pool: &PgPool,
    limit: Option<i64>,
) -> Result<Vec<Derivation>> {
    let limit = limit.unwrap_or(10);
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
            AND d.store_path IS NOT NULL
            AND NOT EXISTS (
                SELECT 1 FROM cve_scans cs
                WHERE cs.derivation_id = d.id
                AND cs.status = 'completed'
            )
            AND NOT EXISTS (
                SELECT 1 FROM cve_scans cs
                WHERE cs.derivation_id = d.id
                AND cs.status = 'failed'
                AND cs.attempts >= 5
            )
        ORDER BY d.completed_at ASC NULLS LAST
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(targets)
}

/// Create a new CVE scan record
pub async fn create_cve_scan(
    pool: &PgPool,
    derivation_id: i32,
    scanner_name: &str,
    scanner_version: Option<String>,
) -> Result<Uuid> {
    let scan_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO cve_scans (
            id, derivation_id, scanner_name, scanner_version,
            status, total_packages, total_vulnerabilities,
            critical_count, high_count, medium_count, low_count,
            attempts
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
        scan_id,
        derivation_id,
        scanner_name,
        scanner_version,
        "pending" as &str,
        0i32,
        0i32,
        0i32,
        0i32,
        0i32,
        0i32,
        0i32
    )
    .execute(pool)
    .await?;

    Ok(scan_id)
}

/// Update CVE scan to in-progress status
pub async fn mark_scan_in_progress(pool: &PgPool, scan_id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE cve_scans 
        SET status = $1, attempts = attempts + 1
        WHERE id = $2
        "#,
        "in_progress" as &str,
        scan_id
    )
    .execute(pool)
    .await?;

    Ok(())
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
    sqlx::query!(
        r#"
        UPDATE cve_scans 
        SET 
            status = $1,
            completed_at = NOW(),
            total_packages = $2,
            total_vulnerabilities = $3,
            critical_count = $4,
            high_count = $5,
            medium_count = $6,
            low_count = $7,
            scan_duration_ms = $8,
            scan_metadata = $9
        WHERE id = $10
        "#,
        "completed" as &str,
        total_packages,
        total_vulnerabilities,
        critical_count,
        high_count,
        medium_count,
        low_count,
        scan_duration_ms,
        scan_metadata,
        scan_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark CVE scan as failed
pub async fn mark_cve_scan_failed(
    pool: &PgPool,
    target: &Derivation,
    error_message: &str,
) -> Result<()> {
    // First try to find an existing pending/in-progress scan
    let existing_scan = sqlx::query!(
        r#"
        SELECT id FROM cve_scans 
        WHERE derivation_id = $1 
            AND status IN ('pending', 'in_progress')
        ORDER BY scheduled_at DESC
        LIMIT 1
        "#,
        target.id
    )
    .fetch_optional(pool)
    .await?;

    let scan_id = if let Some(existing) = existing_scan {
        existing.id
    } else {
        // Create a new scan record to mark as failed
        create_cve_scan(pool, target.id, "vulnix", None).await?
    };

    // Create metadata with error details
    let metadata = serde_json::json!({
        "error": error_message,
        "target_name": target.derivation_name,
        "derivation_path": target.derivation_path
    });

    sqlx::query!(
        r#"
        UPDATE cve_scans 
        SET 
            status = $1,
            completed_at = NOW(),
            attempts = attempts + 1,
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

/// Save complete scan results to database
pub async fn save_scan_results(
    pool: &PgPool,
    scan_id: Uuid,
    vulnix_results: &VulnixScanOutput,
    scan_duration_ms: Option<i32>,
) -> Result<()> {
    save_scan_results_with_store_path_override(pool, scan_id, vulnix_results, scan_duration_ms, None)
        .await
}

pub(crate) async fn save_scan_results_with_store_path_override(
    pool: &PgPool,
    scan_id: Uuid,
    vulnix_results: &VulnixScanOutput,
    scan_duration_ms: Option<i32>,
    store_path_override: Option<&str>,
) -> Result<()> {
    // Calculate statistics from vulnix results
    let stats = VulnixParser::calculate_stats(vulnix_results);

    // Start a transaction
    let mut tx = pool.begin().await?;

    // Update the scan record with completion data
    sqlx::query!(
        r#"
        UPDATE cve_scans
        SET
            status = $1,
            completed_at = NOW(),
            total_packages = $2,
            total_vulnerabilities = $3,
            critical_count = $4,
            high_count = $5,
            medium_count = $6,
            low_count = $7,
            scan_duration_ms = $8
        WHERE id = $9
        "#,
        "completed" as &str,
        stats.total_packages as i32,
        stats.total_vulnerabilities as i32,
        stats.critical_count as i32,
        stats.high_count as i32,
        stats.medium_count as i32,
        stats.low_count as i32,
        scan_duration_ms,
        scan_id
    )
    .execute(&mut *tx)
    .await?;

    // Insert packages and vulnerabilities found during scan
    for entry in vulnix_results {
        debug!(
            "CVE Scan Entry - name: '{}', pname: {:?}, version: {:?}, derivation: '{}', affected_by: {:?}",
            entry.name, entry.pname, entry.version, entry.derivation, entry.affected_by
        );

        let store_path = match store_path_override {
            Some(path) => path.to_string(),
            None => get_store_path_from_drv(&entry.derivation).await?,
        };
        let package_version = truncate_for_varchar(&entry.version, 100);
        if package_version != entry.version {
            debug!(
                "Truncated package version for DB insert (max=100): original='{}', truncated='{}'",
                entry.version, package_version
            );
        }
        // Insert package as a derivation with type 'package' and NULL commit_id
        let package_derivation_id = sqlx::query!(
            r#"
            INSERT INTO derivations (
                commit_id,        -- NULL for packages
                derivation_type, 
                derivation_name, 
                derivation_path, 
                derivation_target, -- NULL for packages discovered during scanning
                pname, 
                version, 
                status_id, 
                store_path,
                attempt_count
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 0)
            ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type) DO UPDATE SET
                pname = EXCLUDED.pname,
                version = EXCLUDED.version,
                status_id = EXCLUDED.status_id
            RETURNING id
            "#,
            None::<i32>, // commit_id is NULL for packages
            "package",
            entry.name,       // Use derivation path as name to ensure uniqueness
            entry.derivation, // This is the derivation path from vulnix
            None::<String>,   // derivation_target is NULL for packages discovered during scanning
            entry.pname,
            package_version,
            11i32, // Status ID for 'complete'
            store_path,
        )
        .fetch_one(&mut *tx)
        .await?
        .id;

        // Link package to scan using the new derivation_id
        // First check if it already exists
        let existing = sqlx::query!(
            r#"
            SELECT id FROM scan_packages 
            WHERE scan_id = $1 AND derivation_id = $2
            "#,
            scan_id,
            package_derivation_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        if existing.is_none() {
            sqlx::query!(
                r#"
                INSERT INTO scan_packages (scan_id, derivation_id, is_runtime_dependency, dependency_depth)
                VALUES ($1, $2, $3, $4)
                "#,
                scan_id,
                package_derivation_id,
                true,  // Assume runtime dependency for now
                0i32   // Assume direct dependency for now
            )
            .execute(&mut *tx)
            .await?;
        }

        // Insert CVEs from this entry
        for cve_id in &entry.affected_by {
            // Get CVSS score for this CVE from the entry
            let cvss_score = entry.cvssv3_basescore.get(cve_id).copied();

            // Insert or update CVE (minimal data from vulnix)
            sqlx::query!(
                r#"
                INSERT INTO cves (id, cvss_v3_score)
                VALUES ($1, $2)
                ON CONFLICT (id) DO UPDATE SET
                    cvss_v3_score = COALESCE(EXCLUDED.cvss_v3_score, cves.cvss_v3_score),
                    updated_at = NOW()
                "#,
                cve_id,
                cvss_score.and_then(BigDecimal::from_f32)
            )
            .execute(&mut *tx)
            .await?;

            // Insert package vulnerability relationship using derivation_id
            sqlx::query!(
                r#"
                INSERT INTO package_vulnerabilities (
                    derivation_id, cve_id, detection_method, is_whitelisted
                )
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (derivation_id, cve_id) DO UPDATE SET
                    detection_method = EXCLUDED.detection_method,
                    updated_at = NOW()
                "#,
                package_derivation_id,
                cve_id,
                "vulnix",
                false // Not whitelisted by default
            )
            .execute(&mut *tx)
            .await?;
        }

        // Handle whitelisted CVEs (still track them but mark as whitelisted)
        for cve_id in &entry.whitelisted {
            let cvss_score = entry.cvssv3_basescore.get(cve_id).copied();

            // Insert or update CVE
            sqlx::query!(
                r#"
                INSERT INTO cves (id, cvss_v3_score)
                VALUES ($1, $2)
                ON CONFLICT (id) DO UPDATE SET
                    cvss_v3_score = COALESCE(EXCLUDED.cvss_v3_score, cves.cvss_v3_score),
                    updated_at = NOW()
                "#,
                cve_id,
                cvss_score.and_then(BigDecimal::from_f32)
            )
            .execute(&mut *tx)
            .await?;

            // Insert whitelisted vulnerability relationship using derivation_id
            sqlx::query!(
                r#"
                INSERT INTO package_vulnerabilities (
                    derivation_id, cve_id, detection_method, is_whitelisted, whitelist_reason
                )
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (derivation_id, cve_id) DO UPDATE SET
                    detection_method = EXCLUDED.detection_method,
                    is_whitelisted = EXCLUDED.is_whitelisted,
                    whitelist_reason = EXCLUDED.whitelist_reason,
                    updated_at = NOW()
                "#,
                package_derivation_id,
                cve_id,
                "vulnix",
                true,
                "vulnix whitelist"
            )
            .execute(&mut *tx)
            .await?;
        }
    }

    // Commit the transaction
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
pub async fn count_unjustified_high_cves(
    pool: &PgPool,
    derivation_id: i32,
) -> Result<Option<i64>> {
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
