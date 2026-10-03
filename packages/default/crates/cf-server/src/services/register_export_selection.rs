//! Selects complete, authorized register rows in one read-only database snapshot.
//!
//! Export writers consume these typed rows; this module neither serializes files
//! nor changes source-owned POA&M or acceptance decisions.

use std::collections::HashSet;

use serde::Deserialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::models::poam::{PoamListQuery, PoamRegisterSummary};
use crate::queries::acceptance_register::{
    self, AcceptanceEntry, AcceptanceListQuery, AcceptanceReadError, AcceptanceSource,
};
use crate::services::poam::{self, PoamActor, PoamClock, PoamError};

/// Caps the combined number of authorized POA&Ms and acceptance decisions.
pub const MAX_AUTHORIZED_EXPORT_ROWS: usize = 1_000;
const PAGE_SIZE: i64 = 100;

/// Selects the source families included in a register export.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegisterRecordType {
    /// Includes plans and acceptance decisions.
    #[default]
    All,
    /// Includes only plans.
    Plans,
    /// Includes only acceptance decisions.
    Acceptances,
}

/// Contains the source rows and effective actor scope from one database snapshot.
///
/// Each POA&M retains its `summary.id`; each acceptance retains its typed
/// `(source, source_id)` identity. Writers must not infer historical scope from
/// a host's current environment membership.
#[derive(Debug)]
pub struct RegisterExportSelection {
    /// Identifies the authenticated actor whose current roles were checked.
    pub actor_id: Uuid,
    /// Indicates whether the actor had the Admin role in the snapshot.
    pub is_admin: bool,
    /// Contains current environment memberships in the snapshot.
    pub environment_ids: Vec<Uuid>,
    /// Contains all authorized POA&Ms matching the requested list filters.
    pub poams: Vec<PoamRegisterSummary>,
    /// Contains complete linked CVE identities and context for each plan.
    pub poam_context: Vec<PoamExportContext>,
    /// Contains all authorized decisions matching the requested list filters.
    pub acceptances: Vec<AcceptanceEntry>,
    /// Contains snapshot-resolved names and linked policy finding evidence for decisions.
    pub acceptance_context: Vec<AcceptanceExportContext>,
}

/// Contains all recorded plan links, including retired historical links.
#[derive(Debug, sqlx::FromRow)]
pub struct PoamExportContext {
    /// Identifies the source plan.
    pub poam_id: Uuid,
    /// Contains every system referenced by policy or CVE finding links.
    pub system_ids: Vec<Uuid>,
    /// Contains hostnames aligned with `system_ids` in the snapshot.
    pub system_names: Vec<String>,
    /// Contains current assignment-expanded systems used only for visibility checks.
    pub visibility_system_ids: Vec<Uuid>,
    /// Contains source-owned scheduled environment scope, not host membership.
    pub environment_ids: Vec<Uuid>,
    /// Contains environment names aligned with `environment_ids`.
    pub environment_names: Vec<String>,
    /// Contains linked and active scheduled canonical CVE IDs without pagination loss.
    pub cve_ids: Vec<String>,
    /// Contains exact scheduled environment disposition tuples, even without links.
    pub scheduled_cve_tuples: sqlx::types::Json<Vec<ScheduledCveTuple>>,
    /// Contains each persisted policy or CVE finding link, including retired links.
    pub links: sqlx::types::Json<Vec<PoamEvidenceLink>>,
}

/// Describes immutable finding evidence linked to a selected plan.
#[derive(Debug, Deserialize)]
pub struct PoamEvidenceLink {
    /// Identifies the persisted policy or CVE finding.
    pub finding_id: Uuid,
    /// Identifies the CVE link-time baseline scan, when recorded.
    pub scan_id: Option<Uuid>,
    /// Canonical CVE ID for a CVE link; absent for policy links.
    pub canonical_cve_id: Option<String>,
    /// Canonical package name for a CVE link; absent for policy links.
    pub canonical_package_name: Option<String>,
    /// Describes a CVE's canonical tuple; policy links have no tuple label.
    pub description: String,
}

/// Identifies the source-owned environment schedule without asserting a finding.
#[derive(Debug, Deserialize)]
pub struct ScheduledCveTuple {
    /// Canonical CVE ID of the scheduled decision.
    pub canonical_cve_id: String,
    /// Canonical package name of the scheduled decision.
    pub canonical_package_name: String,
}

/// Contains source-owned identity and evidence resolved in the authorization snapshot.
#[derive(Debug, sqlx::FromRow)]
pub struct AcceptanceExportContext {
    /// Identifies the decision whose context is described.
    pub source_id: Uuid,
    /// Gives the persisted system hostname or environment name.
    pub scope_name: String,
    /// Gives the policy name for a waiver; CVE decisions use their exact tuple.
    pub policy_name: Option<String>,
    /// Gives the immutable finding linked to a policy waiver, if applicable.
    pub finding_id: Option<Uuid>,
}

/// Reports a safe, source-specific failure without exposing database details.
#[derive(Debug)]
pub enum RegisterExportSelectionError {
    /// The actor no longer has an active reader role.
    Forbidden,
    /// A list filter or pagination value is invalid.
    InvalidQuery,
    /// The authorized selection exceeds [`MAX_AUTHORIZED_EXPORT_ROWS`].
    TooManyRows,
    /// The POA&M context filter exceeded its canonical candidate scan limit.
    CandidateScanLimit,
    /// The source projection returned the same identity more than once.
    DuplicateIdentity,
    /// A visible register page did not include the full linked source context.
    PartialContext,
    /// A database or source projection failed; details are withheld from clients.
    Internal,
}

impl std::fmt::Display for RegisterExportSelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Forbidden => "Export reader is not authorized",
            Self::InvalidQuery => "Invalid export selection filters",
            Self::TooManyRows => "Authorized export exceeds the row limit",
            Self::CandidateScanLimit => "The query is too broad; add a narrower filter",
            Self::DuplicateIdentity => "Duplicate export source identity",
            Self::PartialContext => "Export source context is not fully authorized",
            Self::Internal => "Export selection could not be loaded",
        })
    }
}

impl std::error::Error for RegisterExportSelectionError {}

fn poam_error(error: PoamError) -> RegisterExportSelectionError {
    match error {
        PoamError::Forbidden => RegisterExportSelectionError::Forbidden,
        PoamError::Validation("candidate_scan_limit", _) => {
            RegisterExportSelectionError::CandidateScanLimit
        }
        PoamError::Validation(..) => RegisterExportSelectionError::InvalidQuery,
        _ => RegisterExportSelectionError::Internal,
    }
}

fn acceptance_error(error: AcceptanceReadError) -> RegisterExportSelectionError {
    match error {
        AcceptanceReadError::Forbidden => RegisterExportSelectionError::Forbidden,
        AcceptanceReadError::Validation(..) => RegisterExportSelectionError::InvalidQuery,
        _ => RegisterExportSelectionError::Internal,
    }
}

async fn reading_actor_tx(
    tx: &mut Transaction<'_, Postgres>,
    request: &PoamActor,
) -> Result<PoamActor, RegisterExportSelectionError> {
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT role::text FROM user_role_assignments WHERE user_id=$1 AND EXISTS (SELECT 1 FROM users WHERE id=$1 AND is_active)",
    )
    .bind(request.user_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| RegisterExportSelectionError::Internal)?;
    let is_admin = roles.iter().any(|role| role == "admin");
    let can_mutate = is_admin || roles.iter().any(|role| role == "operator");
    if !can_mutate && !roles.iter().any(|role| role == "viewer") {
        return Err(RegisterExportSelectionError::Forbidden);
    }
    let environment_ids = sqlx::query_scalar(
        "SELECT environment_id FROM user_environment_memberships WHERE user_id=$1 ORDER BY environment_id",
    )
    .bind(request.user_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| RegisterExportSelectionError::Internal)?;
    Ok(PoamActor {
        user_id: request.user_id,
        identifier: request.identifier.clone(),
        is_admin,
        can_mutate,
        environment_ids,
        request_origin: request.request_origin.clone(),
    })
}

/// Loads both filtered registers in a single short repeatable-read snapshot.
///
/// The caller must authenticate `actor.user_id`. Request pagination is ignored:
/// the filters select the whole authorized scope, and each source is paged at
/// 100 rows. The 1,001st combined row fails the entire selection. POA&M context
/// filters retain their existing candidate scan limit even if fewer than 1,001
/// rows match. A concurrent commit cannot change membership or rows mid-export.
///
/// # Errors
///
/// Returns a sanitized authorization, filter, candidate-limit, row-limit,
/// duplicate-identity, or internal error. No partial selection is returned.
pub async fn select(
    pool: &PgPool,
    actor: &PoamActor,
    poam_query: &PoamListQuery,
    acceptance_query: &AcceptanceListQuery,
    clock: &dyn PoamClock,
) -> Result<RegisterExportSelection, RegisterExportSelectionError> {
    select_scoped(
        pool,
        actor,
        poam_query,
        acceptance_query,
        clock,
        RegisterRecordType::All,
    )
    .await
}

/// Loads the selected source families in one authorized repeatable-read snapshot.
///
/// The caller must authenticate the actor. Pagination is ignored and the
/// 1,000-row cap applies to the selected families together. Excluded families
/// are not read and cannot contribute rows or a candidate-limit failure.
///
/// # Errors
///
/// Returns a sanitized authorization, filter, cap, incomplete-context, or
/// persistence error instead of a partial selection.
pub async fn select_scoped(
    pool: &PgPool,
    actor: &PoamActor,
    poam_query: &PoamListQuery,
    acceptance_query: &AcceptanceListQuery,
    clock: &dyn PoamClock,
    record_type: RegisterRecordType,
) -> Result<RegisterExportSelection, RegisterExportSelectionError> {
    select_inner(
        pool,
        actor,
        poam_query,
        acceptance_query,
        clock,
        record_type,
    )
    .await
}

/// Selects the entire authorized acceptance register without unrelated POA&M rows.
///
/// The caller must authenticate the actor. Names and policy finding IDs are
/// loaded inside the same snapshot as source decisions; a missing source scope
/// aborts the export rather than supplying a guessed label.
///
/// # Errors
/// Returns sanitized authorization, filter, cap, or persistence errors.
pub async fn select_acceptances(
    pool: &PgPool,
    actor: &PoamActor,
    query: &AcceptanceListQuery,
) -> Result<RegisterExportSelection, RegisterExportSelectionError> {
    select_inner(
        pool,
        actor,
        &PoamListQuery::default(),
        query,
        &poam::SystemClock,
        RegisterRecordType::Acceptances,
    )
    .await
}

async fn select_inner(
    pool: &PgPool,
    actor: &PoamActor,
    poam_query: &PoamListQuery,
    acceptance_query: &AcceptanceListQuery,
    clock: &dyn PoamClock,
    record_type: RegisterRecordType,
) -> Result<RegisterExportSelection, RegisterExportSelectionError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| RegisterExportSelectionError::Internal)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|_| RegisterExportSelectionError::Internal)?;
    let actor = reading_actor_tx(&mut tx, actor).await?;
    let mut poams = Vec::new();
    let mut poam_ids = HashSet::new();
    let mut offset = 0;
    while record_type != RegisterRecordType::Acceptances {
        let page = poam::list_register_tx(
            &mut tx,
            &actor,
            &PoamListQuery {
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
                ..poam_query.clone()
            },
            clock,
        )
        .await
        .map_err(poam_error)?;
        for row in page.items {
            if !poam_ids.insert(row.summary.id) {
                return Err(RegisterExportSelectionError::DuplicateIdentity);
            }
            poams.push(row);
        }
        if poams.len() > MAX_AUTHORIZED_EXPORT_ROWS {
            return Err(RegisterExportSelectionError::TooManyRows);
        }
        if !page.has_more {
            break;
        }
        offset = page
            .next_offset
            .ok_or(RegisterExportSelectionError::Internal)?;
    }
    // The context matcher may stop after finding enough matches to fill a page.
    // CONCURRENCY: Probe past its candidate boundary in the same snapshot so
    // >1,000 candidates never become a silently incomplete export.
    if record_type != RegisterRecordType::Acceptances
        && (poam_query.policy_lineage_id.is_some()
            || poam_query.bundle_id.is_some()
            || poam_query.requirement.is_some())
    {
        poam::list_register_tx(
            &mut tx,
            &actor,
            &PoamListQuery {
                limit: Some(PAGE_SIZE),
                offset: Some(MAX_AUTHORIZED_EXPORT_ROWS as i64),
                ..poam_query.clone()
            },
            clock,
        )
        .await
        .map_err(poam_error)?;
    }
    // SECURITY: The register projection is page-scoped and its first_cve is
    // only a preview. Read every immutable link under the same RR snapshot;
    // do not silently export just the visible or first linked CVE.
    let plan_ids: Vec<_> = poams.iter().map(|row| row.summary.id).collect();
    let poam_context = sqlx::query_as::<_, PoamExportContext>(
        r#"SELECT plan.id AS poam_id,
                  ARRAY(SELECT DISTINCT links.system_id FROM (
                    SELECT f.system_id FROM poam_finding_links l
                    JOIN poam_findings f ON f.id=l.finding_id WHERE l.poam_id=plan.id
                    UNION ALL
                    SELECT l.system_id FROM poam_cve_finding_links l WHERE l.poam_id=plan.id
                    UNION ALL
                    SELECT a.system_id FROM poam_assignment_references r
                    JOIN compliance_bundle_assignments a ON a.id=r.assignment_id
                    WHERE r.poam_id=plan.id AND a.system_id IS NOT NULL
                  ) links ORDER BY links.system_id) AS system_ids,
                   ARRAY(SELECT s.hostname FROM systems s WHERE s.id=ANY(
                     ARRAY(SELECT DISTINCT links.system_id FROM (
                       SELECT f.system_id FROM poam_finding_links l
                       JOIN poam_findings f ON f.id=l.finding_id WHERE l.poam_id=plan.id
                       UNION ALL
                       SELECT l.system_id FROM poam_cve_finding_links l WHERE l.poam_id=plan.id
                       UNION ALL
                       SELECT a.system_id FROM poam_assignment_references r
                       JOIN compliance_bundle_assignments a ON a.id=r.assignment_id
                       WHERE r.poam_id=plan.id AND a.system_id IS NOT NULL
                     ) links)) ORDER BY s.id) AS system_names,
                   ARRAY(SELECT c.system_id FROM poam_context_systems c
                    WHERE c.poam_id=plan.id ORDER BY c.system_id) AS visibility_system_ids,
                  ARRAY(SELECT DISTINCT scopes.environment_id FROM (
                    SELECT d.environment_id FROM cve_current_environment_dispositions d
                    WHERE d.poam_id=plan.id AND d.state='scheduled'
                      AND plan.status<>'completed'
                    UNION ALL
                    SELECT a.environment_id FROM poam_assignment_references r
                    JOIN compliance_bundle_assignments a ON a.id=r.assignment_id
                    WHERE r.poam_id=plan.id AND a.environment_id IS NOT NULL
                   ) scopes ORDER BY scopes.environment_id) AS environment_ids,
                   ARRAY(SELECT e.name FROM environments e WHERE e.id=ANY(
                     ARRAY(SELECT DISTINCT scopes.environment_id FROM (
                       SELECT d.environment_id FROM cve_current_environment_dispositions d
                       WHERE d.poam_id=plan.id AND d.state='scheduled'
                         AND plan.status<>'completed'
                       UNION ALL
                       SELECT a.environment_id FROM poam_assignment_references r
                       JOIN compliance_bundle_assignments a ON a.id=r.assignment_id
                       WHERE r.poam_id=plan.id AND a.environment_id IS NOT NULL
                     ) scopes)) ORDER BY e.id) AS environment_names,
                    ARRAY(SELECT DISTINCT cve_id FROM (
                      SELECT l.canonical_cve_id::text AS cve_id
                      FROM poam_cve_finding_links l WHERE l.poam_id=plan.id
                      UNION ALL
                      SELECT d.canonical_cve_id::text FROM cve_current_environment_dispositions d
                      WHERE d.poam_id=plan.id AND d.state='scheduled' AND plan.status<>'completed'
                    ) source_cves ORDER BY cve_id) AS cve_ids,
                    COALESCE((SELECT jsonb_agg(jsonb_build_object(
                      'canonical_cve_id',d.canonical_cve_id,
                      'canonical_package_name',d.canonical_package_name)
                      ORDER BY d.canonical_cve_id,d.canonical_package_name)
                      FROM cve_current_environment_dispositions d
                      WHERE d.poam_id=plan.id AND d.state='scheduled'
                        AND plan.status<>'completed'), '[]'::jsonb) AS scheduled_cve_tuples,
                    COALESCE((SELECT jsonb_agg(jsonb_build_object(
                      'finding_id', link.finding_id, 'scan_id', link.scan_id,
                      'canonical_cve_id',link.canonical_cve_id,
                      'canonical_package_name',link.canonical_package_name,
                      'description', link.description) ORDER BY link.finding_id, link.scan_id NULLS FIRST)
                      FROM (
                        SELECT l.finding_id, NULL::uuid AS scan_id,
                          NULL::text AS canonical_cve_id,NULL::text AS canonical_package_name,
                          'Policy finding'::text AS description
                       FROM poam_finding_links l WHERE l.poam_id=plan.id
                       UNION ALL
                        SELECT l.cve_finding_id AS finding_id,l.baseline_scan_id AS scan_id,
                          l.canonical_cve_id::text,l.canonical_package_name::text,
                          (l.canonical_cve_id || ' / ' || l.canonical_package_name)::text AS description
                       FROM poam_cve_finding_links l WHERE l.poam_id=plan.id
                     ) link), '[]'::jsonb) AS links
           FROM poams plan WHERE plan.id=ANY($1) ORDER BY plan.id"#,
    )
    .bind(&plan_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| RegisterExportSelectionError::Internal)?;
    if poam_context.len() != poams.len() {
        return Err(RegisterExportSelectionError::PartialContext);
    }
    if poam_context.iter().any(|context| {
        context.system_ids.len() != context.system_names.len()
            || context.environment_ids.len() != context.environment_names.len()
            || context
                .system_names
                .iter()
                .any(|name| name.trim().is_empty())
            || context
                .environment_names
                .iter()
                .any(|name| name.trim().is_empty())
    }) {
        return Err(RegisterExportSelectionError::PartialContext);
    }
    if !actor.is_admin {
        let source_system_ids: HashSet<_> = poam_context
            .iter()
            .flat_map(|context| {
                context
                    .system_ids
                    .iter()
                    .chain(&context.visibility_system_ids)
                    .copied()
            })
            .collect();
        let source_system_ids: Vec<_> = source_system_ids.into_iter().collect();
        // SECURITY: Retired links can be absent from the current register
        // projection. Authorize every exact source system against its current
        // environment in this snapshot; a missing or moved-out host fails.
        let accessible: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM systems WHERE id=ANY($1) AND environment_id=ANY($2)",
        )
        .bind(&source_system_ids)
        .bind(&actor.environment_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| RegisterExportSelectionError::Internal)?;
        if accessible.len() != source_system_ids.len() {
            return Err(RegisterExportSelectionError::PartialContext);
        }
        for row in &mut poams {
            let context = poam_context
                .iter()
                .find(|c| c.poam_id == row.summary.id)
                .ok_or(RegisterExportSelectionError::PartialContext)?;
            if context
                .environment_ids
                .iter()
                .any(|id| !actor.environment_ids.contains(id))
            {
                return Err(RegisterExportSelectionError::PartialContext);
            }
            // The writers independently require the export row to contain all
            // authorized source IDs. Extend the export-only row after checking
            // exact membership; the normal register projection is unchanged.
            for id in context
                .system_ids
                .iter()
                .chain(&context.visibility_system_ids)
            {
                if !row.system_ids.contains(id) {
                    row.system_ids.push(*id);
                }
            }
            for id in &context.environment_ids {
                if !row.environment_ids.contains(id) {
                    row.environment_ids.push(*id);
                }
            }
        }
    }
    let mut acceptances = Vec::new();
    let mut acceptance_ids = HashSet::<(u8, Uuid)>::new();
    let mut offset = 0;
    while record_type != RegisterRecordType::Plans {
        let page = acceptance_register::list_tx(
            &mut tx,
            actor.user_id,
            &AcceptanceListQuery {
                source: acceptance_query.source,
                status: acceptance_query.status.clone(),
                environment_id: acceptance_query.environment_id,
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
            },
        )
        .await
        .map_err(acceptance_error)?;
        for row in page.items {
            let source = match row.source {
                AcceptanceSource::PolicyWaiver => 0,
                AcceptanceSource::CveHost => 1,
                AcceptanceSource::CveEnvironment => 2,
            };
            if !acceptance_ids.insert((source, row.source_id)) {
                return Err(RegisterExportSelectionError::DuplicateIdentity);
            }
            acceptances.push(row);
        }
        if poams.len() + acceptances.len() > MAX_AUTHORIZED_EXPORT_ROWS {
            return Err(RegisterExportSelectionError::TooManyRows);
        }
        if !page.has_more {
            break;
        }
        offset += PAGE_SIZE;
    }
    let ids: Vec<_> = acceptances.iter().map(|row| row.source_id).collect();
    let acceptance_context = sqlx::query_as::<_, AcceptanceExportContext>(
        r#"SELECT decisions.source_id,decisions.scope_name,decisions.policy_name,decisions.finding_id
           FROM (
             SELECT w.id AS source_id,s.hostname AS scope_name,p.name AS policy_name,w.finding_id
             FROM finding_waivers w JOIN poam_findings f ON f.id=w.finding_id
             JOIN systems s ON s.id=f.system_id
             JOIN deployment_policies p ON p.id=f.policy_lineage_id WHERE w.id=ANY($1)
             UNION ALL
             SELECT d.id,s.hostname,NULL::text,NULL::uuid
             FROM cve_system_dispositions d JOIN systems s ON s.id=d.system_id WHERE d.id=ANY($1)
             UNION ALL
             SELECT d.id,e.name,NULL::text,NULL::uuid
             FROM cve_environment_dispositions d JOIN environments e ON e.id=d.environment_id
             WHERE d.id=ANY($1)
           ) decisions"#,
    )
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| RegisterExportSelectionError::Internal)?;
    // A source UUID is globally unique in practice, but enforce the projection
    // identity here so the writer cannot attach another source's evidence.
    let context_ids: HashSet<_> = acceptance_context.iter().map(|row| row.source_id).collect();
    if context_ids.len() != acceptance_context.len() || context_ids.len() != acceptances.len() {
        return Err(RegisterExportSelectionError::DuplicateIdentity);
    }
    tx.commit()
        .await
        .map_err(|_| RegisterExportSelectionError::Internal)?;
    Ok(RegisterExportSelection {
        actor_id: actor.user_id,
        is_admin: actor.is_admin,
        environment_ids: actor.environment_ids,
        poams,
        poam_context,
        acceptances,
        acceptance_context,
    })
}
