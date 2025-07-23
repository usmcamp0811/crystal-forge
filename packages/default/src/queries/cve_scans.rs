use crate::models::commits::Commit;
use crate::models::evaluation_targets::{EvaluationTarget, TargetType};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use tracing::error;

/// Get targets that need CVE scanning (ordered by priority)
/// Returns array of EvaluationTargets - additional info can be fetched separately
pub async fn get_targets_needing_cve_scan(
    pool: &PgPool,
    limit: Option<i64>,
) -> Result<Vec<EvaluationTarget>> {
    let limit_clause = limit.unwrap_or(50); // Default to 50 if no limit specified

    let targets = sqlx::query_as!(
        EvaluationTarget,
        r#"
        SELECT 
            et.id,
            et.commit_id,
            et.target_type as "target_type: TargetType",
            et.target_name,
            et.derivation_path,
            et.scheduled_at,
            et.completed_at,
            et.status
        FROM evaluation_targets et
        LEFT JOIN cve_scans cs ON et.id = cs.evaluation_target_id
        WHERE 
            et.completed_at IS NOT NULL 
            AND et.status = 'complete'
            AND et.derivation_path IS NOT NULL
            AND (
                cs.id IS NULL 
                OR (cs.status IN ('failed', 'pending') AND cs.attempts < 5)
            )
        ORDER BY 
            COALESCE(cs.attempts, 0) ASC,
            et.completed_at ASC
        LIMIT $1
        "#,
        limit_clause
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch targets needing CVE scan")?;

    Ok(targets)
}

/// Mark a CVE scan as failed in the database
async fn mark_cve_scan_failed(
    pool: &PgPool,
    target: &crate::models::evaluation_targets::EvaluationTarget,
    error_message: &str,
) -> Result<()> {
    let scan_metadata = serde_json::json!({
        "error": error_message,
        "failed_at": chrono::Utc::now()
    });

    sqlx::query!(
        r#"
        INSERT INTO cve_scans 
            (evaluation_target_id, status, attempts, scanner_name, scan_metadata)
        VALUES ($1, 'failed', 1, 'vulnix', $2)
        ON CONFLICT (evaluation_target_id) DO UPDATE SET
            status = 'failed',
            attempts = cve_scans.attempts + 1,
            scan_metadata = $2,
            created_at = NOW()
        "#,
        target.id,
        scan_metadata
    )
    .execute(pool)
    .await?;

    Ok(())
}

// Add this to your queries/cve_scans.rs file

use crate::vulnix::vulnix_parser::VulnixScanResult;
use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::{debug, info, warn};

/// Save complete scan results to the database
pub async fn save_scan_results(pool: &PgPool, scan_result: &VulnixScanResult) -> Result<()> {
    let mut tx = pool.begin().await.context("Failed to start transaction")?;

    // 1. Insert/update the CVE scan record
    sqlx::query!(
        r#"
        INSERT INTO cve_scans (
            id, evaluation_target_id, completed_at, status, scanner_name, 
            scanner_version, total_packages, total_vulnerabilities,
            critical_count, high_count, medium_count, low_count,
            scan_duration_ms, scan_metadata
        ) VALUES ($1, $2, NOW(), 'completed', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT (id) DO UPDATE SET
            completed_at = NOW(),
            status = 'completed',
            total_packages = $5,
            total_vulnerabilities = $6,
            critical_count = $7,
            high_count = $8,
            medium_count = $9,
            low_count = $10,
            scan_duration_ms = $11,
            scan_metadata = $12
        "#,
        scan_result.scan.id,
        scan_result.scan.evaluation_target_id,
        scan_result.scan.scanner_name,
        scan_result.scan.scanner_version,
        scan_result.scan.total_packages,
        scan_result.scan.total_vulnerabilities,
        scan_result.scan.critical_count,
        scan_result.scan.high_count,
        scan_result.scan.medium_count,
        scan_result.scan.low_count,
        scan_result.scan.scan_duration_ms,
        scan_result.scan.scan_metadata
    )
    .execute(&mut *tx)
    .await
    .context("Failed to insert CVE scan")?;

    // 2. Insert packages (with conflict handling)
    for package in &scan_result.packages {
        sqlx::query!(
            r#"
            INSERT INTO nix_packages (derivation_path, name, pname, version)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (derivation_path) DO UPDATE SET
                name = $2,
                pname = $3,
                version = $4
            "#,
            package.derivation_path,
            package.name,
            package.pname,
            package.version
        )
        .execute(&mut *tx)
        .await
        .context("Failed to insert nix package")?;
    }

    // 3. Insert scan-package relationships
    for scan_package in &scan_result.scan_packages {
        sqlx::query!(
            r#"
            INSERT INTO scan_packages (
                id, scan_id, derivation_path, is_runtime_dependency, dependency_depth
            ) VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (scan_id, derivation_path) DO NOTHING
            "#,
            scan_package.id,
            scan_package.scan_id,
            scan_package.derivation_path,
            scan_package.is_runtime_dependency,
            scan_package.dependency_depth
        )
        .execute(&mut *tx)
        .await
        .context("Failed to insert scan package")?;
    }

    // 4. Insert CVEs (with conflict handling)
    for cve in &scan_result.cves {
        sqlx::query!(
            r#"
            INSERT INTO cves (
                id, cvss_v3_score, cvss_v2_score, description, published_date,
                modified_date, vector, cwe_id, metadata
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (id) DO UPDATE SET
                cvss_v3_score = COALESCE($2, cves.cvss_v3_score),
                cvss_v2_score = COALESCE($3, cves.cvss_v2_score),
                description = COALESCE($4, cves.description),
                published_date = COALESCE($5, cves.published_date),
                modified_date = COALESCE($6, cves.modified_date),
                vector = COALESCE($7, cves.vector),
                cwe_id = COALESCE($8, cves.cwe_id),
                metadata = COALESCE($9, cves.metadata),
                updated_at = NOW()
            "#,
            cve.id,
            cve.cvss_v3_score,
            cve.cvss_v2_score,
            cve.description,
            cve.published_date,
            cve.modified_date,
            cve.vector,
            cve.cwe_id,
            cve.metadata
        )
        .execute(&mut *tx)
        .await
        .context("Failed to insert CVE")?;
    }

    // 5. Insert package vulnerabilities
    for vuln in &scan_result.package_vulnerabilities {
        sqlx::query!(
            r#"
            INSERT INTO package_vulnerabilities (
                id, derivation_path, cve_id, is_whitelisted, whitelist_reason,
                whitelist_expires_at, fixed_version, detection_method
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (derivation_path, cve_id) DO UPDATE SET
                is_whitelisted = $4,
                whitelist_reason = $5,
                whitelist_expires_at = $6,
                fixed_version = COALESCE($7, package_vulnerabilities.fixed_version),
                detection_method = $8,
                updated_at = NOW()
            "#,
            vuln.id,
            vuln.derivation_path,
            vuln.cve_id,
            vuln.is_whitelisted,
            vuln.whitelist_reason,
            vuln.whitelist_expires_at,
            vuln.fixed_version,
            vuln.detection_method
        )
        .execute(&mut *tx)
        .await
        .context("Failed to insert package vulnerability")?;
    }

    tx.commit().await.context("Failed to commit transaction")?;

    let summary = scan_result.summary();
    info!(
        "💾 Saved CVE scan results: {} packages, {} CVEs, {} vulnerabilities",
        summary.total_packages, summary.total_cves, summary.total_vulnerabilities
    );

    if summary.critical_count > 0 {
        warn!(
            "⚠️  {} critical vulnerabilities saved to database!",
            summary.critical_count
        );
    }

    Ok(())
}

/// Get the latest CVE scan for a specific evaluation target
pub async fn get_latest_scan_for_target(
    pool: &PgPool,
    evaluation_target_id: i32,
) -> Result<Option<crate::models::cve_scans::CveScan>> {
    let scan = sqlx::query_as!(
        crate::models::cve_scans::CveScan,
        r#"
        SELECT 
            id, evaluation_target_id, scheduled_at, completed_at, status,
            attempts, scanner_name, scanner_version, total_packages,
            total_vulnerabilities, critical_count, high_count, medium_count,
            low_count, scan_duration_ms, scan_metadata, created_at
        FROM cve_scans 
        WHERE evaluation_target_id = $1 
        ORDER BY created_at DESC 
        LIMIT 1
        "#,
        evaluation_target_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch latest CVE scan")?;

    Ok(scan)
}
