use crate::models::cve_scans::{CveScan, ScanStatus};
use crate::models::derivations::{Derivation, DerivationType};
use crate::vulnix::vulnix_parser::{VulnixParser, VulnixScanOutput};
use anyhow::Result;
use bigdecimal::BigDecimal;
use bigdecimal::FromPrimitive;
use sqlx::PgPool;
use uuid::Uuid;

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
                attempt_count
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0)
            ON CONFLICT (derivation_path) DO UPDATE SET
                derivation_name = EXCLUDED.derivation_name,
                pname = EXCLUDED.pname,
                version = EXCLUDED.version,
                status_id = EXCLUDED.status_id
            RETURNING id
            "#,
            None::<i32>, // commit_id is NULL for packages
            "package",
            entry.derivation, // Use derivation path as name to ensure uniqueness
            entry.derivation, // This is the derivation path from vulnix
            None::<String>,   // derivation_target is NULL for packages discovered during scanning
            entry.pname,
            entry.version,
            11i32, // Status ID for 'complete' from your EvaluationStatus enum
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
