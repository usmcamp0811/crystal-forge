use crate::derivations::utils::get_store_path_from_drv;
use crate::derivations::{Derivation, DerivationType};
use crate::models::cve_scans::{CveScan, ScanStatus};
use crate::queries::attention;
use crate::vulnix::vulnix_parser::{VulnixParser, VulnixScanOutput};
use anyhow::Result;
use bigdecimal::BigDecimal;
use bigdecimal::FromPrimitive;
use chrono::Utc;
use sqlx::PgPool;
use sqlx::Row;
use tracing::{debug, error, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CveScanEligibleTarget {
    pub derivation_id: i32,
    pub config_name: String,
    pub hostname: Option<String>,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateCveScanOutcome {
    Created(Uuid),
    Existing(Uuid),
}

impl CreateCveScanOutcome {
    pub fn id(self) -> Uuid {
        match self {
            Self::Created(id) | Self::Existing(id) => id,
        }
    }

    pub fn was_created(self) -> bool {
        matches!(self, Self::Created(_))
    }
}

fn truncate_for_varchar(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
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

    let result = sqlx::query!(
        r#"
        INSERT INTO cve_scans (
            id, derivation_id, scanner_name, scanner_version,
            status, total_packages, total_vulnerabilities,
            critical_count, high_count, medium_count, low_count,
            attempts
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT (derivation_id) WHERE status IN ('pending', 'in_progress')
        DO NOTHING
        "#,
        scan_id,
        derivation_id,
        scanner_name,
        scanner_version,
        "in_progress" as &str,
        0i32,
        0i32,
        0i32,
        0i32,
        0i32,
        0i32,
        1i32
    )
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
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
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "ON CONFLICT returned 0 rows but no active scan found for derivation {}",
                derivation_id
            )
        })?;
        return Ok(CreateCveScanOutcome::Existing(existing));
    }

    Ok(CreateCveScanOutcome::Created(scan_id))
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

/// Mark a specific CVE scan as failed.
pub async fn mark_cve_scan_failed(
    pool: &PgPool,
    scan_id: Uuid,
    target: &Derivation,
    error_message: &str,
) -> Result<()> {
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

/// Mark a specific CVE scan as failed when the full derivation row is not
/// available (for example, if loading the derivation itself failed).
pub async fn mark_cve_scan_failed_by_id(
    pool: &PgPool,
    scan_id: Uuid,
    derivation_id: i32,
    error_message: &str,
) -> Result<()> {
    let metadata = serde_json::json!({
        "error": error_message,
        "derivation_id": derivation_id,
    });

    sqlx::query!(
        r#"
        UPDATE cve_scans
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

/// Save complete scan results to database
pub async fn save_scan_results(
    pool: &PgPool,
    scan_id: Uuid,
    vulnix_results: &VulnixScanOutput,
    scan_duration_ms: Option<i32>,
) -> Result<()> {
    save_scan_results_with_store_path_override(
        pool,
        scan_id,
        vulnix_results,
        scan_duration_ms,
        None,
    )
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

/// Mark scans that have been stuck `in_progress` for more than the given
/// threshold as `failed` so the derivation becomes eligible for re-scanning.
/// This prevents a server crash between `mark_scan_in_progress` and
/// `save_scan_results` from permanently poisoning a derivation.
///
/// Returns the number of scans that were recovered so callers can log the event.
pub async fn recover_stale_scans(
    pool: &PgPool,
    stale_threshold: std::time::Duration,
) -> Result<i64> {
    let result = sqlx::query!(
        r#"
        UPDATE cve_scans
        SET status = 'failed',
            completed_at = NOW(),
            attempts = attempts + 1,
            scan_metadata = COALESCE(scan_metadata, '{}'::jsonb) || 
                           jsonb_build_object('stale_recovered_at', NOW()::text,
                                              'stale_recovery_reason', 'server-crash-recovery')
        WHERE status = 'in_progress'
          AND created_at < NOW() - $1 * INTERVAL '1 second'
        "#,
        stale_threshold.as_secs() as i64
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() as i64)
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
    use crate::queries::commits::insert_commit;
    use crate::queries::derivations::{EvaluationStatus, insert_derivation};
    use crate::queries::flakes::insert_flake;

    /// Helper to create a minimal flake, commit, and derivation for testing.
    async fn setup_test_derivation(pool: &PgPool) -> (i32, String) {
        let flake = insert_flake(
            pool,
            "test-flake",
            "https://example.com/test.git",
            "main",
            "nixos",
        )
        .await
        .expect("flake should be created");

        insert_commit(
            pool,
            "abcdef1234567890abcdef1234567890abcdef12",
            "https://example.com/test.git",
            chrono::Utc::now(),
        )
        .await
        .expect("commit should be created");

        // Create a derivation with build-complete status
        let derivation = insert_derivation(
            pool,
            None, // no commit needed for this test
            "test-host",
            "nixos",
        )
        .await
        .expect("derivation should be created");

        // Update to build-complete with a store path so the scan queries pick it up
        sqlx::query(
            r#"
            UPDATE derivations
            SET status_id = $1,
                store_path = '/nix/store/00000000000000000000000000000000-test',
                completed_at = NOW(),
                derivation_path = '/nix/store/00000000000000000000000000000000-test.drv'
            WHERE id = $2
            "#,
        )
        .bind(EvaluationStatus::BuildComplete.as_id())
        .bind(derivation.id)
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

    /// An unscanned derivation with build-complete status should be selected
    /// for post-build scanning.
    #[sqlx::test]
    #[ignore = "requires test database creation privileges"]
    async fn get_targets_needing_cve_scan_selects_unscanned_derivation(pool: PgPool) {
        setup_test_derivation(&pool).await;

        let targets = get_targets_needing_cve_scan(&pool, Some(10), &[], None)
            .await
            .expect("should fetch targets");

        assert!(
            !targets.is_empty(),
            "unscanned derivation should be selected"
        );
        assert!(
            targets.iter().any(|d| d.derivation_name == "test-host"),
            "test-host should be among targets"
        );
    }

    /// A derivation with an in_progress scan should NOT be selected.
    #[sqlx::test]
    #[ignore = "requires test database creation privileges"]
    async fn get_targets_needing_cve_scan_excludes_in_progress(pool: PgPool) {
        let (derivation_id, _) = setup_test_derivation(&pool).await;

        let scan_id = create_cve_scan(&pool, derivation_id, "vulnix", None)
            .await
            .expect("scan should be created")
            .id();
        mark_scan_in_progress(&pool, scan_id)
            .await
            .expect("scan should be marked in_progress");

        let targets = get_targets_needing_cve_scan(&pool, Some(10), &[], None)
            .await
            .expect("should fetch targets");

        assert!(
            !targets.iter().any(|d| d.id == derivation_id),
            "derivation with in_progress scan should be excluded"
        );
    }

    /// recover_stale_scans should mark old in_progress scans as failed,
    /// making the derivation eligible again.
    #[sqlx::test]
    #[ignore = "requires test database creation privileges"]
    async fn recover_stale_scans_unblocks_derivation(pool: PgPool) {
        let (derivation_id, _) = setup_test_derivation(&pool).await;

        // Create a scan and mark it in_progress with an old timestamp.
        let scan_id = create_cve_scan(&pool, derivation_id, "vulnix", None)
            .await
            .expect("scan should be created")
            .id();
        mark_scan_in_progress(&pool, scan_id)
            .await
            .expect("scan should be marked in_progress");

        // Artificially age the scan so it appears stale.
        sqlx::query(
            r#"
            UPDATE cve_scans
            SET created_at = NOW() - '2 hours'::INTERVAL
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
    #[sqlx::test]
    #[ignore = "requires test database creation privileges"]
    async fn get_targets_needing_cve_rescan_selects_stale_scan(pool: PgPool) {
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
    }
}
