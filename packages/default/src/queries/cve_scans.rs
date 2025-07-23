use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::cve_scans::{CveScan, ScanStatus};
use crate::models::evaluation_targets::EvaluationTarget;

/// Get evaluation targets that need CVE scanning
pub async fn get_targets_needing_cve_scan(
    pool: &PgPool,
    limit: Option<i64>,
) -> Result<Vec<EvaluationTarget>> {
    let limit = limit.unwrap_or(10);

    let targets = sqlx::query_as!(
        EvaluationTarget,
        r#"
        SELECT 
            et.id,
            et.commit_id,
            et.target_type,
            et.target_name,
            et.derivation_path,
            et.scheduled_at,
            et.completed_at,
            et.status
        FROM evaluation_targets et
        LEFT JOIN cve_scans cs ON et.id = cs.evaluation_target_id 
            AND cs.completed_at IS NOT NULL
        WHERE et.status = 'complete'
            AND et.derivation_path IS NOT NULL
            AND (cs.id IS NULL OR cs.completed_at < NOW() - INTERVAL '24 hours')
        ORDER BY et.completed_at ASC
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
    evaluation_target_id: i64,
    scanner_name: &str,
    scanner_version: Option<String>,
) -> Result<Uuid> {
    let scan_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO cve_scans (
            id, evaluation_target_id, scanner_name, scanner_version,
            status, total_packages, total_vulnerabilities,
            critical_count, high_count, medium_count, low_count,
            attempts
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
        scan_id,
        evaluation_target_id as i32,
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
    target: &EvaluationTarget,
    error_message: &str,
) -> Result<()> {
    // First try to find an existing pending/in-progress scan
    let existing_scan = sqlx::query!(
        r#"
        SELECT id FROM cve_scans 
        WHERE evaluation_target_id = $1 
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
        create_cve_scan(pool, target.id.into(), "vulnix", None).await?
    };

    // Create metadata with error details
    let metadata = serde_json::json!({
        "error": error_message,
        "target_name": target.target_name,
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
// pub async fn save_scan_results(
//     pool: &PgPool,
//     scan_result: &crate::vulnix::database_scan_result::DatabaseScanResult,
// ) -> Result<()> {
//     // Start a transaction
//     let mut tx = pool.begin().await?;
//
//     // Update the scan record
//     sqlx::query!(
//         r#"
//         UPDATE cve_scans
//         SET
//             status = $1,
//             completed_at = NOW(),
//             total_packages = $2,
//             total_vulnerabilities = $3,
//             critical_count = $4,
//             high_count = $5,
//             medium_count = $6,
//             low_count = $7,
//             scan_duration_ms = $8,
//             scan_metadata = $9
//         WHERE id = $10
//         "#,
//         "completed" as &str,
//         scan_result.total_packages,
//         scan_result.total_vulnerabilities,
//         scan_result.critical_count,
//         scan_result.high_count,
//         scan_result.medium_count,
//         scan_result.low_count,
//         scan_result.scan_duration_ms,
//         scan_result.metadata,
//         scan_result.scan_id
//     )
//     .execute(&mut *tx)
//     .await?;
//
//     // Insert packages found during scan
//     for package in &scan_result.packages {
//         // Insert or update package
//         sqlx::query!(
//             r#"
//             INSERT INTO nix_packages (derivation_path, name, pname, version)
//             VALUES ($1, $2, $3, $4)
//             ON CONFLICT (derivation_path) DO UPDATE SET
//                 name = EXCLUDED.name,
//                 pname = EXCLUDED.pname,
//                 version = EXCLUDED.version
//             "#,
//             package.derivation_path,
//             package.name,
//             package.pname,
//             package.version
//         )
//         .execute(&mut *tx)
//         .await?;
//
//         // Link package to scan
//         sqlx::query!(
//             r#"
//             INSERT INTO scan_packages (scan_id, derivation_path, is_runtime_dependency, dependency_depth)
//             VALUES ($1, $2, $3, $4)
//             ON CONFLICT (scan_id, derivation_path) DO NOTHING
//             "#,
//             scan_result.scan_id,
//             package.derivation_path,
//             true, // Assume runtime dependency for now
//             0i32  // Assume direct dependency for now
//         )
//         .execute(&mut *tx)
//         .await?;
//     }
//
//     // Insert CVEs found during scan
//     for cve in &scan_result.cves {
//         // Insert or update CVE
//         sqlx::query!(
//             r#"
//             INSERT INTO cves (id, cvss_v3_score, description, published_date, modified_date)
//             VALUES ($1, $2, $3, $4, $5)
//             ON CONFLICT (id) DO UPDATE SET
//                 cvss_v3_score = EXCLUDED.cvss_v3_score,
//                 description = EXCLUDED.description,
//                 published_date = EXCLUDED.published_date,
//                 modified_date = EXCLUDED.modified_date,
//                 updated_at = NOW()
//             "#,
//             cve.id,
//             cve.cvss_v3_score,
//             cve.description,
//             cve.published_date,
//             cve.modified_date
//         )
//         .execute(&mut *tx)
//         .await?;
//     }
//
//     // Insert package vulnerabilities
//     for vulnerability in &scan_result.vulnerabilities {
//         sqlx::query!(
//             r#"
//             INSERT INTO package_vulnerabilities (
//                 derivation_path, cve_id, fixed_version, detection_method
//             )
//             VALUES ($1, $2, $3, $4)
//             ON CONFLICT (derivation_path, cve_id) DO UPDATE SET
//                 fixed_version = EXCLUDED.fixed_version,
//                 detection_method = EXCLUDED.detection_method,
//                 updated_at = NOW()
//             "#,
//             vulnerability.derivation_path,
//             vulnerability.cve_id,
//             vulnerability.fixed_version,
//             vulnerability.detection_method
//         )
//         .execute(&mut *tx)
//         .await?;
//     }
//
//     // Commit the transaction
//     tx.commit().await?;
//
//     Ok(())
// }

/// Get latest CVE scan for an evaluation target
/// Get latest CVE scan for an evaluation target
pub async fn get_latest_scan(pool: &PgPool, evaluation_target_id: i32) -> Result<Option<CveScan>> {
    let scan = sqlx::query_as!(
        CveScan,
        r#"
    SELECT 
        id,
        evaluation_target_id,
        scheduled_at,
        completed_at,
        status as "status: ScanStatus",
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
    WHERE evaluation_target_id = $1
    ORDER BY created_at DESC
    LIMIT 1
    "#,
        evaluation_target_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(scan)
}
