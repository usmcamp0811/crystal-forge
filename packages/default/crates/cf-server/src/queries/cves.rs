//! CVE-related database queries for the advanced CVE dashboard.

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeSet, HashMap};
use uuid::Uuid;

use crate::api::models::{
    CveAffectedSystemDetail, CveDetail, CveFilters, CveFleetStats, CveJustification,
    CveJustificationInput, CveListItem, CvePackageGroup, ExactCveAuthorityFailureReason,
    SystemCveInventoryAuthority, SystemCveInventoryMetadata, SystemCveInventoryParams,
    SystemCveInventorySeverityCounts, SystemCveInventorySource,
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

/// Gives the default number of stable identities in an inventory page.
pub const DEFAULT_SYSTEM_CVE_INVENTORY_PAGE_SIZE: u16 = 100;
/// Gives the largest accepted system CVE inventory page.
pub const MAX_SYSTEM_CVE_INVENTORY_PAGE_SIZE: u16 = 500;
/// Gives the largest normalized inventory search in Unicode scalar values.
pub const MAX_SYSTEM_CVE_INVENTORY_SEARCH_CHARS: usize = 200;
const MAX_SYSTEM_CVE_INVENTORY_ROWS: u16 = 1_000;
const MAX_EXACT_SYSTEM_VULNERABILITIES: i64 = 1_000;
const MAX_FLEET_CVE_AFFECTED_SYSTEMS: usize = 1_000;

/// Reports that a fleet CVE detail exceeds its complete affected-system bound.
#[derive(Debug)]
pub struct CveInventoryOverflow;

impl std::fmt::Display for CveInventoryOverflow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "fleet CVE inventory exceeds the {MAX_FLEET_CVE_AFFECTED_SYSTEMS}-system limit"
        )
    }
}

impl std::error::Error for CveInventoryOverflow {}

/// Reports whether a fleet CVE detail exceeded its affected-system bound.
pub fn is_cve_inventory_overflow(error: &anyhow::Error) -> bool {
    error.downcast_ref::<CveInventoryOverflow>().is_some()
}

/// Reports that the complete legacy system inventory exceeds 1,000 rows.
#[derive(Debug)]
pub struct SystemCveInventoryOverflow;

impl std::fmt::Display for SystemCveInventoryOverflow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("system CVE inventory exceeds the 1000-row limit")
    }
}

impl std::error::Error for SystemCveInventoryOverflow {}

/// Reports whether a complete legacy system inventory exceeded 1,000 rows.
pub fn is_system_cve_inventory_overflow(error: &anyhow::Error) -> bool {
    error.downcast_ref::<SystemCveInventoryOverflow>().is_some()
}

fn reject_inventory_overflow<T>(rows: Vec<T>) -> Result<Vec<T>> {
    if rows.len() > MAX_FLEET_CVE_AFFECTED_SYSTEMS {
        return Err(CveInventoryOverflow.into());
    }
    Ok(rows)
}

/// Reports a client-visible inventory pagination failure.
#[derive(Debug, PartialEq, Eq)]
pub enum SystemCveInventoryPageError {
    /// Reports an invalid page bound or filter value.
    InvalidRequest(&'static str),
    /// Reports a malformed or unsupported cursor.
    InvalidCursor,
    /// Reports that the cursor no longer names this source and filter scope.
    InventoryChanged,
}

impl std::fmt::Display for SystemCveInventoryPageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => formatter.write_str(message),
            Self::InvalidCursor => formatter.write_str("invalid system CVE inventory cursor"),
            Self::InventoryChanged => formatter.write_str("system CVE inventory changed"),
        }
    }
}

impl std::error::Error for SystemCveInventoryPageError {}

/// Returns the typed pagination failure contained in an inventory query error.
pub fn system_cve_inventory_page_error(
    error: &anyhow::Error,
) -> Option<&SystemCveInventoryPageError> {
    error.downcast_ref::<SystemCveInventoryPageError>()
}

/// Contains one vulnerability selected from authoritative deployed scan
/// evidence.
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

/// Contains one system inventory selected from either exact or legacy
/// evidence.
#[derive(Debug)]
pub struct SystemCveInventoryQuery {
    /// Identifies the single selected inventory authority.
    pub authority: SystemCveInventoryAuthority,
    /// Reports the first failed exact-evidence prerequisite for a fallback.
    pub exact_authority_failure: Option<ExactCveAuthorityFailureReason>,
    /// Gives real provenance for the selected completed scan.
    pub source: Option<SystemCveInventorySource>,
    /// Contains rows from only the selected source.
    pub rows: Vec<ExactSystemVulnerabilityRow>,
    /// Gives complete totals over the active filter scope.
    pub metadata: SystemCveInventoryMetadata,
    /// Identifies the source plus dimensions that affect page membership,
    /// order, filters, or totals.
    pub inventory_revision: String,
    /// Reports whether another page exists after `rows`.
    pub has_more: bool,
    /// Continues after the last returned stable identity.
    pub next_cursor: Option<String>,
}

/// Contains validated and normalized inventory page inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemCveInventoryPageRequest {
    limit: u16,
    after: Option<String>,
    search: Option<String>,
    severities: Vec<String>,
    statuses: Vec<String>,
}

impl Default for SystemCveInventoryPageRequest {
    fn default() -> Self {
        Self {
            limit: DEFAULT_SYSTEM_CVE_INVENTORY_PAGE_SIZE,
            after: None,
            search: None,
            severities: Vec::new(),
            statuses: Vec::new(),
        }
    }
}

impl SystemCveInventoryPageRequest {
    /// Selects the complete legacy response up to its fixed compatibility bound.
    pub(crate) fn legacy_complete() -> Self {
        Self {
            limit: MAX_SYSTEM_CVE_INVENTORY_ROWS,
            ..Self::default()
        }
    }

    /// Validates and normalizes API query parameters.
    ///
    /// Search matching is case-insensitive. Repeated whitespace is collapsed,
    /// and case is lowered so equivalent searches have one cursor fingerprint.
    /// Severity and status values are comma-separated OR selections.
    ///
    /// # Errors
    ///
    /// Returns [`SystemCveInventoryPageError::InvalidRequest`] when `limit` is
    /// outside 1 through 500, search exceeds 200 Unicode scalar values, or a
    /// severity or status is outside its documented domain.
    pub fn from_params(params: SystemCveInventoryParams) -> Result<Self> {
        let limit = params
            .limit
            .unwrap_or(DEFAULT_SYSTEM_CVE_INVENTORY_PAGE_SIZE);
        if !(1..=MAX_SYSTEM_CVE_INVENTORY_PAGE_SIZE).contains(&limit) {
            return Err(SystemCveInventoryPageError::InvalidRequest(
                "limit must be between 1 and 500",
            )
            .into());
        }
        let search = params.q.and_then(|value| {
            let normalized = value
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            (!normalized.is_empty()).then_some(normalized)
        });
        if search
            .as_ref()
            .is_some_and(|value| value.chars().count() > MAX_SYSTEM_CVE_INVENTORY_SEARCH_CHARS)
        {
            return Err(SystemCveInventoryPageError::InvalidRequest(
                "q must be 200 characters or less after normalization",
            )
            .into());
        }
        let severities = normalize_inventory_set(
            params.severity,
            &["critical", "high", "medium", "low", "unknown"],
            "severity must contain only critical, high, medium, low, or unknown",
        )?;
        let statuses = normalize_inventory_set(
            params.status,
            &["open", "fix_available"],
            "status must contain only open or fix_available",
        )?;
        Ok(Self {
            limit,
            after: params.after,
            search,
            severities,
            statuses,
        })
    }

    fn filter_fingerprint(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(self.search.as_deref().unwrap_or_default());
        digest.update([0]);
        digest.update(self.severities.join(","));
        digest.update([0]);
        digest.update(self.statuses.join(","));
        hex::encode(digest.finalize())
    }

    fn search_pattern(&self) -> Option<String> {
        self.search.as_ref().map(|value| {
            let escaped = value
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            format!("%{escaped}%")
        })
    }
}

fn normalize_inventory_set(
    value: Option<String>,
    allowed: &[&str],
    error_message: &'static str,
) -> Result<Vec<String>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .split(',')
        .map(|item| item.trim().to_lowercase())
        .collect::<BTreeSet<_>>();
    if values.is_empty()
        || values
            .iter()
            .any(|item| item.is_empty() || !allowed.contains(&item.as_str()))
    {
        return Err(SystemCveInventoryPageError::InvalidRequest(error_message).into());
    }
    Ok(values.into_iter().collect())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemCveInventoryCursor {
    version: u8,
    system_id: Uuid,
    authority: SystemCveInventoryAuthority,
    source_scan_id: Uuid,
    filter_fingerprint: String,
    inventory_revision: String,
    canonical_cve_id: String,
    canonical_package_name: String,
}

#[derive(Debug, sqlx::FromRow)]
struct SystemCveInventoryMetadataRow {
    total_findings: i64,
    total_cves: i64,
    total_packages: i64,
    critical: i64,
    high: i64,
    medium: i64,
    low: i64,
    unknown: i64,
    revision_seed: String,
}

#[derive(Debug, sqlx::FromRow)]
struct InventoryAuthorityRow {
    failure_reason: Option<String>,
    exact_scan_id: Option<Uuid>,
    exact_completed_at: Option<DateTime<Utc>>,
    exact_scanner_name: Option<String>,
    exact_scanner_version: Option<String>,
}

fn parse_exact_authority_failure(value: &str) -> Result<ExactCveAuthorityFailureReason> {
    match value {
        "missing_current_generation" => {
            Ok(ExactCveAuthorityFailureReason::MissingCurrentGeneration)
        }
        "current_store_mismatch" => Ok(ExactCveAuthorityFailureReason::CurrentStoreMismatch),
        "retained_generation_unavailable" => {
            Ok(ExactCveAuthorityFailureReason::RetainedGenerationUnavailable)
        }
        "retained_store_mismatch" => Ok(ExactCveAuthorityFailureReason::RetainedStoreMismatch),
        "lineage_unverified" => Ok(ExactCveAuthorityFailureReason::LineageUnverified),
        "snapshot_unavailable" => Ok(ExactCveAuthorityFailureReason::SnapshotUnavailable),
        "snapshot_unsupported" => Ok(ExactCveAuthorityFailureReason::SnapshotUnsupported),
        "exact_derivation_unavailable" => {
            Ok(ExactCveAuthorityFailureReason::ExactDerivationUnavailable)
        }
        "no_schema1_current_scan" => Ok(ExactCveAuthorityFailureReason::NoSchema1CurrentScan),
        _ => Err(anyhow::anyhow!(
            "exact CVE authority returned unknown failure reason {value}"
        )),
    }
}

/// Fetches one read-only CVE inventory source for a system.
///
/// The function determines exact authority before it reads findings. Exact
/// authority therefore wins for both vulnerable and clean scans. If exact
/// authority is unavailable, the function uses the latest completed scan under
/// the bounded `view_system_vulnerabilities` selection semantics. It never
/// unions sources or infers immutable observations from legacy data.
///
/// All authority and row reads use one repeatable-read, read-only transaction.
/// Legacy rows have no exact observation identity and cannot authorize a
/// remediation mutation.
///
/// # Errors
///
/// Returns a database error if the consistent snapshot, authority, scan
/// provenance, or bounded finding rows cannot be loaded. Returns
/// [`SystemCveInventoryOverflow`] when the complete compatibility response
/// contains more than 1,000 stable rows.
pub async fn fetch_system_cve_inventory(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<SystemCveInventoryQuery> {
    let inventory = fetch_system_cve_inventory_page(
        pool,
        system_id,
        &SystemCveInventoryPageRequest::legacy_complete(),
    )
    .await?;
    if inventory.has_more {
        return Err(SystemCveInventoryOverflow.into());
    }
    Ok(inventory)
}

/// Fetches one bounded inventory page without applying user visibility.
///
/// Authority selection, cursor validation, totals, and page rows share one
/// read-only repeatable-read transaction. Callers that serve an authenticated
/// request must use `fetch_authorized_system_cve_inventory_tx` instead.
///
/// # Errors
///
/// Returns a database error, malformed cursor error, or inventory-changed
/// conflict when the cursor binding no longer matches the selected source.
pub async fn fetch_system_cve_inventory_page(
    pool: &PgPool,
    system_id: Uuid,
    page: &SystemCveInventoryPageRequest,
) -> Result<SystemCveInventoryQuery> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let inventory = fetch_system_cve_inventory_tx(&mut transaction, system_id, page).await?;
    transaction.commit().await?;
    Ok(inventory)
}

/// Fetches an inventory when the user can see the system in the same snapshot.
///
/// An active Viewer or Operator must have a current membership in the system's
/// environment. An active Admin can also read an unassigned system. The
/// visibility decision and evidence selection share one read-only,
/// repeatable-read transaction.
///
/// # Errors
///
/// Returns a database or pagination error. Returns `Ok(None)` when the system
/// is absent or hidden from the user.
pub async fn fetch_authorized_system_cve_inventory(
    pool: &PgPool,
    system_id: Uuid,
    user_id: Uuid,
) -> Result<Option<SystemCveInventoryQuery>> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let inventory = fetch_authorized_system_cve_inventory_tx(
        &mut transaction,
        system_id,
        user_id,
        &SystemCveInventoryPageRequest::legacy_complete(),
    )
    .await?;
    transaction.commit().await?;
    match inventory {
        Some(inventory) if inventory.has_more => Err(SystemCveInventoryOverflow.into()),
        inventory => Ok(inventory),
    }
}

/// Fetches authorized inventory inside the caller's consistent snapshot.
///
/// # Errors
///
/// Returns a database or pagination error. Returns `Ok(None)` when the system
/// is absent or hidden from the user. Visibility is decided before cursor
/// parsing, which preserves hidden-as-absent non-disclosure.
pub(crate) async fn fetch_authorized_system_cve_inventory_tx(
    transaction: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    user_id: Uuid,
    page: &SystemCveInventoryPageRequest,
) -> Result<Option<SystemCveInventoryQuery>> {
    let visible = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(
             SELECT 1 FROM systems system
             JOIN users actor ON actor.id=$2 AND actor.is_active
             WHERE system.id=$1
               AND EXISTS(
                 SELECT 1 FROM user_role_assignments assignment
                 WHERE assignment.user_id=actor.id
                   AND assignment.role IN ('viewer','operator','admin'))
               AND (
                 EXISTS(SELECT 1 FROM user_role_assignments assignment
                        WHERE assignment.user_id=actor.id AND assignment.role='admin')
                 OR EXISTS(SELECT 1 FROM user_environment_memberships membership
                           WHERE membership.user_id=actor.id
                             AND membership.environment_id=system.environment_id)))"#,
    )
    .bind(system_id)
    .bind(user_id)
    .fetch_one(&mut **transaction)
    .await?;
    if !visible {
        return Ok(None);
    }
    // SECURITY: Cursor parsing occurs only after this visibility decision. A
    // hidden or absent system therefore returns the same result for every
    // cursor value and does not disclose source identity or cursor validity.
    let inventory = fetch_system_cve_inventory_tx(transaction, system_id, page).await?;
    Ok(Some(inventory))
}

async fn fetch_system_cve_inventory_tx(
    transaction: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    page: &SystemCveInventoryPageRequest,
) -> Result<SystemCveInventoryQuery> {
    // SECURITY: This prerequisite order mirrors exact-CVE authority without
    // changing any writer predicate. The latest state is selected first, so an
    // older matching state cannot authorize or describe the current deployment.
    let authority = sqlx::query_as::<_, InventoryAuthorityRow>(
        r#"WITH latest_state AS (
             SELECT state.store_path,state.generation,
                    state.generation_matches_current_store_path
             FROM systems system
             LEFT JOIN LATERAL (
               SELECT candidate.store_path,candidate.generation,
                      candidate.generation_matches_current_store_path
               FROM system_states candidate
               WHERE candidate.hostname=system.hostname
               ORDER BY candidate.timestamp DESC,candidate.id DESC LIMIT 1
             ) state ON true
             WHERE system.id=$1
           )
           SELECT CASE
                    WHEN state.generation IS NULL OR state.store_path IS NULL
                      OR btrim(state.store_path)='' THEN 'missing_current_generation'
                    WHEN state.generation_matches_current_store_path IS NOT TRUE
                      THEN 'current_store_mismatch'
                    WHEN retained.id IS NULL THEN 'retained_generation_unavailable'
                    WHEN retained.source_store_path<>state.store_path
                      THEN 'retained_store_mismatch'
                    WHEN retained.lineage_verified IS NOT TRUE THEN 'lineage_unverified'
                    WHEN artifact.id IS NULL OR artifact.commit_id<>retained.commit_id
                      OR artifact.configuration_name<>retained.configuration_name
                      OR artifact.lifecycle<>'available' THEN 'snapshot_unavailable'
                    WHEN artifact.integrity_version<>1 THEN 'snapshot_unsupported'
                    WHEN derivation.id IS NULL OR derivation.commit_id<>retained.commit_id
                      OR derivation.derivation_name<>retained.configuration_name
                      OR derivation.derivation_type<>'nixos'
                      OR COALESCE(derivation.store_path,derivation.expected_store_path)
                         <>retained.source_store_path THEN 'exact_derivation_unavailable'
                    WHEN scan.id IS NULL THEN 'no_schema1_current_scan'
                    ELSE NULL
                  END AS failure_reason,
                  scan.id AS exact_scan_id,scan.completed_at AS exact_completed_at,
                  scan.scanner_name AS exact_scanner_name,
                  scan.scanner_version AS exact_scanner_version
           FROM latest_state state
           LEFT JOIN evaluation_generation_snapshots retained
             ON retained.system_id=$1 AND retained.generation=state.generation
           LEFT JOIN evaluation_snapshots artifact ON artifact.id=retained.snapshot_id
           LEFT JOIN derivations derivation ON derivation.id=retained.derivation_id
           LEFT JOIN LATERAL (
             SELECT candidate.id,candidate.completed_at,candidate.scanner_name,
                    candidate.scanner_version
             FROM cve_scans candidate
             WHERE candidate.derivation_id=derivation.id
               AND candidate.status='completed'
               AND candidate.completed_at IS NOT NULL
               AND candidate.evidence_schema_version=1
             ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
           ) scan ON true"#,
    )
    .bind(system_id)
    .fetch_one(&mut **transaction)
    .await?;

    if authority.failure_reason.is_none() {
        let scan_id = authority
            .exact_scan_id
            .ok_or_else(|| anyhow::anyhow!("exact CVE authority omitted its scan identity"))?;
        let source = SystemCveInventorySource {
            scan_id,
            scanner_name: authority
                .exact_scanner_name
                .ok_or_else(|| anyhow::anyhow!("exact CVE authority omitted its scanner name"))?,
            scanner_version: authority.exact_scanner_version,
            completed_at: authority.exact_completed_at.ok_or_else(|| {
                anyhow::anyhow!("exact CVE authority omitted its completion time")
            })?,
        };
        return fetch_inventory_page_for_source(
            transaction,
            system_id,
            SystemCveInventoryAuthority::Exact,
            None,
            source,
            page,
        )
        .await;
    }

    let failure = authority
        .failure_reason
        .as_deref()
        .map(parse_exact_authority_failure)
        .transpose()?;
    // COMPATIBILITY: The predicates and ordering mirror migration 0177's
    // bounded view. Rows are then loaded by this scan ID so provenance cannot
    // diverge from findings if the view definition changes.
    let legacy_source = sqlx::query_as::<_, (Uuid, DateTime<Utc>, String, Option<String>)>(
        r#"SELECT scan.id,scan.completed_at,scan.scanner_name,scan.scanner_version
           FROM systems system
           JOIN derivations derivation ON derivation.derivation_name=system.hostname
              AND derivation.derivation_type='nixos'
            JOIN derivation_statuses status ON status.id=derivation.status_id
              AND status.name=ANY(ARRAY['build-complete','complete'])
            JOIN commits commit ON commit.id=derivation.commit_id
            JOIN flakes flake ON flake.id=commit.flake_id
            JOIN cve_scans scan ON scan.derivation_id=derivation.id
             AND scan.status='completed' AND scan.completed_at IS NOT NULL
           WHERE system.id=$1
           ORDER BY scan.completed_at DESC,scan.id DESC LIMIT 1"#,
    )
    .bind(system_id)
    .fetch_optional(&mut **transaction)
    .await?;

    let Some((scan_id, completed_at, scanner_name, scanner_version)) = legacy_source else {
        if let Some(cursor) = page.after.as_deref() {
            decode_system_cve_inventory_cursor(cursor)?;
            return Err(SystemCveInventoryPageError::InventoryChanged.into());
        }
        return Ok(SystemCveInventoryQuery {
            authority: SystemCveInventoryAuthority::NoScan,
            exact_authority_failure: failure,
            source: None,
            rows: Vec::new(),
            metadata: SystemCveInventoryMetadata::default(),
            inventory_revision: inventory_revision(&format!("{}|no_scan|{:?}", system_id, failure)),
            has_more: false,
            next_cursor: None,
        });
    };
    fetch_inventory_page_for_source(
        transaction,
        system_id,
        SystemCveInventoryAuthority::Legacy,
        failure,
        SystemCveInventorySource {
            scan_id,
            scanner_name,
            scanner_version,
            completed_at,
        },
        page,
    )
    .await
}

const EXACT_INVENTORY_CTE: &str = r#"WITH selected_observations AS (
             SELECT DISTINCT ON (observation.canonical_cve_id COLLATE "C",
                                   observation.canonical_package_name COLLATE "C")
                    observation.canonical_cve_id,observation.canonical_package_name,
                    observation.observed_package_name,
                    observation.observed_package_version,
                    observation.observed_derivation_path
             FROM cve_scan_vulnerability_observations observation
              WHERE observation.scan_id=$1 AND NOT observation.is_whitelisted
              ORDER BY observation.canonical_cve_id COLLATE "C",
                       observation.canonical_package_name COLLATE "C",
                       observation.observed_derivation_path COLLATE "C"
            ), inventory AS (
            SELECT $1::uuid AS scan_id,
                  observation.observed_derivation_path AS occurrence_derivation_path,
                  observation.canonical_cve_id AS cve_id,
                  observation.canonical_package_name,
                   observation.observed_package_name AS package_name,
                    observation.observed_package_version AS installed_version,
                    lower(severity_from_cvss(cve.cvss_v3_score)) AS severity,
                    cve.cvss_v3_score::double precision AS cvss_score,
                    scan.completed_at AS first_seen,
                    CASE WHEN EXISTS (
                      SELECT 1
                      FROM derivations package_derivation
                      JOIN package_vulnerabilities vulnerability
                        ON vulnerability.derivation_id=package_derivation.id
                       AND vulnerability.cve_id=observation.canonical_cve_id
                       AND vulnerability.fixed_version IS NOT NULL
                      WHERE package_derivation.derivation_path=
                            observation.observed_derivation_path
                    ) THEN 'fix_available' ELSE 'open' END AS status
           FROM selected_observations observation
           JOIN cve_scans scan ON scan.id=$1
           JOIN cves cve ON cve.id=observation.canonical_cve_id
            ), filtered AS (
              SELECT * FROM inventory
              WHERE ($3::text IS NULL OR cve_id ILIKE $3 ESCAPE '\'
                     OR canonical_package_name ILIKE $3 ESCAPE '\'
                     OR package_name ILIKE $3 ESCAPE '\'
                     OR installed_version ILIKE $3 ESCAPE '\')
                AND (cardinality($4::text[])=0 OR severity=ANY($4::text[]))
                AND (cardinality($5::text[])=0 OR status=ANY($5::text[]))
            )"#;

const LEGACY_INVENTORY_CTE: &str = r#"WITH selected_findings AS (
              SELECT DISTINCT ON (
                       cve.id COLLATE "C",
                       COALESCE(package_derivation.pname,package_derivation.derivation_name) COLLATE "C")
                    scan.id AS scan_id,
                   package_derivation.derivation_path AS occurrence_derivation_path,
                  cve.id AS cve_id,
                  COALESCE(package_derivation.pname,package_derivation.derivation_name)
                    AS canonical_package_name,
                  package_derivation.derivation_name AS package_name,
                   COALESCE(package_derivation.version,'') AS installed_version,
                    lower(severity_from_cvss(cve.cvss_v3_score)) AS severity,
                    cve.cvss_v3_score::double precision AS cvss_score,
                    scan.completed_at AS first_seen,
                   CASE WHEN vulnerability.fixed_version IS NULL THEN 'open'
                        ELSE 'fix_available' END AS status
            FROM systems system
           JOIN derivations derivation ON derivation.derivation_name=system.hostname
             AND derivation.derivation_type='nixos'
            JOIN cve_scans scan ON scan.id=$1 AND scan.derivation_id=derivation.id
           JOIN scan_packages scan_package ON scan_package.scan_id=scan.id
           JOIN derivations package_derivation
             ON package_derivation.id=scan_package.derivation_id
             AND package_derivation.derivation_type='package'
           JOIN package_vulnerabilities vulnerability
             ON vulnerability.derivation_id=package_derivation.id
             AND NOT vulnerability.is_whitelisted
           JOIN cves cve ON cve.id=vulnerability.cve_id
             WHERE system.id=$2
             ORDER BY cve.id COLLATE "C",
                      COALESCE(package_derivation.pname,package_derivation.derivation_name) COLLATE "C",
                      package_derivation.derivation_path COLLATE "C"
            ), inventory AS (
              SELECT * FROM selected_findings
            ), filtered AS (
              SELECT * FROM inventory
              WHERE ($3::text IS NULL OR cve_id ILIKE $3 ESCAPE '\'
                     OR canonical_package_name ILIKE $3 ESCAPE '\'
                     OR package_name ILIKE $3 ESCAPE '\'
                     OR installed_version ILIKE $3 ESCAPE '\')
                AND (cardinality($4::text[])=0 OR severity=ANY($4::text[]))
                AND (cardinality($5::text[])=0 OR status=ANY($5::text[]))
            )"#;

async fn fetch_inventory_page_for_source(
    transaction: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    authority: SystemCveInventoryAuthority,
    exact_authority_failure: Option<ExactCveAuthorityFailureReason>,
    source: SystemCveInventorySource,
    page: &SystemCveInventoryPageRequest,
) -> Result<SystemCveInventoryQuery> {
    let fingerprint = page.filter_fingerprint();
    let cursor = page
        .after
        .as_deref()
        .map(decode_system_cve_inventory_cursor)
        .transpose()?;
    if cursor.as_ref().is_some_and(|cursor| {
        cursor.system_id != system_id
            || cursor.authority != authority
            || cursor.source_scan_id != source.scan_id
            || cursor.filter_fingerprint != fingerprint
    }) {
        return Err(SystemCveInventoryPageError::InventoryChanged.into());
    }
    let after_cve = cursor
        .as_ref()
        .map(|cursor| cursor.canonical_cve_id.as_str());
    let after_package = cursor
        .as_ref()
        .map(|cursor| cursor.canonical_package_name.as_str());
    let search_pattern = page.search_pattern();
    let cte = match authority {
        SystemCveInventoryAuthority::Exact => EXACT_INVENTORY_CTE,
        SystemCveInventoryAuthority::Legacy => LEGACY_INVENTORY_CTE,
        SystemCveInventoryAuthority::NoScan => {
            return Err(anyhow::anyhow!("no-scan inventory cannot have a source"));
        }
    };
    let metadata_sql = inventory_metadata_sql(cte);
    let metadata = sqlx::query_as::<_, SystemCveInventoryMetadataRow>(&metadata_sql)
        .bind(source.scan_id)
        .bind(system_id)
        .bind(search_pattern.as_deref())
        .bind(&page.severities)
        .bind(&page.statuses)
        .bind(match authority {
            SystemCveInventoryAuthority::Exact => "exact",
            SystemCveInventoryAuthority::Legacy => "legacy",
            SystemCveInventoryAuthority::NoScan => "no_scan",
        })
        .fetch_one(&mut **transaction)
        .await?;
    let inventory_revision = inventory_revision(&metadata.revision_seed);
    if cursor
        .as_ref()
        .is_some_and(|cursor| cursor.inventory_revision != inventory_revision)
    {
        return Err(SystemCveInventoryPageError::InventoryChanged.into());
    }
    let page_sql = inventory_page_sql(cte);
    let mut rows = sqlx::query_as::<_, ExactSystemVulnerabilityRow>(&page_sql)
        .bind(source.scan_id)
        .bind(system_id)
        .bind(search_pattern.as_deref())
        .bind(&page.severities)
        .bind(&page.statuses)
        .bind(after_cve)
        .bind(after_package)
        .bind(i64::from(page.limit) + 1)
        .fetch_all(&mut **transaction)
        .await?;
    let has_more = rows.len() > usize::from(page.limit);
    if has_more {
        rows.pop();
    }
    let next_cursor = if has_more {
        rows.last()
            .map(|row| {
                encode_system_cve_inventory_cursor(&SystemCveInventoryCursor {
                    version: 1,
                    system_id,
                    authority,
                    source_scan_id: source.scan_id,
                    filter_fingerprint: fingerprint,
                    inventory_revision: inventory_revision.clone(),
                    canonical_cve_id: row.cve_id.clone(),
                    canonical_package_name: row.canonical_package_name.clone(),
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(SystemCveInventoryQuery {
        authority,
        exact_authority_failure,
        source: Some(source),
        rows,
        metadata: SystemCveInventoryMetadata {
            total_findings: metadata.total_findings,
            total_cves: metadata.total_cves,
            total_packages: metadata.total_packages,
            severity: SystemCveInventorySeverityCounts {
                critical: metadata.critical,
                high: metadata.high,
                medium: metadata.medium,
                low: metadata.low,
                unknown: metadata.unknown,
            },
        },
        inventory_revision,
        has_more,
        next_cursor,
    })
}

fn inventory_metadata_sql(cte: &str) -> String {
    format!(
        r#"{cte}
           SELECT COUNT(*)::bigint AS total_findings,
                  COUNT(DISTINCT cve_id)::bigint AS total_cves,
                  COUNT(DISTINCT canonical_package_name)::bigint AS total_packages,
                  COUNT(*) FILTER (WHERE severity='critical')::bigint AS critical,
                  COUNT(*) FILTER (WHERE severity='high')::bigint AS high,
                  COUNT(*) FILTER (WHERE severity='medium')::bigint AS medium,
                  COUNT(*) FILTER (WHERE severity='low')::bigint AS low,
                  COUNT(*) FILTER (WHERE severity='unknown')::bigint AS unknown,
                  concat_ws('|',$1::text,$2::text,$6::text,
                    (SELECT COUNT(*)::text FROM inventory),
                    (SELECT COALESCE(SUM(hashtextextended(
                       jsonb_build_array(
                         cve_id,canonical_package_name,package_name,
                         installed_version,severity,status)::text,440))::text,'0')
                     FROM inventory)
                  ) AS revision_seed
           FROM filtered"#
    )
}

fn inventory_page_sql(cte: &str) -> String {
    format!(
        r#"{cte}
           , selected_page AS (
             SELECT * FROM filtered
             WHERE ($6::text IS NULL OR
                    (cve_id COLLATE "C",canonical_package_name COLLATE "C") >
                    ($6::text COLLATE "C",$7::text COLLATE "C"))
             ORDER BY cve_id COLLATE "C",canonical_package_name COLLATE "C"
             LIMIT $8
           )
           SELECT filtered.scan_id,filtered.occurrence_derivation_path,
                  filtered.cve_id,filtered.canonical_package_name,
                  filtered.package_name,filtered.installed_version,
                  filtered.severity,filtered.cvss_score,
                  COALESCE(cve.description,'') AS description,
                   package_metadata.fixed_version,filtered.first_seen,
                  cve.published_date::timestamptz AS published_at,
                  filtered.status,
                  justification.category AS justification_category,
                  justification.reason AS justification_reason,
                  justification.updated_at AS justification_updated_at
           FROM selected_page filtered
           JOIN cves cve ON cve.id=filtered.cve_id
           LEFT JOIN LATERAL (
             SELECT vulnerability.fixed_version
             FROM derivations package_derivation
             JOIN package_vulnerabilities vulnerability
               ON vulnerability.derivation_id=package_derivation.id
              AND vulnerability.cve_id=filtered.cve_id
             WHERE package_derivation.derivation_path=
                   filtered.occurrence_derivation_path
             ORDER BY package_derivation.id DESC LIMIT 1
           ) package_metadata ON true
           LEFT JOIN LATERAL (
             SELECT candidate.category,candidate.reason,candidate.updated_at
             FROM system_cve_justifications candidate
             WHERE candidate.cve_id=filtered.cve_id
               AND (candidate.system_id=$2 OR candidate.system_id IS NULL)
             ORDER BY (candidate.system_id IS NOT NULL) DESC,
                      candidate.updated_at DESC LIMIT 1
           ) justification ON true
           ORDER BY filtered.cve_id COLLATE "C",
                    filtered.canonical_package_name COLLATE "C"
           LIMIT $8"#
    )
}

fn decode_system_cve_inventory_cursor(value: &str) -> Result<SystemCveInventoryCursor> {
    // SECURITY: The cursor is an unsigned position hint, not authority. A
    // caller can only skip rows in an inventory that visibility checks already
    // authorized; source, filter, and revision bindings are verified below.
    if value.is_empty() || value.len() > 8_192 {
        return Err(SystemCveInventoryPageError::InvalidCursor.into());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| SystemCveInventoryPageError::InvalidCursor)?;
    let cursor: SystemCveInventoryCursor =
        serde_json::from_slice(&bytes).map_err(|_| SystemCveInventoryPageError::InvalidCursor)?;
    if cursor.version != 1
        || cursor.filter_fingerprint.len() != 64
        || cursor.inventory_revision.len() != 64
        || cursor.canonical_cve_id.is_empty()
        || cursor.canonical_cve_id.len() > 20
        || cursor.canonical_package_name.is_empty()
        || cursor.canonical_package_name.len() > 4_096
    {
        return Err(SystemCveInventoryPageError::InvalidCursor.into());
    }
    Ok(cursor)
}

fn encode_system_cve_inventory_cursor(cursor: &SystemCveInventoryCursor) -> Result<String> {
    let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(cursor)?);
    if encoded.len() > 8_192 {
        return Err(anyhow::anyhow!(
            "system CVE inventory cursor exceeds its bound"
        ));
    }
    Ok(encoded)
}

fn inventory_revision(seed: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    hex::encode(digest.finalize())
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
                 SELECT state.store_path,state.generation,
                        state.generation_matches_current_store_path
                FROM system_states state
                WHERE state.hostname=system.hostname
                ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
              ) deployed ON deployed.store_path IS NOT NULL
                AND deployed.generation IS NOT NULL
                AND deployed.generation_matches_current_store_path IS TRUE
                AND btrim(deployed.store_path)<>''
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
        exact_systems_affected: 0,
        legacy_systems_affected: 0,
        no_scan_systems: 0,
        outstanding: 0,
        accepted: 0,
        scheduled: 0,
    }
}

// SECURITY: This read model annotates a bounded legacy-view row as exact only
// when the latest deployed state resolves to the immutable scan that supplied
// the same system and canonical CVE/package identity. Mutation code does not
// use this CTE.
const FLEET_INVENTORY_LIST_CTE: &str = r#"
WITH exact_authority_systems AS (
  SELECT system.id AS system_id,scan.scan_id
  FROM systems system
  JOIN LATERAL (
    SELECT state.store_path,state.generation,state.generation_matches_current_store_path
    FROM system_states state WHERE state.hostname=system.hostname
    ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
  ) current ON current.store_path IS NOT NULL AND current.generation IS NOT NULL
    AND current.generation_matches_current_store_path IS TRUE
    AND btrim(current.store_path)<>''
  JOIN evaluation_generation_snapshots retained
    ON retained.system_id=system.id AND retained.generation=current.generation
   AND retained.source_store_path=current.store_path AND retained.lineage_verified
  JOIN evaluation_snapshots artifact ON artifact.id=retained.snapshot_id
   AND artifact.commit_id=retained.commit_id
   AND artifact.configuration_name=retained.configuration_name
   AND artifact.lifecycle='available' AND artifact.integrity_version=1
  JOIN derivations derivation ON derivation.id=retained.derivation_id
   AND derivation.commit_id=retained.commit_id
   AND derivation.derivation_name=retained.configuration_name
   AND derivation.derivation_type='nixos'
   AND COALESCE(derivation.store_path,derivation.expected_store_path)
       =retained.source_store_path
  JOIN LATERAL (
    SELECT scan.id AS scan_id FROM cve_scans scan WHERE scan.derivation_id=derivation.id
      AND scan.status='completed' AND scan.completed_at IS NOT NULL
      AND scan.evidence_schema_version=1
    ORDER BY scan.completed_at DESC,scan.id DESC LIMIT 1
  ) scan ON true
  WHERE system.is_active
    AND ($1::uuid[] IS NULL OR system.environment_id=ANY($1))
), exact_subjects AS (
  SELECT DISTINCT ON (occurrence.system_id,occurrence.cve_id,occurrence.package_name)
         occurrence.system_id,occurrence.environment_id,occurrence.environment_name,
         occurrence.cve_id,occurrence.package_name,occurrence.installed_version,
         package_metadata.fixed_version,occurrence.completed_at,'exact'::text AS authority
   FROM view_current_exact_cve_occurrences occurrence
   JOIN exact_authority_systems exact ON exact.system_id=occurrence.system_id
     AND exact.scan_id=occurrence.scan_id
  LEFT JOIN LATERAL (
    SELECT vulnerability.fixed_version
    FROM derivations package_derivation
    JOIN package_vulnerabilities vulnerability
      ON vulnerability.derivation_id=package_derivation.id
     AND vulnerability.cve_id=occurrence.cve_id
    WHERE package_derivation.derivation_path=occurrence.observed_derivation_path
    ORDER BY package_derivation.id DESC LIMIT 1
  ) package_metadata ON true
  WHERE $1::uuid[] IS NULL OR occurrence.environment_id=ANY($1)
  ORDER BY occurrence.system_id,occurrence.cve_id,occurrence.package_name,
           occurrence.observed_derivation_path
), legacy_subjects AS (
  SELECT DISTINCT ON (
           system.id,view.cve_id,COALESCE(view.package_pname,view.package_name))
         system.id AS system_id,system.environment_id,environment.name AS environment_name,
         view.cve_id,COALESCE(view.package_pname,view.package_name) AS package_name,
         COALESCE(view.package_version,'') AS installed_version,view.fixed_version,
         view.completed_at,
         'legacy'::text AS authority
  FROM view_system_vulnerabilities view
  JOIN systems system ON system.hostname=view.hostname AND system.is_active
  LEFT JOIN environments environment ON environment.id=system.environment_id
  WHERE ($1::uuid[] IS NULL OR system.environment_id=ANY($1))
    AND NOT EXISTS(
      SELECT 1 FROM exact_authority_systems exact
      WHERE exact.system_id=system.id)
  ORDER BY system.id,view.cve_id,COALESCE(view.package_pname,view.package_name),
           view.derivation_path
), inventory_subjects AS (
  SELECT * FROM exact_subjects
  UNION ALL
  SELECT * FROM legacy_subjects
), inventory_list AS (
  SELECT subject.cve_id,cve.cvss_v3_score,
         severity_from_cvss(cve.cvss_v3_score) AS severity,
         COALESCE(NULLIF(btrim(cve.description),''),cve.id) AS title,
         cve.vector AS cvss_vector,cve.published_date,cve.exploited,
         subject.package_name,max(subject.installed_version) AS installed_version,
         max(subject.fixed_version) AS fixed_version,
         CASE WHEN max(subject.fixed_version) IS NULL THEN 'open'
              ELSE 'fix_available' END AS fix_status,
         count(DISTINCT subject.system_id)::bigint AS affected_count,
         count(DISTINCT subject.system_id) FILTER (WHERE subject.authority='exact')::bigint
           AS exact_affected_count,
         count(DISTINCT subject.system_id) FILTER (WHERE subject.authority='legacy')::bigint
           AS legacy_affected_count,
         array_agg(DISTINCT subject.environment_name ORDER BY subject.environment_name)
           FILTER (WHERE subject.environment_name IS NOT NULL) AS affected_environments,
         min(subject.completed_at) AS first_seen,max(subject.completed_at) AS last_seen,
         COALESCE(EXTRACT(EPOCH FROM (now()-cve.published_date))/86400,0)::integer
           AS age_days,
         CASE WHEN bool_or(subject.authority='legacy') THEN 'inventory_only'
              ELSE COALESCE(max(exact_list.triage_status),'outstanding') END AS triage_status
  FROM inventory_subjects subject
  JOIN cves cve ON cve.id=subject.cve_id
  LEFT JOIN cve_list_for_environment_scope($1) exact_list
    ON exact_list.cve_id=subject.cve_id
   AND exact_list.package_name=subject.package_name
  GROUP BY subject.cve_id,cve.id,cve.cvss_v3_score,cve.description,cve.vector,
           cve.published_date,cve.exploited,subject.package_name
)
"#;

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
/// This query applies the same bounded inventory authority, caller scope,
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

    let sql = format!(
        "{FLEET_INVENTORY_LIST_CTE}{}",
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
            exact_affected_count,
            legacy_affected_count,
            affected_environments,
            first_seen,
            last_seen,
            COALESCE(age_days, 0)::int AS age_days,
            LOWER(COALESCE(triage_status, 'outstanding')) AS triage_status
        FROM inventory_list
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
        "#
    );
    let rows = sqlx::query_as::<_, CveListItem>(&sql)
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
    let package_sql = format!(
        "{FLEET_INVENTORY_LIST_CTE}{}",
        r#", filtered AS (
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
            FROM inventory_list
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
                COUNT(DISTINCT subject.environment_id)::bigint as environments_count,
                COUNT(DISTINCT subject.system_id)::bigint as total_affected_systems
            FROM filtered f
            JOIN inventory_subjects subject
              ON subject.cve_id=f.cve_id
             AND subject.package_name=f.package_name
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
        "#
    );
    let package_stats = sqlx::query_as::<_, CvePackageStatsRow>(&package_sql)
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
    let nested_sql = format!(
        "{FLEET_INVENTORY_LIST_CTE}{}",
        r#", filtered AS (
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
                exact_affected_count,
                legacy_affected_count,
                affected_environments,
                first_seen,
                last_seen,
                COALESCE(age_days, 0)::int AS age_days,
                LOWER(COALESCE(triage_status, 'outstanding')) AS triage_status
            FROM inventory_list
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
            affected_count,exact_affected_count,legacy_affected_count,
            affected_environments,first_seen,last_seen,age_days,
            triage_status
        FROM ranked
        WHERE rn <= 100
        ORDER BY package_name ASC, rn ASC
        "#
    );
    let nested_rows = sqlx::query_as::<_, CveListItem>(&nested_sql)
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
    let sql = format!(
        "{FLEET_INVENTORY_LIST_CTE}{}",
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
        FROM inventory_list v
        LEFT JOIN cves c ON c.id = v.cve_id
        WHERE v.cve_id = $2
        ORDER BY v.package_name ASC
        LIMIT 1
        "#
    );
    let detail = sqlx::query_as::<_, CveDetail>(&sql)
        .bind(scope.environment_ids())
        .bind(cve_id)
        .fetch_one(pool)
        .await?;

    Ok(detail)
}

#[derive(sqlx::FromRow)]
struct FleetAffectedSystemRow {
    system_id: Uuid,
    hostname: String,
    environment_id: Option<Uuid>,
    environment: Option<String>,
    primary_ip_address: Option<String>,
    flake_name: Option<String>,
    flake_id: Option<i32>,
    commit_hash: Option<String>,
    deployment_policy: String,
    current_package_version: Option<String>,
    inventory_authority: String,
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
/// Returns a database error when the bounded inventory cannot be loaded.
pub async fn fetch_cve_affected_systems(
    pool: &PgPool,
    scope: &CveReadScope,
    cve_id: &str,
) -> Result<Vec<CveAffectedSystemDetail>> {
    fetch_cve_inventory_systems(pool, scope, cve_id, None).await
}

/// Fetches visible fleet inventory systems for one CVE and optional package.
///
/// Exact rows are annotated from immutable current occurrences. All other rows
/// come from the bounded legacy inventory view and are display-only.
///
/// # Errors
///
/// Returns a database error when the bounded inventory cannot be loaded.
pub async fn fetch_cve_inventory_systems(
    pool: &PgPool,
    scope: &CveReadScope,
    cve_id: &str,
    package_name: Option<&str>,
) -> Result<Vec<CveAffectedSystemDetail>> {
    let sql = format!(
        "{FLEET_INVENTORY_LIST_CTE}{}",
        r#"
        SELECT
            subject.system_id,system.hostname,system.environment_id,
            environment.name AS environment,state.primary_ip_address,
            flake.name AS flake_name,flake.id AS flake_id,NULL::text AS commit_hash,
            system.deployment_policy,
            subject.installed_version AS current_package_version,
            subject.authority AS inventory_authority
        FROM inventory_subjects subject
        JOIN systems system ON system.id=subject.system_id
        LEFT JOIN environments environment ON environment.id=system.environment_id
        LEFT JOIN flakes flake ON flake.id=system.flake_id
        LEFT JOIN LATERAL (
          SELECT candidate.primary_ip_address FROM system_states candidate
          WHERE candidate.hostname=system.hostname
          ORDER BY candidate.timestamp DESC,candidate.id DESC LIMIT 1
        ) state ON true
        WHERE subject.cve_id=$2 AND ($3::text IS NULL OR subject.package_name=$3)
        ORDER BY environment.name NULLS LAST,system.hostname
        LIMIT $4
        "#
    );
    let systems = sqlx::query_as::<_, FleetAffectedSystemRow>(&sql)
        .bind(scope.environment_ids())
        .bind(cve_id)
        .bind(package_name)
        .bind((MAX_FLEET_CVE_AFFECTED_SYSTEMS + 1) as i64)
        .fetch_all(pool)
        .await?;

    let systems = reject_inventory_overflow(systems)?;

    systems
        .into_iter()
        .map(|row| {
            let inventory_authority = match row.inventory_authority.as_str() {
                "exact" => SystemCveInventoryAuthority::Exact,
                "legacy" => SystemCveInventoryAuthority::Legacy,
                value => anyhow::bail!("unknown fleet CVE inventory authority {value}"),
            };
            Ok(CveAffectedSystemDetail {
                system_id: row.system_id,
                hostname: row.hostname,
                environment_id: row.environment_id,
                environment: row.environment,
                primary_ip_address: row.primary_ip_address,
                flake_name: row.flake_name,
                flake_id: row.flake_id,
                commit_hash: row.commit_hash,
                deployment_policy: row.deployment_policy,
                current_package_version: row.current_package_version,
                inventory_authority,
            })
        })
        .collect()
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
    let sql = format!(
        "{FLEET_INVENTORY_LIST_CTE}{}",
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
              SELECT 1 FROM exact_subjects subject
              WHERE subject.system_id=scj.system_id
                AND subject.cve_id=scj.cve_id
                AND subject.environment_id=ANY($1)
            ))
          )
        ORDER BY scj.updated_at DESC
        "#,
    );
    let justifications = sqlx::query_as::<_, CveJustification>(&sql)
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

/// Fetches CVE inventory statistics in the caller's scope.
///
/// CVE totals count exact and bounded legacy CVE/package rows. System and
/// environment totals count distinct identities and never sum per-CVE rows.
///
/// # Errors
///
/// Returns a database error when scoped occurrence aggregation fails.
pub async fn fetch_cve_fleet_stats(pool: &PgPool, scope: &CveReadScope) -> Result<CveFleetStats> {
    let sql = format!(
        "{FLEET_INVENTORY_LIST_CTE}{}",
        r#", scoped_systems AS (
          SELECT system.id,system.environment_id,
                 EXISTS(SELECT 1 FROM inventory_subjects subject
                        WHERE subject.system_id=system.id AND subject.authority='exact')
                   AS has_exact,
                 EXISTS(SELECT 1 FROM inventory_subjects subject
                        WHERE subject.system_id=system.id AND subject.authority='legacy')
                   AS has_legacy,
                  EXISTS(SELECT 1 FROM exact_authority_systems exact
                         WHERE exact.system_id=system.id)
                  OR EXISTS(
                    SELECT 1 FROM derivations derivation
                   JOIN derivation_statuses status ON status.id=derivation.status_id
                     AND status.name=ANY(ARRAY['build-complete','complete'])
                   JOIN cve_scans scan ON scan.derivation_id=derivation.id
                     AND scan.status='completed' AND scan.completed_at IS NOT NULL
                   WHERE derivation.derivation_name=system.hostname
                     AND derivation.derivation_type='nixos') AS has_scan
          FROM systems system WHERE system.is_active
            AND ($1::uuid[] IS NULL OR system.environment_id=ANY($1))
        )
        SELECT COUNT(*)::bigint AS total_cves,
          COUNT(*) FILTER (WHERE severity='CRITICAL')::bigint AS critical,
          COUNT(*) FILTER (WHERE severity='HIGH')::bigint AS high,
          COUNT(*) FILTER (WHERE severity='MEDIUM')::bigint AS medium,
          COUNT(*) FILTER (WHERE severity='LOW')::bigint AS low,
          COUNT(*) FILTER (WHERE exploited)::bigint AS exploited,
          COUNT(*) FILTER (WHERE fix_status='fix_available')::bigint AS fixable,
          (SELECT COUNT(DISTINCT environment_id) FROM scoped_systems
           WHERE has_exact OR has_legacy)::bigint
            AS environments_affected,
          (SELECT COUNT(*) FROM scoped_systems WHERE has_exact OR has_legacy)::bigint
            AS systems_affected,
          (SELECT COUNT(*) FROM scoped_systems WHERE has_exact)::bigint
            AS exact_systems_affected,
          (SELECT COUNT(*) FROM scoped_systems WHERE has_legacy)::bigint
            AS legacy_systems_affected,
          (SELECT COUNT(*) FROM scoped_systems WHERE NOT has_scan)::bigint
            AS no_scan_systems,
          COUNT(*) FILTER (WHERE triage_status='outstanding')::bigint AS outstanding,
          COUNT(*) FILTER (WHERE triage_status='accepted')::bigint AS accepted,
          COUNT(*) FILTER (WHERE triage_status='scheduled')::bigint AS scheduled
        FROM inventory_list
        "#
    );
    let stats = sqlx::query_as::<_, CveFleetStats>(&sql)
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
    let sql = format!(
        "{FLEET_INVENTORY_LIST_CTE}{}",
        r#"
        SELECT DISTINCT package_name as "package_name!"
        FROM inventory_list
        WHERE package_name IS NOT NULL
        ORDER BY package_name
        LIMIT 500
        "#
    );
    let packages = sqlx::query_scalar::<_, String>(&sql)
        .bind(scope.environment_ids())
        .fetch_all(pool)
        .await?;

    Ok(packages)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use std::time::{Duration, Instant};

    use crate::models::public_key::PublicKey;
    use crate::models::systems::System;
    use crate::queries::commits::{get_commit_by_hash, insert_commit_with_metadata};
    use crate::queries::derivations::insert_derivation;
    use crate::queries::flakes::insert_flake;
    use crate::queries::systems::insert_system;

    use super::*;

    async fn inventory_test_system(pool: &PgPool, suffix: &str) -> (System, i32) {
        let repo_url = format!("https://example.test/cve-inventory-{suffix}.git");
        let flake = insert_flake(
            pool,
            &format!("cve-inventory-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("inventory flake should persist");
        let hash = format!("{:0>40}", &suffix[..suffix.len().min(32)]);
        insert_commit_with_metadata(
            pool,
            &hash,
            &repo_url,
            Utc::now(),
            Some("test"),
            Some("test"),
        )
        .await
        .expect("inventory commit should persist");
        let commit = get_commit_by_hash(pool, &hash)
            .await
            .expect("inventory commit should load");
        let key = SigningKey::from_bytes(&[43; 32]);
        let system = insert_system(
            pool,
            &System {
                id: Uuid::new_v4(),
                hostname: format!("inventory-{suffix}"),
                environment_id: None,
                is_active: true,
                public_key: PublicKey::from_verifying_key(key.verifying_key()),
                flake_id: Some(flake.id),
                derivation: String::new(),
                system_configuration_name: Some(format!("inventory-{suffix}")),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                desired_target: None,
                deployment_policy: "manual".into(),
            },
        )
        .await
        .expect("inventory system should persist");
        (system, commit.id)
    }

    async fn completed_legacy_scan(pool: &PgPool, system: &System, commit_id: i32) -> Uuid {
        let derivation_id: i32 = sqlx::query_scalar(
            r#"INSERT INTO derivations(
                 derivation_name,derivation_path,derivation_type,commit_id,status_id,
                 completed_at,store_path)
               VALUES($1,$2,'nixos',$3,
                 (SELECT id FROM derivation_statuses WHERE name='build-complete' LIMIT 1),
                 now(),$4)
               RETURNING id"#,
        )
        .bind(&system.hostname)
        .bind(format!("/nix/store/{}-legacy.drv", system.hostname))
        .bind(commit_id)
        .bind(format!("/nix/store/{}-legacy", system.hostname))
        .fetch_one(pool)
        .await
        .expect("legacy derivation should persist");
        sqlx::query_scalar(
            r#"INSERT INTO cve_scans(
                 derivation_id,scanner_name,scanner_version,status,completed_at)
               VALUES($1,'legacy-scanner','0.9','completed',now()) RETURNING id"#,
        )
        .bind(derivation_id)
        .fetch_one(pool)
        .await
        .expect("legacy scan should persist")
    }

    async fn add_legacy_finding(pool: &PgPool, scan_id: Uuid, commit_id: i32, suffix: &str) {
        sqlx::query(
            r#"INSERT INTO cves(id,cvss_v3_score,description,published_date)
               VALUES('CVE-2099-4400',8.7,'legacy inventory finding','2099-01-01')"#,
        )
        .execute(pool)
        .await
        .expect("legacy CVE should persist");
        let package_id: i32 = sqlx::query_scalar(
            r#"INSERT INTO derivations(
                 derivation_name,derivation_path,derivation_type,commit_id,status_id,
                 completed_at,pname,version)
               VALUES('legacy-package',$1,'package',$2,
                 (SELECT id FROM derivation_statuses WHERE name='build-complete' LIMIT 1),
                 now(),'legacy-package','1.0') RETURNING id"#,
        )
        .bind(format!("/nix/store/{suffix}-legacy-package.drv"))
        .bind(commit_id)
        .fetch_one(pool)
        .await
        .expect("legacy package derivation should persist");
        sqlx::query("INSERT INTO scan_packages(scan_id,derivation_id) VALUES($1,$2)")
            .bind(scan_id)
            .bind(package_id)
            .execute(pool)
            .await
            .expect("legacy scan package should persist");
        sqlx::query(
            r#"INSERT INTO package_vulnerabilities(
                 derivation_id,cve_id,is_whitelisted,fixed_version,detection_method)
               VALUES($1,'CVE-2099-4400',false,'1.1','legacy-scanner')"#,
        )
        .bind(package_id)
        .execute(pool)
        .await
        .expect("legacy package vulnerability should persist");
    }

    async fn inventory_test_user(pool: &PgPool, role: &str, active: bool) -> Uuid {
        let user_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO users(
                 id,username,first_name,last_name,email,user_type,is_active)
               VALUES($1,$2,'Inventory','Tester',$3,'human',$4)"#,
        )
        .bind(user_id)
        .bind(format!("inv-{role}-{}", user_id.simple()))
        .bind(format!("inventory-{user_id}@example.test"))
        .bind(active)
        .execute(pool)
        .await
        .expect("inventory test user should persist");
        sqlx::query("INSERT INTO user_role_assignments(user_id,role) VALUES($1,$2::auth_role)")
            .bind(user_id)
            .bind(role)
            .execute(pool)
            .await
            .expect("inventory test role should persist");
        user_id
    }

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

    #[test]
    fn inventory_page_parameters_are_bounded_and_normalized() {
        let request = SystemCveInventoryPageRequest::from_params(SystemCveInventoryParams {
            limit: Some(500),
            q: Some("  OpenSSL   PACKAGE ".into()),
            severity: Some("HIGH,critical,high".into()),
            status: Some("fix_available,open".into()),
            ..SystemCveInventoryParams::default()
        })
        .expect("valid page parameters should normalize");
        assert_eq!(request.limit, 500);
        assert_eq!(request.search.as_deref(), Some("openssl package"));
        assert_eq!(request.severities, vec!["critical", "high"]);
        assert_eq!(request.statuses, vec!["fix_available", "open"]);

        for params in [
            SystemCveInventoryParams {
                limit: Some(0),
                ..SystemCveInventoryParams::default()
            },
            SystemCveInventoryParams {
                limit: Some(501),
                ..SystemCveInventoryParams::default()
            },
            SystemCveInventoryParams {
                severity: Some("urgent".into()),
                ..SystemCveInventoryParams::default()
            },
            SystemCveInventoryParams {
                status: Some("accepted".into()),
                ..SystemCveInventoryParams::default()
            },
        ] {
            let error = SystemCveInventoryPageRequest::from_params(params)
                .expect_err("invalid page parameter must fail");
            assert!(matches!(
                system_cve_inventory_page_error(&error),
                Some(SystemCveInventoryPageError::InvalidRequest(_))
            ));
        }
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn system_inventory_classifies_fallbacks_and_exact_precedence(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (legacy_system, legacy_commit_id) = inventory_test_system(&pool, &suffix).await;
        let legacy_scan = completed_legacy_scan(&pool, &legacy_system, legacy_commit_id).await;
        add_legacy_finding(&pool, legacy_scan, legacy_commit_id, &suffix).await;

        let legacy = fetch_system_cve_inventory(&pool, legacy_system.id)
            .await
            .expect("legacy inventory should load");
        assert_eq!(legacy.authority, SystemCveInventoryAuthority::Legacy);
        assert_eq!(
            legacy.source.as_ref().map(|source| source.scan_id),
            Some(legacy_scan)
        );
        assert_eq!(legacy.rows.len(), 1, "legacy findings must remain visible");

        let duplicate_package_id: i32 = sqlx::query_scalar(
            r#"INSERT INTO derivations(
                 derivation_name,derivation_path,derivation_type,commit_id,status_id,
                 completed_at,pname,version)
               VALUES('duplicate-legacy-package',$1,'package',$2,
                 (SELECT id FROM derivation_statuses WHERE name='build-complete' LIMIT 1),
                 now(),'legacy-package','1.0') RETURNING id"#,
        )
        .bind(format!("/nix/store/000-{suffix}-legacy-package.drv"))
        .bind(legacy_commit_id)
        .fetch_one(&pool)
        .await
        .expect("duplicate legacy package should persist");
        sqlx::query("INSERT INTO scan_packages(scan_id,derivation_id) VALUES($1,$2)")
            .bind(legacy_scan)
            .bind(duplicate_package_id)
            .execute(&pool)
            .await
            .expect("duplicate legacy scan package should persist");
        sqlx::query(
            r#"INSERT INTO package_vulnerabilities(
                 derivation_id,cve_id,is_whitelisted,fixed_version,detection_method)
               VALUES($1,'CVE-2099-4400',false,'1.1','legacy-scanner')"#,
        )
        .bind(duplicate_package_id)
        .execute(&pool)
        .await
        .expect("duplicate legacy vulnerability should persist");
        let deduplicated = fetch_system_cve_inventory(&pool, legacy_system.id)
            .await
            .expect("deduplicated legacy inventory should load");
        assert_eq!(deduplicated.rows.len(), 1);
        assert_eq!(
            deduplicated.rows[0].canonical_package_name,
            "legacy-package"
        );

        let clean_suffix = Uuid::new_v4().simple().to_string();
        let (clean_system, clean_commit_id) = inventory_test_system(&pool, &clean_suffix).await;
        completed_legacy_scan(&pool, &clean_system, clean_commit_id).await;
        let clean = fetch_system_cve_inventory(&pool, clean_system.id)
            .await
            .expect("legacy-clean inventory should load");
        assert_eq!(clean.authority, SystemCveInventoryAuthority::Legacy);
        assert!(clean.source.is_some());
        assert!(
            clean.rows.is_empty(),
            "completed scan with no findings is legacy-clean"
        );

        let no_scan_suffix = Uuid::new_v4().simple().to_string();
        let (no_scan_system, _) = inventory_test_system(&pool, &no_scan_suffix).await;
        let no_scan = fetch_system_cve_inventory(&pool, no_scan_system.id)
            .await
            .expect("no-scan inventory should load");
        assert_eq!(no_scan.authority, SystemCveInventoryAuthority::NoScan);
        assert!(no_scan.source.is_none());
        assert!(no_scan.rows.is_empty());

        let legacy_list = fetch_cve_list(
            &pool,
            &CveReadScope::All,
            &CveFilters {
                search: Some("CVE-2099-4400".to_string()),
                ..CveFilters::default()
            },
        )
        .await
        .expect("legacy fleet inventory should load");
        assert_eq!(legacy_list.len(), 1);
        assert_eq!(legacy_list[0].affected_count, 1);
        assert_eq!(legacy_list[0].exact_affected_count, 0);
        assert_eq!(legacy_list[0].legacy_affected_count, 1);
        assert_eq!(legacy_list[0].triage_status, "inventory_only");
        let legacy_systems = fetch_cve_inventory_systems(
            &pool,
            &CveReadScope::All,
            "CVE-2099-4400",
            Some("legacy-package"),
        )
        .await
        .expect("legacy fleet systems should load");
        assert_eq!(legacy_systems.len(), 1);
        assert_eq!(
            legacy_systems[0].inventory_authority,
            SystemCveInventoryAuthority::Legacy
        );
        let legacy_stats = fetch_cve_fleet_stats(&pool, &CveReadScope::All)
            .await
            .expect("legacy fleet stats should load");
        assert_eq!(legacy_stats.systems_affected, 1);
        assert_eq!(legacy_stats.exact_systems_affected, 0);
        assert_eq!(legacy_stats.legacy_systems_affected, 1);
        assert_eq!(legacy_stats.no_scan_systems, 1);

        let commit = sqlx::query_as::<_, crate::models::commits::Commit>(
            "SELECT * FROM commits WHERE id=$1",
        )
        .bind(legacy_commit_id)
        .fetch_one(&pool)
        .await
        .expect("exact fixture commit should load");
        let exact_derivation =
            insert_derivation(&pool, Some(&commit), &legacy_system.hostname, "nixos")
                .await
                .expect("exact derivation should persist");
        let store_path = format!("/nix/store/{}-exact", legacy_system.hostname);
        sqlx::query("UPDATE derivations SET store_path=$2,completed_at=now() WHERE id=$1")
            .bind(exact_derivation.id)
            .bind(&store_path)
            .execute(&pool)
            .await
            .expect("exact derivation store path should persist");
        let snapshot_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO evaluation_snapshots(
                 commit_id,configuration_name,lifecycle,integrity_version,
                 option_count,module_count,content_bytes)
               VALUES($1,$2,'available',0,0,0,0) RETURNING id"#,
        )
        .bind(legacy_commit_id)
        .bind(&legacy_system.hostname)
        .fetch_one(&pool)
        .await
        .expect("evaluation snapshot should persist");
        sqlx::query(
            r#"UPDATE evaluation_snapshots
               SET integrity_version=1
               WHERE id=$1"#,
        )
        .bind(snapshot_id)
        .execute(&pool)
        .await
        .expect("evaluation snapshot should certify");
        let mut legacy_lineage_transaction = pool
            .begin()
            .await
            .expect("legacy lineage fixture transaction should begin");
        sqlx::query("SET LOCAL session_replication_role='replica'")
            .execute(&mut *legacy_lineage_transaction)
            .await
            .expect("legacy lineage fixture should bypass current writer triggers");
        sqlx::query(
            r#"INSERT INTO evaluation_generation_snapshots(
                 system_id,generation,snapshot_id,derivation_id,commit_id,
                 source_store_path,configuration_name,lineage_verified)
               VALUES($1,7,$2,$3,$4,$5,$6,true)"#,
        )
        .bind(legacy_system.id)
        .bind(snapshot_id)
        .bind(exact_derivation.id)
        .bind(legacy_commit_id)
        .bind(&store_path)
        .bind(&legacy_system.hostname)
        .execute(&pool)
        .await
        .expect("retained generation should persist");
        sqlx::query(
            r#"INSERT INTO system_states(
                 hostname,change_reason,store_path,generation,
                 generation_matches_current_store_path,timestamp)
               VALUES($1,'startup',$2,7,true,now())"#,
        )
        .bind(&legacy_system.hostname)
        .bind(&store_path)
        .execute(&pool)
        .await
        .expect("current exact state should persist");
        let exact_scan_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO cve_scans(derivation_id,scanner_name,status)
               VALUES($1,'exact-scanner','in_progress') RETURNING id"#,
        )
        .bind(exact_derivation.id)
        .fetch_one(&pool)
        .await
        .expect("exact scan should persist");
        sqlx::query(
            r#"INSERT INTO cve_scan_vulnerability_observations(
                 scan_id,canonical_cve_id,canonical_package_name,
                 observed_package_name,observed_package_version,
                 observed_derivation_path,is_whitelisted,detection_method)
               VALUES($1,'CVE-2099-4400','exact-package','exact-package','2.0',
                      '/nix/store/exact-package.drv',false,'exact-scanner')"#,
        )
        .bind(exact_scan_id)
        .execute(&pool)
        .await
        .expect("exact observation should persist");
        sqlx::query(
            r#"INSERT INTO cve_scan_vulnerability_observations(
                 scan_id,canonical_cve_id,canonical_package_name,
                 observed_package_name,observed_package_version,
                 observed_derivation_path,is_whitelisted,detection_method)
               VALUES($1,'CVE-2099-4400','exact-package','duplicate-exact-package','2.1',
                      '/nix/store/zzz-exact-package.drv',false,'exact-scanner')"#,
        )
        .bind(exact_scan_id)
        .execute(&pool)
        .await
        .expect("duplicate exact observation should persist");
        sqlx::query(
            r#"UPDATE cve_scans SET status='completed',
                 completed_at=now()-interval '1 minute',
                  evidence_schema_version=1 WHERE id=$1"#,
        )
        .bind(exact_scan_id)
        .execute(&pool)
        .await
        .expect("exact scan should seal");

        let exact_vulnerable = fetch_system_cve_inventory(&pool, legacy_system.id)
            .await
            .expect("exact inventory should load");
        assert_eq!(
            exact_vulnerable.authority,
            SystemCveInventoryAuthority::Exact
        );
        assert_eq!(
            exact_vulnerable
                .source
                .as_ref()
                .map(|source| source.scan_id),
            Some(exact_scan_id)
        );
        assert_eq!(exact_vulnerable.rows.len(), 1);
        assert_eq!(
            exact_vulnerable.rows[0].canonical_package_name,
            "exact-package"
        );
        assert_eq!(
            exact_vulnerable.rows[0].occurrence_derivation_path, "/nix/store/exact-package.drv",
            "duplicate exact observations must collapse before pagination"
        );
        assert!(exact_vulnerable.exact_authority_failure.is_none());
        let exact_list = fetch_cve_list(
            &pool,
            &CveReadScope::All,
            &CveFilters {
                search: Some("CVE-2099-4400".to_string()),
                ..CveFilters::default()
            },
        )
        .await
        .expect("exact fleet inventory should load");
        let exact_systems =
            fetch_cve_inventory_systems(&pool, &CveReadScope::All, "CVE-2099-4400", None)
                .await
                .expect("exact fleet systems should load");
        assert_eq!(
            exact_list.len(),
            1,
            "exact fleet rows: {exact_list:?}; systems: {exact_systems:?}"
        );
        assert_eq!(exact_list[0].affected_count, 1);
        assert_eq!(exact_list[0].exact_affected_count, 1);
        assert_eq!(exact_list[0].legacy_affected_count, 0);
        assert_eq!(exact_systems.len(), 1);
        assert_eq!(
            exact_systems[0].inventory_authority,
            SystemCveInventoryAuthority::Exact
        );

        let exact_clean_scan_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO cve_scans(derivation_id,scanner_name,status)
               VALUES($1,'exact-scanner','in_progress') RETURNING id"#,
        )
        .bind(exact_derivation.id)
        .fetch_one(&pool)
        .await
        .expect("exact clean scan should persist");
        sqlx::query(
            r#"UPDATE cve_scans SET status='completed',completed_at=now(),
                 evidence_schema_version=1 WHERE id=$1"#,
        )
        .bind(exact_clean_scan_id)
        .execute(&pool)
        .await
        .expect("exact clean scan should seal");
        let exact_clean = fetch_system_cve_inventory(&pool, legacy_system.id)
            .await
            .expect("exact-clean inventory should load");
        assert_eq!(exact_clean.authority, SystemCveInventoryAuthority::Exact);
        assert_eq!(
            exact_clean.source.as_ref().map(|source| source.scan_id),
            Some(exact_clean_scan_id)
        );
        assert!(
            exact_clean.rows.is_empty(),
            "exact-clean must not return or duplicate stale legacy rows"
        );
        assert!(exact_clean.exact_authority_failure.is_none());

        sqlx::query(
            r#"INSERT INTO system_states(
                 hostname,change_reason,store_path,generation,
                 generation_matches_current_store_path,timestamp)
               VALUES($1,'startup',$2,8,false,now()+interval '1 minute')"#,
        )
        .bind(&legacy_system.hostname)
        .bind(&store_path)
        .execute(&pool)
        .await
        .expect("mismatched current state should persist");
        let mismatch = fetch_system_cve_inventory(&pool, legacy_system.id)
            .await
            .expect("mismatched inventory should fall back");
        assert_eq!(mismatch.authority, SystemCveInventoryAuthority::Legacy);
        assert_eq!(
            mismatch.exact_authority_failure,
            Some(ExactCveAuthorityFailureReason::CurrentStoreMismatch)
        );
        assert_eq!(
            mismatch.source.as_ref().map(|source| source.scan_id),
            Some(exact_clean_scan_id)
        );
        assert!(mismatch.rows.is_empty());
        let mismatched_fleet = fetch_cve_list(
            &pool,
            &CveReadScope::All,
            &CveFilters {
                search: Some("CVE-2099-4400".to_string()),
                ..CveFilters::default()
            },
        )
        .await
        .expect("mismatched fleet inventory should fall back");
        assert!(
            mismatched_fleet
                .iter()
                .all(|row| row.exact_affected_count == 0),
            "an older valid state must not retain exact fleet authority"
        );
        assert!(
            fetch_cve_inventory_systems(
                &pool,
                &CveReadScope::All,
                "CVE-2099-4400",
                Some("exact-package")
            )
            .await
            .expect("mismatched exact system array should load")
            .is_empty(),
            "the compatibility system array must reject older-state exact evidence"
        );

        sqlx::query(
            r#"INSERT INTO evaluation_generation_snapshots(
                 system_id,generation,snapshot_id,derivation_id,commit_id,
                 source_store_path,configuration_name,lineage_verified)
               VALUES($1,9,$2,$3,$4,$5,$6,false)"#,
        )
        .bind(legacy_system.id)
        .bind(snapshot_id)
        .bind(exact_derivation.id)
        .bind(legacy_commit_id)
        .bind(&store_path)
        .bind(&legacy_system.hostname)
        .execute(&mut *legacy_lineage_transaction)
        .await
        .expect("unverified retained generation should persist");
        legacy_lineage_transaction
            .commit()
            .await
            .expect("legacy lineage fixture transaction should commit");
        sqlx::query(
            r#"INSERT INTO system_states(
                 hostname,change_reason,store_path,generation,
                 generation_matches_current_store_path,timestamp)
               VALUES($1,'startup',$2,9,true,now()+interval '2 minutes')"#,
        )
        .bind(&legacy_system.hostname)
        .bind(&store_path)
        .execute(&pool)
        .await
        .expect("unverified-lineage current state should persist");
        let unverified = fetch_system_cve_inventory(&pool, legacy_system.id)
            .await
            .expect("unverified lineage should fall back");
        assert_eq!(unverified.authority, SystemCveInventoryAuthority::Legacy);
        assert_eq!(
            unverified.exact_authority_failure,
            Some(ExactCveAuthorityFailureReason::LineageUnverified)
        );
        assert_eq!(
            unverified.source.as_ref().map(|source| source.scan_id),
            Some(exact_clean_scan_id)
        );
        assert!(unverified.rows.is_empty());
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn authorized_system_inventory_enforces_role_membership_and_admin_scope(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (system, _) = inventory_test_system(&pool, &suffix).await;
        let viewer = inventory_test_user(&pool, "viewer", true).await;
        let operator = inventory_test_user(&pool, "operator", true).await;
        let admin = inventory_test_user(&pool, "admin", true).await;
        let inactive_admin = inventory_test_user(&pool, "admin", false).await;

        assert!(
            fetch_authorized_system_cve_inventory(&pool, system.id, admin)
                .await
                .expect("admin inventory read should succeed")
                .is_some(),
            "an active Admin can read an unassigned system"
        );
        for hidden_user in [viewer, operator, inactive_admin] {
            assert!(
                fetch_authorized_system_cve_inventory(&pool, system.id, hidden_user)
                    .await
                    .expect("hidden inventory read should not fail")
                    .is_none()
            );
        }
        let mut hidden_transaction = pool
            .begin()
            .await
            .expect("hidden cursor transaction should begin");
        let hidden_with_malformed_cursor = fetch_authorized_system_cve_inventory_tx(
            &mut hidden_transaction,
            system.id,
            viewer,
            &SystemCveInventoryPageRequest {
                after: Some("not-a-cursor".into()),
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect("hidden system must not parse its cursor");
        assert!(hidden_with_malformed_cursor.is_none());
        hidden_transaction
            .rollback()
            .await
            .expect("hidden cursor transaction should roll back");
        assert!(
            fetch_authorized_system_cve_inventory(&pool, Uuid::new_v4(), admin)
                .await
                .expect("absent inventory read should not fail")
                .is_none(),
            "absent and hidden systems have the same query result"
        );

        let environment_id: Uuid =
            sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
                .bind(format!("inventory-{suffix}"))
                .fetch_one(&pool)
                .await
                .expect("inventory environment should persist");
        sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
            .bind(system.id)
            .bind(environment_id)
            .execute(&pool)
            .await
            .expect("inventory system assignment should persist");
        for member in [viewer, operator] {
            sqlx::query(
                "INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)",
            )
            .bind(member)
            .bind(environment_id)
            .execute(&pool)
            .await
            .expect("inventory membership should persist");
            assert!(
                fetch_authorized_system_cve_inventory(&pool, system.id, member)
                    .await
                    .expect("member inventory read should succeed")
                    .is_some()
            );
        }
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn system_inventory_paginates_1315_stable_findings(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (system, commit_id) = inventory_test_system(&pool, &suffix).await;
        let scan_id = completed_legacy_scan(&pool, &system, commit_id).await;
        sqlx::query(
            r#"INSERT INTO cves(id,cvss_v3_score,description,published_date)
               SELECT 'CVE-2098-' || lpad(value::text,4,'0'),5.0,
                      'bounded inventory finding','2098-01-01'
               FROM generate_series(1,1315) value"#,
        )
        .execute(&pool)
        .await
        .expect("overflow CVEs should persist");
        sqlx::query(
            r#"INSERT INTO derivations(
                 derivation_name,derivation_path,derivation_type,commit_id,status_id,
                 completed_at,pname,version)
               SELECT 'overflow-package-' || value,
                      '/nix/store/overflow-package-' || value || '.drv','package',$1,
                      (SELECT id FROM derivation_statuses WHERE name='build-complete' LIMIT 1),
                      now(),'overflow-package-' || value,'1.0'
                FROM generate_series(1,1315) value"#,
        )
        .bind(commit_id)
        .execute(&pool)
        .await
        .expect("overflow package derivations should persist");
        sqlx::query(
            r#"INSERT INTO scan_packages(scan_id,derivation_id)
               SELECT $1,id FROM derivations
               WHERE commit_id=$2 AND pname LIKE 'overflow-package-%'"#,
        )
        .bind(scan_id)
        .bind(commit_id)
        .execute(&pool)
        .await
        .expect("overflow scan packages should persist");
        sqlx::query(
            r#"INSERT INTO package_vulnerabilities(
                 derivation_id,cve_id,is_whitelisted,detection_method)
               SELECT id,'CVE-2098-' || lpad(substring(pname from 18)::text,4,'0'),
                      false,'legacy-scanner'
               FROM derivations
               WHERE commit_id=$1 AND pname LIKE 'overflow-package-%'"#,
        )
        .bind(commit_id)
        .execute(&pool)
        .await
        .expect("overflow vulnerabilities should persist");
        sqlx::query("UPDATE cves SET cvss_v3_score=9.5 WHERE id<='CVE-2098-0010'")
            .execute(&pool)
            .await
            .expect("critical fixture severities should persist");
        sqlx::query(
            "UPDATE package_vulnerabilities SET fixed_version='2.0' WHERE cve_id='CVE-2098-0001'",
        )
        .execute(&pool)
        .await
        .expect("fixed-version fixture should persist");

        let empty_filters = Vec::<String>::new();
        let metadata_plan_started = Instant::now();
        let metadata_plan = sqlx::query_scalar::<_, String>(&format!(
            "EXPLAIN (ANALYZE, BUFFERS) {}",
            inventory_metadata_sql(LEGACY_INVENTORY_CTE)
        ))
        .bind(scan_id)
        .bind(system.id)
        .bind(Option::<String>::None)
        .bind(&empty_filters)
        .bind(&empty_filters)
        .bind("legacy")
        .fetch_all(&pool)
        .await
        .expect("metadata plan should execute")
        .join("\n");
        let metadata_plan_elapsed = metadata_plan_started.elapsed();
        let page_plan_started = Instant::now();
        let page_plan = sqlx::query_scalar::<_, String>(&format!(
            "EXPLAIN (ANALYZE, BUFFERS) {}",
            inventory_page_sql(LEGACY_INVENTORY_CTE)
        ))
        .bind(scan_id)
        .bind(system.id)
        .bind(Option::<String>::None)
        .bind(&empty_filters)
        .bind(&empty_filters)
        .bind(Some("CVE-2098-0500"))
        .bind(Some("overflow-package-500"))
        .bind(501_i64)
        .fetch_all(&pool)
        .await
        .expect("continuation page plan should execute")
        .join("\n");
        let page_plan_elapsed = page_plan_started.elapsed();
        eprintln!(
            "1,315-row inventory metadata plan ({:?}):\n{}\ncontinuation page plan ({page_plan_elapsed:?}):\n{}",
            metadata_plan_elapsed, metadata_plan, page_plan
        );
        assert!(!inventory_metadata_sql(LEGACY_INVENTORY_CTE).contains("LATERAL"));
        assert!(!inventory_metadata_sql(EXACT_INVENTORY_CTE).contains("LATERAL"));
        assert!(!metadata_plan.contains("system_cve_justifications"));
        assert!(!metadata_plan.contains("poam_"));
        assert!(page_plan.contains("Limit"));
        assert!(page_plan.contains("system_cve_justifications"));
        assert!(
            metadata_plan_elapsed < Duration::from_secs(10)
                && page_plan_elapsed < Duration::from_secs(10),
            "representative inventory plans must each execute within 10 seconds"
        );

        let legacy_error = fetch_system_cve_inventory(&pool, system.id)
            .await
            .expect_err("the complete compatibility response must retain its fixed bound");
        assert!(
            is_system_cve_inventory_overflow(&legacy_error),
            "unexpected legacy inventory error: {legacy_error:#}"
        );

        let mut request = SystemCveInventoryPageRequest {
            limit: 500,
            ..SystemCveInventoryPageRequest::default()
        };
        let mut identities = Vec::new();
        let mut page_count = 0;
        loop {
            let page = fetch_system_cve_inventory_page(&pool, system.id, &request)
                .await
                .expect("inventory page should load");
            page_count += 1;
            assert_eq!(page.metadata.total_findings, 1315);
            assert_eq!(page.metadata.total_cves, 1315);
            assert_eq!(page.metadata.total_packages, 1315);
            assert_eq!(page.metadata.severity.critical, 10);
            assert_eq!(page.metadata.severity.medium, 1305);
            identities.extend(
                page.rows
                    .iter()
                    .map(|row| (row.cve_id.clone(), row.canonical_package_name.clone())),
            );
            if !page.has_more {
                assert!(page.next_cursor.is_none());
                assert_eq!(page.rows.len(), 315);
                break;
            }
            assert_eq!(page.rows.len(), 500);
            request.after = page.next_cursor;
        }
        assert_eq!(page_count, 3);
        assert_eq!(identities.len(), 1315);
        assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
        let unique = identities.iter().collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), identities.len());

        let filtered = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest::from_params(SystemCveInventoryParams {
                q: Some("  CVE-2098-0001  ".into()),
                severity: Some("critical,high".into()),
                status: Some("fix_available".into()),
                ..SystemCveInventoryParams::default()
            })
            .expect("filters should validate"),
        )
        .await
        .expect("filtered inventory should load");
        assert_eq!(filtered.rows.len(), 1);
        assert_eq!(filtered.rows[0].cve_id, "CVE-2098-0001");
        assert_eq!(filtered.metadata.total_findings, 1);
        assert_eq!(filtered.metadata.total_cves, 1);
        assert_eq!(filtered.metadata.total_packages, 1);
        assert_eq!(filtered.metadata.severity.critical, 1);

        let first_page = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                limit: 1,
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect("cursor fixture page should load");
        let stable_revision = first_page.inventory_revision.clone();
        let cursor = first_page.next_cursor.expect("first page should continue");
        let malformed = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                after: Some("not-a-cursor".into()),
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect_err("malformed cursor must fail");
        assert_eq!(
            system_cve_inventory_page_error(&malformed),
            Some(&SystemCveInventoryPageError::InvalidCursor)
        );
        sqlx::query("UPDATE cves SET description='changed between pages' WHERE id='CVE-2098-0001'")
            .execute(&pool)
            .await
            .expect("display metadata change should persist");
        let changed_display = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                after: Some(cursor.clone()),
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect("display metadata must not invalidate membership pagination");
        assert_eq!(changed_display.inventory_revision, stable_revision);
        let justification_user = inventory_test_user(&pool, "operator", true).await;
        sqlx::query(
            r#"INSERT INTO system_cve_justifications(
                 system_id,cve_id,category,reason,updated_by,updated_at)
               VALUES($1,'CVE-2098-0001','accepted_risk','test',$2,now())"#,
        )
        .bind(system.id)
        .bind(justification_user)
        .execute(&pool)
        .await
        .expect("justification should persist");
        let changed_justification = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                after: Some(cursor.clone()),
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect("justification must not invalidate membership pagination");
        assert_eq!(changed_justification.inventory_revision, stable_revision);

        sqlx::query(
            "UPDATE derivations SET version='1.1' WHERE commit_id=$1 AND pname='overflow-package-2'",
        )
        .bind(commit_id)
        .execute(&pool)
        .await
        .expect("search field change should persist");
        let changed_search = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                after: Some(cursor),
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect_err("search field change must invalidate membership pagination");
        assert_eq!(
            system_cve_inventory_page_error(&changed_search),
            Some(&SystemCveInventoryPageError::InventoryChanged)
        );

        let cursor = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                limit: 1,
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect("replacement cursor page should load")
        .next_cursor
        .expect("replacement page should continue");
        sqlx::query(
            "UPDATE derivations SET pname='renamed-package-3' WHERE commit_id=$1 AND pname='overflow-package-3'",
        )
        .bind(commit_id)
        .execute(&pool)
        .await
        .expect("stable identity change should persist");
        let changed_identity = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                after: Some(cursor),
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect_err("stable identity change must invalidate membership pagination");
        assert_eq!(
            system_cve_inventory_page_error(&changed_identity),
            Some(&SystemCveInventoryPageError::InventoryChanged)
        );

        let cursor = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                limit: 1,
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect("identity replacement cursor page should load")
        .next_cursor
        .expect("identity replacement page should continue");
        sqlx::query("UPDATE cves SET cvss_v3_score=8.0 WHERE id='CVE-2098-0003'")
            .execute(&pool)
            .await
            .expect("severity change should persist");
        let changed_severity = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                after: Some(cursor),
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect_err("severity change must invalidate membership pagination");
        assert_eq!(
            system_cve_inventory_page_error(&changed_severity),
            Some(&SystemCveInventoryPageError::InventoryChanged)
        );

        let cursor = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                limit: 1,
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect("severity replacement cursor page should load")
        .next_cursor
        .expect("severity replacement page should continue");
        sqlx::query(
            "UPDATE package_vulnerabilities SET fixed_version='2.0' WHERE cve_id='CVE-2098-0003'",
        )
        .execute(&pool)
        .await
        .expect("fix availability change should persist");
        let changed_fix_status = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                after: Some(cursor),
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect_err("fix availability change must invalidate membership pagination");
        assert_eq!(
            system_cve_inventory_page_error(&changed_fix_status),
            Some(&SystemCveInventoryPageError::InventoryChanged)
        );

        let cursor = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                limit: 1,
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect("fix-status replacement cursor page should load")
        .next_cursor
        .expect("fix-status replacement page should continue");
        let filter_mismatch = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                after: Some(cursor.clone()),
                severities: vec!["critical".into()],
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect_err("filter-bound cursor must reject changed filters");
        assert_eq!(
            system_cve_inventory_page_error(&filter_mismatch),
            Some(&SystemCveInventoryPageError::InventoryChanged)
        );
        let other_suffix = Uuid::new_v4().simple().to_string();
        let (other_system, _) = inventory_test_system(&pool, &other_suffix).await;
        let cross_system = fetch_system_cve_inventory_page(
            &pool,
            other_system.id,
            &SystemCveInventoryPageRequest {
                after: Some(cursor.clone()),
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect_err("cross-system cursor must not be accepted");
        assert_eq!(
            system_cve_inventory_page_error(&cross_system),
            Some(&SystemCveInventoryPageError::InventoryChanged)
        );
        sqlx::query(
            r#"INSERT INTO cve_scans(
                 derivation_id,scanner_name,scanner_version,status,completed_at)
               SELECT derivation_id,'legacy-scanner','1.0','completed',now()+interval '1 minute'
               FROM cve_scans WHERE id=$1"#,
        )
        .bind(scan_id)
        .execute(&pool)
        .await
        .expect("newer legacy scan should persist");
        let stale = fetch_system_cve_inventory_page(
            &pool,
            system.id,
            &SystemCveInventoryPageRequest {
                after: Some(cursor),
                ..SystemCveInventoryPageRequest::default()
            },
        )
        .await
        .expect_err("source-bound cursor must reject a newer selected scan");
        assert_eq!(
            system_cve_inventory_page_error(&stale),
            Some(&SystemCveInventoryPageError::InventoryChanged)
        );
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
