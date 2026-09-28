//! Builds POA&M-only tabular exports from one authorized database snapshot.
//!
//! A register context is not a POA&M ownership field. This reader exports all
//! persisted link systems, immutable assignment scopes, and live scheduled
//! environments. A plan without recorded scope has unspecified scope; no scope
//! is inferred from a host's current environment.

use std::collections::{BTreeMap, HashMap, HashSet};

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::models::poam::{PoamListQuery, PoamRegisterSummary};
use crate::services::poam::{self, PoamActor, PoamClock, PoamError};
use crate::services::register_tabular_export::{
    self as tabular, Entry, Evidence, Scope, Snapshot, Source,
};

const PAGE_SIZE: i64 = 100;
/// Caps the authorized source population before expanding evidence rows.
pub const MAX_POAMS: usize = 1_000;

/// Reports a failure without exposing hidden scope or persistence details.
#[derive(Debug, PartialEq, Eq)]
pub enum ExportError {
    /// The actor has no active reader role.
    Forbidden,
    /// The filter is invalid or its context scan cannot be completed.
    InvalidQuery,
    /// The authorized scope exceeds the bounded export capacity.
    TooManyRows,
    /// A recorded link or schedule exists outside the reader's authorized scope.
    AmbiguousScope,
    /// The source projection or serialization could not be completed.
    Internal,
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Forbidden => "Export reader is not authorized",
            Self::InvalidQuery => "Invalid or too broad POA&M export filters",
            Self::TooManyRows => "Authorized POA&M export exceeds the row limit",
            Self::AmbiguousScope => "POA&M export contains hidden recorded scope",
            Self::Internal => "POA&M export could not be produced",
        })
    }
}

impl std::error::Error for ExportError {}

#[derive(sqlx::FromRow)]
struct Link {
    poam_id: Uuid,
    system_id: Uuid,
    hostname: String,
    finding_id: Uuid,
    scan_id: Option<Uuid>,
    description: String,
}

#[derive(sqlx::FromRow)]
struct Schedule {
    poam_id: Uuid,
    environment_id: Uuid,
    environment_name: String,
}

#[derive(sqlx::FromRow)]
struct AssignmentScope {
    poam_id: Uuid,
    kind: String,
    id: Uuid,
    name: String,
}

struct Plan {
    summary: PoamRegisterSummary,
    links: Vec<Link>,
    schedules: Vec<Schedule>,
    assignments: Vec<AssignmentScope>,
}

fn service_error(error: PoamError) -> ExportError {
    match error {
        PoamError::Forbidden => ExportError::Forbidden,
        PoamError::Validation(..) => ExportError::InvalidQuery,
        _ => ExportError::Internal,
    }
}

async fn snapshot_actor(
    tx: &mut Transaction<'_, Postgres>,
    request: &PoamActor,
) -> Result<PoamActor, ExportError> {
    // SECURITY: Never trust request-carried roles or memberships for exports.
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT role::text FROM user_role_assignments WHERE user_id=$1 AND EXISTS (SELECT 1 FROM users WHERE id=$1 AND is_active)",
    )
    .bind(request.user_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ExportError::Internal)?;
    let is_admin = roles.iter().any(|role| role == "admin");
    let can_mutate = is_admin || roles.iter().any(|role| role == "operator");
    if !can_mutate && !roles.iter().any(|role| role == "viewer") {
        return Err(ExportError::Forbidden);
    }
    let environment_ids = sqlx::query_scalar(
        "SELECT environment_id FROM user_environment_memberships WHERE user_id=$1 ORDER BY environment_id",
    )
    .bind(request.user_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ExportError::Internal)?;
    Ok(PoamActor {
        user_id: request.user_id,
        identifier: request.identifier.clone(),
        is_admin,
        can_mutate,
        environment_ids,
        request_origin: request.request_origin.clone(),
    })
}

async fn links_for_page(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[Uuid],
    actor: &PoamActor,
) -> Result<(Vec<Link>, Vec<Schedule>, Vec<AssignmentScope>), ExportError> {
    // SECURITY: Retired moved-host evidence belongs to its original episode,
    // but must not disclose that host's current name to an A-only reader.
    // The scan ID is the immutable CVE link-time baseline, not a current scan.
    // SECURITY: A visible POA&M with any hidden recorded link or immutable
    // assignment scope cannot be exported as a partial source record.
    let hidden: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
             SELECT 1 FROM poam_finding_links l
             JOIN poam_findings f ON f.id=l.finding_id
             JOIN systems s ON s.id=f.system_id
             WHERE l.poam_id=ANY($1) AND NOT ($2 OR COALESCE(s.environment_id=ANY($3),false))
             UNION ALL
              SELECT 1 FROM poam_cve_finding_links l
              JOIN systems s ON s.id=l.system_id
              WHERE l.poam_id=ANY($1) AND NOT ($2 OR COALESCE(s.environment_id=ANY($3),false))
              UNION ALL
               SELECT 1 FROM cve_current_environment_dispositions d
              JOIN poams p ON p.id=d.poam_id
              WHERE d.poam_id=ANY($1) AND d.state='scheduled'
                AND p.status<>'completed'
                 AND NOT ($2 OR COALESCE(d.environment_id=ANY($3),false))
              UNION ALL
               SELECT 1 FROM poam_assignment_references r
               JOIN compliance_bundle_assignments a ON a.id=r.assignment_id
               LEFT JOIN systems s ON s.id=a.system_id
               WHERE r.poam_id=ANY($1) AND NOT ($2 OR CASE
                 WHEN a.system_id IS NOT NULL THEN COALESCE(s.environment_id=ANY($3),false)
                 ELSE COALESCE(a.environment_id=ANY($3),false) END)
           )"#,
    )
    .bind(ids)
    .bind(actor.is_admin)
    .bind(&actor.environment_ids)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| ExportError::Internal)?;
    if hidden {
        return Err(ExportError::AmbiguousScope);
    }
    let links = sqlx::query_as::<_, Link>(
        r#"SELECT l.poam_id, f.system_id, s.hostname, l.finding_id,
                  NULL::uuid AS scan_id, ''::text AS description
           FROM poam_finding_links l JOIN poam_findings f ON f.id=l.finding_id
           JOIN systems s ON s.id=f.system_id
           WHERE l.poam_id=ANY($1) AND ($2 OR s.environment_id=ANY($3))
           UNION ALL
           SELECT l.poam_id, l.system_id, s.hostname, l.cve_finding_id,
                  l.baseline_scan_id,
                  (l.canonical_cve_id || ' / ' || l.canonical_package_name)::text
           FROM poam_cve_finding_links l JOIN systems s ON s.id=l.system_id
           WHERE l.poam_id=ANY($1) AND ($2 OR s.environment_id=ANY($3))
           ORDER BY poam_id, finding_id, scan_id NULLS FIRST"#,
    )
    .bind(ids)
    .bind(actor.is_admin)
    .bind(&actor.environment_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ExportError::Internal)?;
    let schedules = sqlx::query_as::<_, Schedule>(
        r#"SELECT DISTINCT d.poam_id,d.environment_id,e.name AS environment_name
           FROM cve_current_environment_dispositions d
           JOIN environments e ON e.id=d.environment_id
           JOIN poams p ON p.id=d.poam_id
           WHERE d.poam_id=ANY($1) AND d.state='scheduled'
             AND p.status<>'completed' AND ($2 OR d.environment_id=ANY($3))
           ORDER BY d.poam_id,d.environment_id"#,
    )
    .bind(ids)
    .bind(actor.is_admin)
    .bind(&actor.environment_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ExportError::Internal)?;
    let assignments = sqlx::query_as::<_, AssignmentScope>(
        r#"SELECT r.poam_id, 'system'::text AS kind,
                  a.system_id AS id, s.hostname AS name
           FROM poam_assignment_references r
           JOIN compliance_bundle_assignments a ON a.id=r.assignment_id
           JOIN systems s ON s.id=a.system_id
           WHERE r.poam_id=ANY($1) AND a.system_id IS NOT NULL
             AND ($2 OR s.environment_id=ANY($3))
           UNION
           SELECT r.poam_id, 'environment'::text AS kind,
                  a.environment_id AS id, e.name AS name
           FROM poam_assignment_references r
           JOIN compliance_bundle_assignments a ON a.id=r.assignment_id
           JOIN environments e ON e.id=a.environment_id
           WHERE r.poam_id=ANY($1) AND a.environment_id IS NOT NULL
             AND ($2 OR a.environment_id=ANY($3))
           ORDER BY poam_id, kind, id"#,
    )
    .bind(ids)
    .bind(actor.is_admin)
    .bind(&actor.environment_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ExportError::Internal)?;
    Ok((links, schedules, assignments))
}

fn recorded_scopes(plan: &Plan) -> Vec<Scope<'_>> {
    // INVARIANT: The key deduplicates links and assignment versions for the
    // same exact scope. A host's environment is not an assignment scope.
    let mut identities = BTreeMap::new();
    for link in &plan.links {
        identities.insert(("system", link.system_id), link.hostname.as_str());
    }
    for schedule in &plan.schedules {
        identities.insert(
            ("environment", schedule.environment_id),
            schedule.environment_name.as_str(),
        );
    }
    for assignment in &plan.assignments {
        identities.insert(
            (assignment.kind.as_str(), assignment.id),
            assignment.name.as_str(),
        );
    }
    identities
        .into_iter()
        .map(|((kind, id), name)| match kind {
            "system" => Scope::System { id, name },
            _ => Scope::Environment { id, name },
        })
        .collect()
}

fn write_plans(plans: &[Plan]) -> Result<tabular::Exports, ExportError> {
    let scopes = plans.iter().map(recorded_scopes).collect::<Vec<_>>();
    let evidence = plans
        .iter()
        .map(|plan| {
            plan.links
                .iter()
                .map(|link| Evidence {
                    finding_id: Some(link.finding_id),
                    scan_id: link.scan_id,
                    description: &link.description,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let entries = plans
        .iter()
        .zip(&evidence)
        .zip(&scopes)
        .map(|((plan, links), scopes)| {
            let summary = &plan.summary.summary;
            Entry {
                uuid: summary.id,
                source_id: &summary.human_id,
                title: &summary.title,
                description: &summary.plan,
                status: &summary.status,
                scope: match scopes.as_slice() {
                    [] => Scope::Unspecified,
                    [Scope::System { id, name }] => Scope::System { id: *id, name },
                    [Scope::Environment { id, name }] => Scope::Environment { id: *id, name },
                    _ => Scope::Multiple { scopes },
                },
                // A first displayed CVE is not an exact, single-source CVE ID.
                cve_id: None,
                source: Source::Plan {
                    target_date: summary.target_date,
                },
                evidence: links,
            }
        })
        .collect::<Vec<_>>();
    tabular::write_register(&Snapshot { entries: &entries }).map_err(|_| ExportError::Internal)
}

/// Exports every matching authorized POA&M as equivalent CSV and XLSX bytes.
///
/// The caller must authenticate `actor.user_id`. Pagination supplied in `query`
/// is ignored. The reader rechecks roles and memberships and collects complete
/// pages, finding links, and immutable assignment scopes inside one read-only
/// repeatable-read transaction. It returns no partial download if scope,
/// size, or serialization fails. It never joins a separate post-selection
/// detail read.
///
/// # Errors
/// Returns [`ExportError::Forbidden`] for inactive readers,
/// [`ExportError::TooManyRows`] above the authorized cap, or
/// [`ExportError::AmbiguousScope`] when a recorded scope is hidden from the reader.
/// Filter, candidate-scan, database, and writer failures also abort the export.
pub async fn export(
    pool: &PgPool,
    actor: &PoamActor,
    query: &PoamListQuery,
    clock: &dyn PoamClock,
) -> Result<tabular::Exports, ExportError> {
    let mut tx = pool.begin().await.map_err(|_| ExportError::Internal)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|_| ExportError::Internal)?;
    let actor = snapshot_actor(&mut tx, actor).await?;
    let mut plans = Vec::new();
    let mut seen = HashSet::new();
    let mut offset = 0;
    loop {
        let page = poam::list_register_tx(
            &mut tx,
            &actor,
            &PoamListQuery {
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
                ..query.clone()
            },
            clock,
        )
        .await
        .map_err(service_error)?;
        let ids = page
            .items
            .iter()
            .map(|row| row.summary.id)
            .collect::<Vec<_>>();
        let (links, schedules, assignments) = links_for_page(&mut tx, &ids, &actor).await?;
        let mut links_by_id = HashMap::<Uuid, Vec<Link>>::new();
        for link in links {
            links_by_id.entry(link.poam_id).or_default().push(link);
        }
        let mut schedules_by_id = HashMap::<Uuid, Vec<Schedule>>::new();
        for schedule in schedules {
            schedules_by_id
                .entry(schedule.poam_id)
                .or_default()
                .push(schedule);
        }
        let mut assignments_by_id = HashMap::<Uuid, Vec<AssignmentScope>>::new();
        for assignment in assignments {
            assignments_by_id
                .entry(assignment.poam_id)
                .or_default()
                .push(assignment);
        }
        for summary in page.items {
            let id = summary.summary.id;
            if !seen.insert(id) {
                return Err(ExportError::Internal);
            }
            plans.push(Plan {
                summary,
                links: links_by_id.remove(&id).unwrap_or_default(),
                schedules: schedules_by_id.remove(&id).unwrap_or_default(),
                assignments: assignments_by_id.remove(&id).unwrap_or_default(),
            });
        }
        if plans.len() > MAX_POAMS {
            return Err(ExportError::TooManyRows);
        }
        if !page.has_more {
            break;
        }
        offset = page.next_offset.ok_or(ExportError::Internal)?;
    }
    // The canonical context matcher can fill a page before scanning its full
    // candidate population. Probe the boundary in the SAME snapshot; an error
    // must abort rather than silently omit a later matching record.
    if query.policy_lineage_id.is_some() || query.bundle_id.is_some() || query.requirement.is_some()
    {
        poam::list_register_tx(
            &mut tx,
            &actor,
            &PoamListQuery {
                limit: Some(PAGE_SIZE),
                offset: Some(MAX_POAMS as i64),
                ..query.clone()
            },
            clock,
        )
        .await
        .map_err(service_error)?;
    }
    let result = write_plans(&plans)?;
    tx.commit().await.map_err(|_| ExportError::Internal)?;
    Ok(result)
}
