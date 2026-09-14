//! CVE-related database queries for the advanced CVE dashboard.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::api::models::{
    CveAffectedSystemDetail, CveDetail, CveFilters, CveFleetStats, CveJustification,
    CveJustificationInput, CveListItem, CvePackageGroup,
};
use crate::auth::extractors::AuthenticatedUser;

/// Limits a CSV export while permitting one extra row for overflow detection.
///
/// The endpoint rejects a result above this bound. It never emits a partial
/// export. Callers must narrow the existing CVE filters when the bound is
/// exceeded.
pub const MAX_CVE_EXPORT_ROWS: i64 = 1_000;

/// Reports why a complete scoped CVE export could not be loaded.
#[derive(Debug)]
pub enum CveExportError {
    /// The filtered result exceeds [`MAX_CVE_EXPORT_ROWS`].
    TooManyRows,
    /// PostgreSQL could not load the scoped export rows.
    Database(sqlx::Error),
}

impl std::fmt::Display for CveExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyRows => write!(
                formatter,
                "CVE export exceeds the {MAX_CVE_EXPORT_ROWS}-row limit; narrow the filters and retry"
            ),
            Self::Database(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for CveExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::TooManyRows => None,
            Self::Database(error) => Some(error),
        }
    }
}

/// Defines the environment boundary for an authenticated CVE dashboard read.
///
/// `All` is valid only for an authenticated Admin. Every non-Admin scope stores
/// the user's current environment memberships. An empty membership list returns
/// no occurrence-derived data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CveReadScope {
    /// Allows fleet-wide reads for an Admin.
    All,
    /// Allows reads only from the listed environments.
    Environments(Vec<Uuid>),
}

impl CveReadScope {
    /// Resolves the current dashboard scope for an authenticated user.
    ///
    /// # Errors
    ///
    /// Returns a database error when current environment memberships cannot be
    /// loaded for a non-Admin user.
    pub async fn for_user(pool: &PgPool, user: &AuthenticatedUser) -> Result<Self> {
        if user.is_admin() {
            return Ok(Self::All);
        }
        let environment_ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT environment_id FROM user_environment_memberships WHERE user_id=$1 ORDER BY environment_id",
        )
        .bind(user.user_id)
        .fetch_all(pool)
        .await?;
        Ok(Self::Environments(environment_ids))
    }

    fn environment_ids(&self) -> Option<&[Uuid]> {
        match self {
            Self::All => None,
            Self::Environments(environment_ids) => Some(environment_ids),
        }
    }
}

// The endpoint contract bounds exact vulnerability rows and relationship
// hydration to one batch of 1,000 stable identities.
const MAX_EXACT_SYSTEM_VULNERABILITIES: i64 = 1_000;

/// Contains one vulnerability selected from authoritative deployed scan evidence.
#[derive(Debug, sqlx::FromRow)]
pub struct ExactSystemVulnerabilityRow {
    /// Identifies the authoritative scan that supplied this row.
    pub scan_id: Uuid,
    /// Gives the package derivation path that supplied this row.
    pub occurrence_derivation_path: String,
    /// Gives the canonical CVE identifier.
    pub cve_id: String,
    /// Gives the canonical package identity used by exact-CVE findings.
    pub canonical_package_name: String,
    /// Gives the package name emitted by the scanner.
    pub package_name: String,
    /// Gives the package version emitted by the scanner.
    pub installed_version: String,
    /// Gives the normalized severity derived from current CVE metadata.
    pub severity: String,
    /// Gives the current CVSS v3 score when available.
    pub cvss_score: Option<f64>,
    /// Gives the current CVE description.
    pub description: String,
    /// Gives the current known fixed version for the exact package derivation.
    pub fixed_version: Option<String>,
    /// Gives the authoritative scan completion time.
    pub first_seen: Option<DateTime<Utc>>,
    /// Gives the current CVE publication time.
    pub published_at: Option<DateTime<Utc>>,
    /// Gives the current fix-availability status.
    pub status: String,
    /// Gives the applicable system or fleet justification category.
    pub justification_category: Option<String>,
    /// Gives the applicable system or fleet justification reason.
    pub justification_reason: Option<String>,
    /// Gives the applicable justification update time.
    pub justification_updated_at: Option<DateTime<Utc>>,
}

/// Fetches vulnerabilities from the latest exact scan for the deployed generation.
///
/// The query returns no rows unless the system's current reported generation and
/// store path have one verified retained snapshot with an available integrity-v1
/// artifact. Row existence and installed versions come only from that retained
/// derivation's latest completed evidence-schema-1 scan. One deterministic
/// occurrence represents each stable CVE and canonical-package identity.
///
/// # Errors
///
/// Returns a database error when authoritative evidence or current metadata
/// cannot be loaded.
pub async fn fetch_exact_system_vulnerabilities(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Vec<ExactSystemVulnerabilityRow>> {
    let rows = sqlx::query_as::<_, ExactSystemVulnerabilityRow>(
        r#"WITH authoritative_scan AS (
             SELECT system.id AS system_id,scan.id AS scan_id,
                    scan.completed_at
             FROM systems system
             JOIN LATERAL (
               SELECT state.store_path,state.generation
               FROM system_states state
               WHERE state.hostname=system.hostname
                 AND state.store_path IS NOT NULL
                 AND state.generation IS NOT NULL
                 AND state.generation_matches_current_store_path IS TRUE
                 AND btrim(state.store_path)<>''
               ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
             ) deployed ON true
             JOIN evaluation_generation_snapshots retained
               ON retained.system_id=system.id
              AND retained.generation=deployed.generation
              AND retained.source_store_path=deployed.store_path
              AND retained.lineage_verified
             JOIN evaluation_snapshots artifact
               ON artifact.id=retained.snapshot_id
              AND artifact.commit_id=retained.commit_id
              AND artifact.configuration_name=retained.configuration_name
              AND artifact.lifecycle='available'
              AND artifact.integrity_version=1
             JOIN derivations derivation
               ON derivation.id=retained.derivation_id
              AND derivation.commit_id=retained.commit_id
              AND derivation.derivation_name=retained.configuration_name
              AND derivation.derivation_type='nixos'
              AND COALESCE(derivation.store_path,derivation.expected_store_path)
                  =retained.source_store_path
             JOIN LATERAL (
               SELECT candidate.id,candidate.completed_at
               FROM cve_scans candidate
               WHERE candidate.derivation_id=derivation.id
                 AND candidate.status='completed'
                 AND candidate.completed_at IS NOT NULL
                 AND candidate.evidence_schema_version=1
               ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
             ) scan ON true
             WHERE system.id=$1
           ), selected_observations AS (
             SELECT DISTINCT ON (
                      observation.canonical_cve_id,
                      observation.canonical_package_name)
                    authority.system_id,authority.scan_id,
                    authority.completed_at,
                    observation.canonical_cve_id,
                    observation.canonical_package_name,
                    observation.observed_package_name,
                    observation.observed_package_version,
                    observation.observed_derivation_path
             FROM authoritative_scan authority
             JOIN cve_scan_vulnerability_observations observation
               ON observation.scan_id=authority.scan_id
             WHERE NOT observation.is_whitelisted
             ORDER BY observation.canonical_cve_id,
                      observation.canonical_package_name,
                      observation.observed_derivation_path
           )
           SELECT observation.scan_id,observation.observed_derivation_path
                      AS occurrence_derivation_path,
                  observation.canonical_cve_id AS cve_id,
                  observation.canonical_package_name,
                  observation.observed_package_name AS package_name,
                  observation.observed_package_version AS installed_version,
                  lower(severity_from_cvss(cve.cvss_v3_score)) AS severity,
                  cve.cvss_v3_score::double precision AS cvss_score,
                  COALESCE(cve.description,'') AS description,
                  package_metadata.fixed_version,
                  observation.completed_at AS first_seen,
                  cve.published_date::timestamptz AS published_at,
                  CASE WHEN package_metadata.fixed_version IS NULL
                       THEN 'open' ELSE 'fix_available' END AS status,
                  justification.category AS justification_category,
                  justification.reason AS justification_reason,
                  justification.updated_at AS justification_updated_at
           FROM selected_observations observation
           JOIN cves cve ON cve.id=observation.canonical_cve_id
           LEFT JOIN LATERAL (
             SELECT vulnerability.fixed_version
             FROM derivations package_derivation
             JOIN package_vulnerabilities vulnerability
               ON vulnerability.derivation_id=package_derivation.id
              AND vulnerability.cve_id=observation.canonical_cve_id
             WHERE package_derivation.derivation_path
                   =observation.observed_derivation_path
             ORDER BY package_derivation.id DESC LIMIT 1
           ) package_metadata ON true
           LEFT JOIN LATERAL (
             SELECT candidate.category,candidate.reason,candidate.updated_at
             FROM system_cve_justifications candidate
             WHERE candidate.cve_id=observation.canonical_cve_id
               AND (candidate.system_id=observation.system_id
                 OR candidate.system_id IS NULL)
             ORDER BY (candidate.system_id IS NOT NULL) DESC,
                      candidate.updated_at DESC
             LIMIT 1
           ) justification ON true
           ORDER BY cvss_score DESC NULLS LAST,cve_id,canonical_package_name
           LIMIT $2"#,
    )
    .bind(system_id)
    .bind(MAX_EXACT_SYSTEM_VULNERABILITIES)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

fn default_cve_fleet_stats() -> CveFleetStats {
    CveFleetStats {
        total_cves: 0,
        critical: 0,
        high: 0,
        medium: 0,
        low: 0,
        exploited: 0,
        fixable: 0,
        environments_affected: 0,
        systems_affected: 0,
        outstanding: 0,
        accepted: 0,
        scheduled: 0,
    }
}

#[derive(sqlx::FromRow)]
struct CvePackageStatsRow {
    package_name: String,
    cve_count: i64,
    critical_count: i64,
    high_count: i64,
    medium_count: i64,
    low_count: i64,
    environments_count: i64,
    total_affected_systems: i64,
    fixable_count: i64,
    outstanding_count: i64,
    exploited_count: i64,
    max_cvss: Option<f32>,
    severity_score: i64,
}

/// Fetches a bounded CVE/package list from the caller's visible occurrences.
///
/// Filters are AND combined. Search matches the CVE ID, package name, or title.
/// The query applies [`CveReadScope`] before every count and status rollup.
///
/// # Errors
///
/// Returns a database error when the scoped list cannot be loaded.
pub async fn fetch_cve_list(
    pool: &PgPool,
    scope: &CveReadScope,
    filters: &CveFilters,
) -> Result<Vec<CveListItem>> {
    let limit = filters.limit.unwrap_or(500).min(1000);
    Ok(fetch_cve_rows(pool, scope, filters, limit).await?)
}

/// Fetches every filtered CVE/package row when the result fits the export bound.
///
/// This query applies the same exact occurrence authority, caller scope,
/// filters, and ordering as [`fetch_cve_list`]. It does not apply the list
/// pagination parameter. The query requests one extra row so an oversized
/// result is rejected instead of silently truncated.
///
/// # Errors
///
/// Returns [`CveExportError::TooManyRows`] when the filtered result exceeds
/// [`MAX_CVE_EXPORT_ROWS`]. Returns [`CveExportError::Database`] when PostgreSQL
/// cannot load the scoped rows.
pub async fn fetch_cves_for_export(
    pool: &PgPool,
    scope: &CveReadScope,
    filters: &CveFilters,
) -> std::result::Result<Vec<CveListItem>, CveExportError> {
    let rows = fetch_cve_rows(pool, scope, filters, MAX_CVE_EXPORT_ROWS + 1)
        .await
        .map_err(CveExportError::Database)?;
    if rows.len() > MAX_CVE_EXPORT_ROWS as usize {
        return Err(CveExportError::TooManyRows);
    }
    Ok(rows)
}

async fn fetch_cve_rows(
    pool: &PgPool,
    scope: &CveReadScope,
    filters: &CveFilters,
    limit: i64,
) -> std::result::Result<Vec<CveListItem>, sqlx::Error> {
    let severity_param = filters.severity.as_ref().map(|s| s.to_uppercase());
    let fix_status_param = filters.fix_status.clone();
    let triage_status_param = filters.triage_status.clone();
    let package_param = filters.package.as_ref().map(|p| format!("%{p}%"));
    let search_param = filters.search.as_ref().map(|s| format!("%{s}%"));
    let sort_param = filters.sort.as_deref().unwrap_or("severity");

    let rows = sqlx::query_as::<_, CveListItem>(
        r#"
        SELECT
            cve_id,
            cvss_v3_score::real AS cvss_v3_score,
            UPPER(COALESCE(severity, 'UNKNOWN')) AS severity,
            COALESCE(title, '') AS title,
            cvss_vector,
            published_date,
            COALESCE(exploited, FALSE) AS exploited,
            package_name,
            installed_version,
            fixed_version,
            COALESCE(fix_status, 'open') AS fix_status,
            COALESCE(affected_count, 0)::bigint AS affected_count,
            affected_environments,
            first_seen,
            last_seen,
            COALESCE(age_days, 0)::int AS age_days,
            LOWER(COALESCE(triage_status, 'outstanding')) AS triage_status
        FROM cve_list_for_environment_scope($1)
        WHERE
            ($2::text IS NULL OR UPPER(severity) = $2)
            AND (
                $3::text IS NULL
                OR ($3 = 'available' AND fix_status = 'fix_available')
                OR ($3 = 'pending' AND fix_status = 'open')
                OR ($3 = 'exploited' AND exploited = TRUE)
            )
            AND ($4::text IS NULL OR LOWER(triage_status) = LOWER($4))
            AND ($5::text IS NULL OR package_name ILIKE $5)
            AND (
                $6::text IS NULL
                OR cve_id ILIKE $6
                OR package_name ILIKE $6
                OR title ILIKE $6
            )
        ORDER BY
            CASE
                WHEN $7 = 'severity' THEN
                    CASE UPPER(severity)
                        WHEN 'CRITICAL' THEN 1
                        WHEN 'HIGH' THEN 2
                        WHEN 'MEDIUM' THEN 3
                        WHEN 'LOW' THEN 4
                        ELSE 5
                    END
                ELSE NULL
            END ASC NULLS LAST,
            CASE WHEN $7 = 'severity' THEN cvss_v3_score END DESC NULLS LAST,
            CASE WHEN $7 = 'cvss' THEN cvss_v3_score END DESC NULLS LAST,
            CASE WHEN $7 = 'age' THEN age_days END ASC NULLS LAST,
            CASE WHEN $7 = 'affected' THEN affected_count END DESC NULLS LAST,
            cve_id ASC
        LIMIT $8
        "#,
    )
    .bind(scope.environment_ids())
    .bind(severity_param)
    .bind(fix_status_param)
    .bind(triage_status_param)
    .bind(package_param)
    .bind(search_param)
    .bind(sort_param)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Fetches CVEs grouped by package from the caller's visible occurrences.
///
/// Package system totals count distinct systems after all active filters.
///
/// # Errors
///
/// Returns a database error when scoped package aggregation fails.
pub async fn fetch_cve_packages_grouped(
    pool: &PgPool,
    scope: &CveReadScope,
    filters: &CveFilters,
) -> Result<Vec<CvePackageGroup>> {
    let severity_param = filters.severity.as_ref().map(|s| s.to_uppercase());
    let fix_status_param = filters.fix_status.clone();
    let triage_status_param = filters.triage_status.clone();
    let package_param = filters.package.as_ref().map(|p| format!("%{p}%"));
    let search_param = filters.search.as_ref().map(|s| format!("%{s}%"));

    // 1) Aggregate package cards over the full filtered dataset (no list-row cap).
    let package_stats = sqlx::query_as::<_, CvePackageStatsRow>(
        r#"
        WITH filtered AS (
            SELECT
                cve_id,
                package_name,
                UPPER(COALESCE(severity, 'UNKNOWN')) AS severity,
                COALESCE(affected_count, 0)::bigint AS affected_count,
                COALESCE(fix_status, 'open') AS fix_status,
                LOWER(COALESCE(triage_status, 'outstanding')) AS triage_status,
                COALESCE(exploited, FALSE) AS exploited,
                cvss_v3_score::real AS cvss_v3_score,
                affected_environments
            FROM cve_list_for_environment_scope($1)
            WHERE
                ($2::text IS NULL OR UPPER(severity) = $2)
                AND (
                    $3::text IS NULL
                    OR ($3 = 'available' AND fix_status = 'fix_available')
                    OR ($3 = 'pending' AND fix_status = 'open')
                    OR ($3 = 'exploited' AND exploited = TRUE)
                )
                AND ($4::text IS NULL OR LOWER(triage_status) = LOWER($4))
                AND ($5::text IS NULL OR package_name ILIKE $5)
                AND (
                    $6::text IS NULL
                    OR cve_id ILIKE $6
                    OR package_name ILIKE $6
                    OR title ILIKE $6
                )
                AND package_name IS NOT NULL
        ),
        package_counts AS (
            SELECT
                package_name,
                COUNT(*)::bigint as cve_count,
                COUNT(*) FILTER (WHERE severity = 'CRITICAL')::bigint as critical_count,
                COUNT(*) FILTER (WHERE severity = 'HIGH')::bigint as high_count,
                COUNT(*) FILTER (WHERE severity = 'MEDIUM')::bigint as medium_count,
                COUNT(*) FILTER (WHERE severity = 'LOW')::bigint as low_count,
                COUNT(*) FILTER (WHERE fix_status = 'fix_available')::bigint as fixable_count,
                COUNT(*) FILTER (WHERE triage_status = 'outstanding')::bigint as outstanding_count,
                COUNT(*) FILTER (WHERE exploited = TRUE)::bigint as exploited_count,
                MAX(cvss_v3_score)::real as max_cvss,
                SUM(
                    CASE severity
                        WHEN 'CRITICAL' THEN 1000
                        WHEN 'HIGH' THEN 100
                        WHEN 'MEDIUM' THEN 10
                        WHEN 'LOW' THEN 1
                        ELSE 0
                    END
                )::bigint as severity_score
            FROM filtered
            GROUP BY package_name
        ),
        package_occurrence_counts AS (
            SELECT
                f.package_name,
                COUNT(DISTINCT occurrence.environment_id)::bigint as environments_count,
                COUNT(DISTINCT occurrence.system_id)::bigint as total_affected_systems
            FROM filtered f
            JOIN view_current_exact_cve_occurrences occurrence
              ON occurrence.cve_id=f.cve_id
             AND occurrence.package_name=f.package_name
             AND ($1::uuid[] IS NULL
                  OR occurrence.environment_id=ANY($1))
            GROUP BY f.package_name
        )
        SELECT
            pc.package_name,
            pc.cve_count,
            pc.critical_count,
            pc.high_count,
            pc.medium_count,
            pc.low_count,
            COALESCE(po.environments_count, 0)::bigint as environments_count,
            COALESCE(po.total_affected_systems, 0)::bigint as total_affected_systems,
            pc.fixable_count,
            pc.outstanding_count,
            pc.exploited_count,
            pc.max_cvss,
            pc.severity_score
        FROM package_counts pc
        LEFT JOIN package_occurrence_counts po ON po.package_name = pc.package_name
        ORDER BY pc.severity_score DESC, pc.max_cvss DESC NULLS LAST, pc.package_name ASC
        LIMIT 100
        "#,
    )
    .bind(scope.environment_ids())
    .bind(severity_param.clone())
    .bind(fix_status_param.clone())
    .bind(triage_status_param.clone())
    .bind(package_param.clone())
    .bind(search_param.clone())
    .fetch_all(pool)
    .await?;

    let selected_packages: Vec<String> = package_stats
        .iter()
        .map(|row| row.package_name.clone())
        .collect();

    // 2) Fetch nested rows in one query; cap nested rows per package via ROW_NUMBER.
    let nested_rows = sqlx::query_as::<_, CveListItem>(
        r#"
        WITH filtered AS (
            SELECT
                cve_id,
                cvss_v3_score::real AS cvss_v3_score,
                UPPER(COALESCE(severity, 'UNKNOWN')) AS severity,
                COALESCE(title, '') AS title,
                cvss_vector,
                published_date,
                COALESCE(exploited, FALSE) AS exploited,
                package_name,
                installed_version,
                fixed_version,
                COALESCE(fix_status, 'open') AS fix_status,
                COALESCE(affected_count, 0)::bigint AS affected_count,
                affected_environments,
                first_seen,
                last_seen,
                COALESCE(age_days, 0)::int AS age_days,
                LOWER(COALESCE(triage_status, 'outstanding')) AS triage_status
            FROM cve_list_for_environment_scope($1)
            WHERE
                ($2::text IS NULL OR UPPER(severity) = $2)
                AND (
                    $3::text IS NULL
                    OR ($3 = 'available' AND fix_status = 'fix_available')
                    OR ($3 = 'pending' AND fix_status = 'open')
                    OR ($3 = 'exploited' AND exploited = TRUE)
                )
                AND ($4::text IS NULL OR LOWER(triage_status) = LOWER($4))
                AND ($5::text IS NULL OR package_name ILIKE $5)
                AND (
                    $6::text IS NULL
                    OR cve_id ILIKE $6
                    OR package_name ILIKE $6
                    OR title ILIKE $6
                )
                AND package_name = ANY($7::text[])
        ),
        ranked AS (
            SELECT
                *,
                ROW_NUMBER() OVER (
                    PARTITION BY package_name
                    ORDER BY
                        CASE severity
                            WHEN 'CRITICAL' THEN 1
                            WHEN 'HIGH' THEN 2
                            WHEN 'MEDIUM' THEN 3
                            WHEN 'LOW' THEN 4
                            ELSE 5
                        END,
                        cvss_v3_score DESC NULLS LAST,
                        cve_id ASC
                ) AS rn
            FROM filtered
        )
        SELECT
            cve_id,cvss_v3_score,severity,title,cvss_vector,published_date,
            exploited,package_name,installed_version,fixed_version,fix_status,
            affected_count,affected_environments,first_seen,last_seen,age_days,
            triage_status
        FROM ranked
        WHERE rn <= 100
        ORDER BY package_name ASC, rn ASC
        "#,
    )
    .bind(scope.environment_ids())
    .bind(severity_param)
    .bind(fix_status_param)
    .bind(triage_status_param)
    .bind(package_param)
    .bind(search_param)
    .bind(&selected_packages)
    .fetch_all(pool)
    .await?;

    let mut nested_by_package: HashMap<String, Vec<CveListItem>> = HashMap::new();
    for row in nested_rows {
        let pkg_key = row.package_name.clone().unwrap_or_default();
        nested_by_package.entry(pkg_key).or_default().push(row);
    }

    let mut result = Vec::new();
    for row in package_stats {
        let cves = nested_by_package
            .remove(&row.package_name)
            .unwrap_or_default();

        result.push(CvePackageGroup {
            package_name: row.package_name,
            cve_count: row.cve_count,
            critical_count: row.critical_count,
            high_count: row.high_count,
            medium_count: row.medium_count,
            low_count: row.low_count,
            environments_count: row.environments_count,
            total_affected_systems: row.total_affected_systems,
            fixable_count: row.fixable_count,
            outstanding_count: row.outstanding_count,
            exploited_count: row.exploited_count,
            max_cvss: row.max_cvss,
            severity_score: row.severity_score,
            cves: Some(cves),
        });
    }

    Ok(result)
}

/// Fetches one deterministic visible package row for a CVE.
///
/// The legacy route does not identify a package. When more than one visible
/// package has the CVE, this query returns the first canonical package name.
///
/// # Errors
///
/// Returns `sqlx::Error::RowNotFound` when the CVE has no occurrence in the
/// caller's scope, or a database error when the scoped read fails.
pub async fn fetch_cve_detail(
    pool: &PgPool,
    scope: &CveReadScope,
    cve_id: &str,
) -> Result<CveDetail> {
    let detail = sqlx::query_as::<_, CveDetail>(
        r#"
        SELECT
            v.cve_id,v.cvss_v3_score::real AS cvss_v3_score,
            COALESCE(v.severity, 'UNKNOWN') AS severity,
            COALESCE(v.title, '') AS title,v.cvss_vector,c.cwe_id,
            v.published_date,c.modified_date,
            COALESCE(v.exploited, FALSE) AS exploited,v.package_name,
            v.installed_version,v.fixed_version,
            NULL::text AS detection_method,
            COALESCE(v.fix_status, 'open') AS fix_status
        FROM cve_list_for_environment_scope($1) v
        LEFT JOIN cves c ON c.id = v.cve_id
        WHERE v.cve_id = $2
        ORDER BY v.package_name ASC
        LIMIT 1
        "#,
    )
    .bind(scope.environment_ids())
    .bind(cve_id)
    .fetch_one(pool)
    .await?;

    Ok(detail)
}

/// Fetches detailed CVE metadata in the caller's transaction.
///
/// Fleet triage uses this form so response construction either succeeds before
/// commit or rolls back with the mutation.
///
/// # Errors
///
/// Returns an error when the CVE metadata row is unavailable or PostgreSQL
/// cannot execute the query.
pub(crate) async fn fetch_cve_detail_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    scope: &CveReadScope,
    cve_id: &str,
    package_name: &str,
) -> Result<CveDetail> {
    Ok(sqlx::query_as::<_, CveDetail>(
        r#"SELECT v.cve_id,
                  v.cvss_v3_score::real AS cvss_v3_score,
                  COALESCE(v.severity,'UNKNOWN') AS severity,
                  COALESCE(v.title,'') AS title,
                  v.cvss_vector,c.cwe_id,v.published_date,c.modified_date,
                  COALESCE(v.exploited,FALSE) AS exploited,
                  v.package_name,v.installed_version,v.fixed_version,
                  NULL::text AS detection_method,
                  COALESCE(v.fix_status,'open') AS fix_status
           FROM cve_list_for_environment_scope($1) v
           LEFT JOIN cves c ON c.id=v.cve_id
           WHERE v.cve_id=$2 AND v.package_name=$3
           LIMIT 1"#,
    )
    .bind(scope.environment_ids())
    .bind(cve_id)
    .bind(package_name)
    .fetch_one(&mut **tx)
    .await?)
}

/// Fetches distinct visible systems affected by a specific CVE.
///
/// # Errors
///
/// Returns a database error when authoritative occurrences cannot be loaded.
pub async fn fetch_cve_affected_systems(
    pool: &PgPool,
    scope: &CveReadScope,
    cve_id: &str,
) -> Result<Vec<CveAffectedSystemDetail>> {
    let systems = sqlx::query_as::<_, CveAffectedSystemDetail>(
        r#"
        WITH latest_per_system AS (
            SELECT DISTINCT ON (s.id)
                s.id as system_id,
                s.hostname,
                e.name as environment,
                ss.primary_ip_address,
                f.name as flake_name,
                f.id as flake_id,
                NULL::text as commit_hash,
                s.deployment_policy,
                occurrence.installed_version as current_package_version,
                occurrence.package_name
            FROM view_current_exact_cve_occurrences occurrence
            JOIN systems s ON s.id=occurrence.system_id
            LEFT JOIN environments e ON e.id=occurrence.environment_id
            LEFT JOIN flakes f ON s.flake_id = f.id
            LEFT JOIN LATERAL (
                SELECT state.primary_ip_address
                FROM system_states state
                WHERE state.hostname=s.hostname
                ORDER BY state.timestamp DESC,state.id DESC
                LIMIT 1
            ) ss ON TRUE
            WHERE occurrence.cve_id = $2
              AND ($1::uuid[] IS NULL
                   OR occurrence.environment_id=ANY($1))
            ORDER BY s.id,occurrence.package_name,
                     occurrence.observed_derivation_path
        )
        SELECT
            system_id,
            hostname,environment,primary_ip_address,flake_name,flake_id,
            commit_hash,deployment_policy,current_package_version
        FROM latest_per_system
        ORDER BY environment NULLS LAST, hostname
        "#,
    )
    .bind(scope.environment_ids())
    .bind(cve_id)
    .fetch_all(pool)
    .await?;

    Ok(systems)
}

/// Fetches visible justification history for a CVE.
///
/// Admin reads retain fleet-wide and historical rows. Scoped reads return only
/// per-system rows backed by a current visible occurrence; they exclude
/// fleet-wide rows because those rows can disclose hidden CVE presence.
///
/// # Errors
///
/// Returns a database error when visible history cannot be loaded.
pub async fn fetch_cve_justifications(
    pool: &PgPool,
    scope: &CveReadScope,
    cve_id: &str,
) -> Result<Vec<CveJustification>> {
    let justifications = sqlx::query_as::<_, CveJustification>(
        r#"
        SELECT 
            scj.system_id,scj.cve_id,scj.category,scj.reason,scj.updated_by,
            scj.updated_at,scj.created_at,u.username AS updated_by_username
        FROM system_cve_justifications scj
        LEFT JOIN users u ON scj.updated_by = u.id
        WHERE scj.cve_id = $2
          AND (
            $1::uuid[] IS NULL
            OR (scj.system_id IS NOT NULL AND EXISTS (
              SELECT 1 FROM view_current_exact_cve_occurrences occurrence
              WHERE occurrence.system_id=scj.system_id
                AND occurrence.cve_id=scj.cve_id
                AND occurrence.environment_id=ANY($1)
            ))
          )
        ORDER BY scj.updated_at DESC
        "#,
    )
    .bind(scope.environment_ids())
    .bind(cve_id)
    .fetch_all(pool)
    .await?;

    Ok(justifications)
}

/// Insert or update a CVE justification.
///
/// Fleet-wide rows (system_id IS NULL) use a separate partial-unique-index
/// conflict target so PostgreSQL can upsert them correctly — NULL values are
/// not considered equal by normal UNIQUE constraints, so repeated fleet-wide
/// saves would otherwise insert duplicates.
///
/// Per-system rows (system_id IS NOT NULL) continue to use the composite
/// primary key (system_id, cve_id) as the conflict target.
pub async fn insert_cve_justification(pool: &PgPool, input: &CveJustificationInput) -> Result<()> {
    let mut tx = pool.begin().await?;
    // CONCURRENCY: The canonical-CVE lock precedes every system and finding
    // lock so fleet and per-system justification writes serialize with POA&M.
    let system_ids = if let Some(system_id) = input.system_id {
        vec![system_id]
    } else {
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT DISTINCT system_id FROM poam_cve_findings WHERE canonical_cve_id=$1 ORDER BY system_id",
        )
        .bind(&input.cve_id)
        .fetch_all(&mut *tx)
        .await?
    };
    // CONCURRENCY: Justification state changes verification semantics. Acquire
    // each system sentinel and its sorted policy/CVE keys before mutation.
    crate::services::composite_enforcement::lock_poam_cve_scope_for_systems_tx(
        &mut tx,
        &input.cve_id,
        &system_ids,
    )
    .await?;
    if input.system_id.is_none() {
        // Fleet-wide: conflict on the partial unique index WHERE system_id IS NULL
        sqlx::query(
            r#"
            INSERT INTO system_cve_justifications
                (system_id, cve_id, category, reason, updated_by, updated_at)
            VALUES (NULL, $1, $2, $3, $4, NOW())
            ON CONFLICT (cve_id)
            WHERE system_id IS NULL
            DO UPDATE SET
                category   = EXCLUDED.category,
                reason     = EXCLUDED.reason,
                updated_by = EXCLUDED.updated_by,
                updated_at = NOW()
            "#,
        )
        .bind(&input.cve_id)
        .bind(&input.category)
        .bind(&input.reason)
        .bind(input.updated_by)
        .execute(&mut *tx)
        .await?;
    } else {
        // Per-system: conflict on partial unique index (system_id, cve_id)
        // WHERE system_id IS NOT NULL.
        sqlx::query(
            r#"
            INSERT INTO system_cve_justifications
                (system_id, cve_id, category, reason, updated_by, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (system_id, cve_id)
            WHERE system_id IS NOT NULL
            DO UPDATE SET
                category   = EXCLUDED.category,
                reason     = EXCLUDED.reason,
                updated_by = EXCLUDED.updated_by,
                updated_at = NOW()
            "#,
        )
        .bind(input.system_id)
        .bind(&input.cve_id)
        .bind(&input.category)
        .bind(&input.reason)
        .bind(input.updated_by)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Revoke the fleet-wide justification for a CVE (DELETE WHERE system_id IS NULL).
///
/// Per-system justifications are left untouched.
/// Returns Ok(()) whether or not a row existed (idempotent).
pub async fn revoke_fleet_cve_justification(pool: &PgPool, cve_id: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    let system_ids = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT DISTINCT system_id FROM poam_cve_findings WHERE canonical_cve_id=$1 ORDER BY system_id",
    )
    .bind(cve_id)
    .fetch_all(&mut *tx)
    .await?;
    crate::services::composite_enforcement::lock_poam_cve_scope_for_systems_tx(
        &mut tx,
        cve_id,
        &system_ids,
    )
    .await?;
    sqlx::query(
        r#"
        DELETE FROM system_cve_justifications
        WHERE cve_id = $1
          AND system_id IS NULL
        "#,
    )
    .bind(cve_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Fetches CVE statistics from authoritative occurrences in the caller's scope.
///
/// CVE totals count exact CVE/package rows. System and environment totals count
/// distinct identities and never sum per-CVE occurrence counts.
///
/// # Errors
///
/// Returns a database error when scoped occurrence aggregation fails.
pub async fn fetch_cve_fleet_stats(pool: &PgPool, scope: &CveReadScope) -> Result<CveFleetStats> {
    let stats = sqlx::query_as::<_, CveFleetStats>(
        r#"
        WITH scoped_list AS (
          SELECT * FROM cve_list_for_environment_scope($1)
        ), scoped_occurrences AS (
          SELECT occurrence.system_id,occurrence.environment_id
          FROM view_current_exact_cve_occurrences occurrence
          WHERE $1::uuid[] IS NULL OR occurrence.environment_id=ANY($1)
        )
        SELECT COUNT(*)::bigint AS total_cves,
          COUNT(*) FILTER (WHERE severity='CRITICAL')::bigint AS critical,
          COUNT(*) FILTER (WHERE severity='HIGH')::bigint AS high,
          COUNT(*) FILTER (WHERE severity='MEDIUM')::bigint AS medium,
          COUNT(*) FILTER (WHERE severity='LOW')::bigint AS low,
          COUNT(*) FILTER (WHERE exploited)::bigint AS exploited,
          COUNT(*) FILTER (WHERE fix_status='fix_available')::bigint AS fixable,
          (SELECT COUNT(DISTINCT environment_id) FROM scoped_occurrences)::bigint
            AS environments_affected,
          (SELECT COUNT(DISTINCT system_id) FROM scoped_occurrences)::bigint
            AS systems_affected,
          COUNT(*) FILTER (WHERE triage_status='outstanding')::bigint AS outstanding,
          COUNT(*) FILTER (WHERE triage_status='accepted')::bigint AS accepted,
          COUNT(*) FILTER (WHERE triage_status='scheduled')::bigint AS scheduled
        FROM scoped_list
        "#,
    )
    .bind(scope.environment_ids())
    .fetch_optional(pool)
    .await?;

    Ok(stats.unwrap_or_else(default_cve_fleet_stats))
}

/// Fetches distinct package names visible in the caller's scope.
///
/// # Errors
///
/// Returns a database error when the scoped package list cannot be loaded.
pub async fn fetch_package_names(pool: &PgPool, scope: &CveReadScope) -> Result<Vec<String>> {
    let packages = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT package_name as "package_name!"
        FROM cve_list_for_environment_scope($1)
        WHERE package_name IS NOT NULL
        ORDER BY package_name
        LIMIT 500
        "#,
    )
    .bind(scope.environment_ids())
    .fetch_all(pool)
    .await?;

    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Query builder unit tests (pure logic, no DB connection needed) ──

    fn build_list_query(filters: &CveFilters) -> String {
        let mut query = String::from("SELECT * FROM view_cve_list_with_metadata WHERE 1=1\n");
        let mut conditions = Vec::new();

        if let Some(ref severity) = filters.severity {
            conditions.push(format!("AND severity = '{}'", severity.to_uppercase()));
        }
        if let Some(ref fix_status) = filters.fix_status {
            match fix_status.as_str() {
                "available" => conditions.push("AND fix_status = 'fix_available'".to_string()),
                "pending" => conditions.push("AND fix_status = 'open'".to_string()),
                "exploited" => conditions.push("AND exploited = TRUE".to_string()),
                _ => {}
            }
        }
        if let Some(ref triage_status) = filters.triage_status {
            conditions.push(format!("AND triage_status = '{}'", triage_status));
        }
        if let Some(ref package) = filters.package {
            conditions.push(format!(
                "AND package_name ILIKE '%{}%'",
                package.replace('\'', "''")
            ));
        }
        if let Some(ref search) = filters.search {
            let esc = search.replace('\'', "''");
            conditions.push(format!(
                "AND (cve_id ILIKE '%{0}%' OR package_name ILIKE '%{0}%' OR title ILIKE '%{0}%')",
                esc
            ));
        }
        for cond in conditions {
            query.push_str(&cond);
            query.push('\n');
        }
        query
    }

    #[test]
    fn no_filters_produces_base_query_only() {
        let q = build_list_query(&CveFilters::default());
        assert!(!q.contains("AND severity"));
        assert!(!q.contains("AND fix_status"));
        assert!(!q.contains("AND triage_status"));
        assert!(!q.contains("AND package_name"));
        assert!(!q.contains("AND (cve_id"));
    }

    #[test]
    fn severity_filter_uppercased_in_query() {
        let f = CveFilters {
            severity: Some("critical".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("severity = 'CRITICAL'"), "query: {}", q);
    }

    #[test]
    fn fix_status_available_maps_to_fix_available() {
        let f = CveFilters {
            fix_status: Some("available".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("fix_status = 'fix_available'"), "query: {}", q);
    }

    #[test]
    fn fix_status_pending_maps_to_open() {
        let f = CveFilters {
            fix_status: Some("pending".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("fix_status = 'open'"), "query: {}", q);
    }

    #[test]
    fn fix_status_exploited_maps_to_boolean_filter() {
        let f = CveFilters {
            fix_status: Some("exploited".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("exploited = TRUE"), "query: {}", q);
    }

    #[test]
    fn unknown_fix_status_is_silently_ignored() {
        let f = CveFilters {
            fix_status: Some("wontfix".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(!q.contains("AND fix_status"), "query: {}", q);
        assert!(!q.contains("wontfix"), "query: {}", q);
    }

    #[test]
    fn triage_status_filter_passed_through_verbatim() {
        for status in ["outstanding", "scheduled", "accepted"] {
            let f = CveFilters {
                triage_status: Some(status.to_string()),
                ..Default::default()
            };
            let q = build_list_query(&f);
            assert!(
                q.contains(&format!("triage_status = '{}'", status)),
                "query for status={}: {}",
                status,
                q
            );
        }
    }

    #[test]
    fn search_filter_escapes_single_quotes() {
        let f = CveFilters {
            search: Some("O'Reilly".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        // Single quote must be doubled to prevent SQL injection
        assert!(q.contains("O''Reilly"), "query: {}", q);
        assert!(!q.contains("O'Reilly") || q.matches("O''Reilly").count() > 0);
    }

    #[test]
    fn package_filter_escapes_single_quotes() {
        let f = CveFilters {
            package: Some("lib's-pkg".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("lib''s-pkg"), "query: {}", q);
    }

    #[test]
    fn multiple_filters_all_appear_in_query() {
        let f = CveFilters {
            severity: Some("high".to_string()),
            fix_status: Some("available".to_string()),
            triage_status: Some("outstanding".to_string()),
            search: Some("openssl".to_string()),
            ..Default::default()
        };
        let q = build_list_query(&f);
        assert!(q.contains("severity = 'HIGH'"));
        assert!(q.contains("fix_status = 'fix_available'"));
        assert!(q.contains("triage_status = 'outstanding'"));
        assert!(q.contains("openssl"));
    }

    #[test]
    fn cve_filters_default_all_none() {
        let f = CveFilters::default();
        assert!(f.severity.is_none());
        assert!(f.fix_status.is_none());
        assert!(f.triage_status.is_none());
        assert!(f.package.is_none());
        assert!(f.search.is_none());
        assert!(f.sort.is_none());
        assert!(f.limit.is_none());
    }

    #[test]
    fn default_cve_fleet_stats_is_all_zero() {
        let stats = default_cve_fleet_stats();
        assert_eq!(stats.total_cves, 0);
        assert_eq!(stats.critical, 0);
        assert_eq!(stats.high, 0);
        assert_eq!(stats.medium, 0);
        assert_eq!(stats.low, 0);
        assert_eq!(stats.exploited, 0);
        assert_eq!(stats.fixable, 0);
        assert_eq!(stats.environments_affected, 0);
        assert_eq!(stats.systems_affected, 0);
        assert_eq!(stats.outstanding, 0);
        assert_eq!(stats.accepted, 0);
        assert_eq!(stats.scheduled, 0);
    }

    // ── Live DB tests (require running PostgreSQL with migrations applied) ──

    #[tokio::test]
    #[ignore = "requires test database"]
    async fn fetch_cve_list_no_filters_returns_ok() {
        let pool = crate::config::db_pool().await.expect("test db pool");
        let result = fetch_cve_list(&pool, &CveReadScope::All, &CveFilters::default()).await;
        assert!(result.is_ok(), "error: {:?}", result.err());
    }

    #[tokio::test]
    #[ignore = "requires test database"]
    async fn fetch_cve_fleet_stats_returns_ok() {
        let pool = crate::config::db_pool().await.expect("test db pool");
        let result = fetch_cve_fleet_stats(&pool, &CveReadScope::All).await;
        assert!(result.is_ok(), "error: {:?}", result.err());
    }

    #[tokio::test]
    #[ignore = "requires test database"]
    async fn fetch_cve_list_severity_filter_constrains_results() {
        let pool = crate::config::db_pool().await.expect("test db pool");
        let f = CveFilters {
            severity: Some("critical".to_string()),
            ..Default::default()
        };
        let result = fetch_cve_list(&pool, &CveReadScope::All, &f)
            .await
            .expect("query failed");
        for cve in &result {
            assert_eq!(
                cve.severity.to_uppercase(),
                "CRITICAL",
                "expected only CRITICAL cves, got: {:?}",
                cve.severity
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires test database"]
    async fn fetch_cve_list_limit_respected() {
        let pool = crate::config::db_pool().await.expect("test db pool");
        let f = CveFilters {
            limit: Some(5),
            ..Default::default()
        };
        let result = fetch_cve_list(&pool, &CveReadScope::All, &f)
            .await
            .expect("query failed");
        assert!(
            result.len() <= 5,
            "expected at most 5 results, got {}",
            result.len()
        );
    }
}
