//! Provides persistence queries for POA&M resources and evidence views.
//!
//! Callers supply actor visibility inputs explicitly. This module applies those
//! filters in SQL and returns persistence models; lifecycle authorization and
//! mutation policy remain in the POA&M service layer.

use anyhow::Result;
use chrono::NaiveDate;
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::models::poam::{
    ActivityView, AssignmentReferenceView, CompatibleFinding, CveFindingView,
    CveVerificationItemView, DashboardSummary, FindingRequirementView, FindingView, HistoryCursor,
    MilestoneView, Page, PoamAssigneeCatalog, PoamAssigneeGroup, PoamAssigneePerson, PoamDetail,
    PoamListQuery, PoamRegisterSummary, PoamSummary, RegisterSystemScope, Rollup,
    VerificationAttemptView, VerificationItemView, WaiverListQuery, WaiverView,
};

const SUMMARY_COLUMNS: &str = r#"
    p.id, 'POAM-' || lpad(p.human_number::text, 4, '0') AS human_id,
    p.title, p.plan, p.owner, poam_assignee_view(p) AS assignee, p.target_date, p.risk, p.status, p.revision,
    COALESCE(p.status <> 'completed' AND p.target_date < $1, FALSE) AS overdue,
    (SELECT COUNT(DISTINCT l.finding_id) FROM poam_finding_links l
      WHERE l.poam_id = p.id AND ((p.status <> 'completed' AND l.retired_at IS NULL)
        OR (p.status = 'completed' AND l.retirement_reason='closed:'||p.closure_attempt_id::text))) AS finding_count,
    (SELECT COUNT(DISTINCT l.cve_finding_id) FROM poam_cve_finding_links l
      WHERE l.poam_id = p.id AND ((p.status <> 'completed' AND l.retired_at IS NULL)
        OR (p.status = 'completed' AND l.retirement_reason='closed:'||p.closure_attempt_id::text))) AS cve_finding_count,
    p.created_at, p.updated_at, p.closed_at, p.closure_attempt_id
"#;

const SUMMARY_COLUMNS_BEFORE_TODAY: &str = r#"
    p.id, 'POAM-' || lpad(p.human_number::text, 4, '0') AS human_id,
    p.title, p.plan, p.owner, poam_assignee_view(p) AS assignee, p.target_date, p.risk, p.status, p.revision,
    COALESCE(p.status <> 'completed' AND p.target_date <
"#;

const SUMMARY_COLUMNS_AFTER_TODAY: &str = r#", FALSE) AS overdue,
    (SELECT COUNT(DISTINCT l.finding_id) FROM poam_finding_links l
      WHERE l.poam_id = p.id AND ((p.status <> 'completed' AND l.retired_at IS NULL)
        OR (p.status = 'completed' AND l.retirement_reason='closed:'||p.closure_attempt_id::text))) AS finding_count,
    (SELECT COUNT(DISTINCT l.cve_finding_id) FROM poam_cve_finding_links l
      WHERE l.poam_id = p.id AND ((p.status <> 'completed' AND l.retired_at IS NULL)
        OR (p.status = 'completed' AND l.retirement_reason='closed:'||p.closure_attempt_id::text))) AS cve_finding_count,
    p.created_at, p.updated_at, p.closed_at, p.closure_attempt_id
"#;

#[derive(sqlx::FromRow)]
struct RelatedPoamSummary {
    relation_id: Uuid,
    relation_active: bool,
    id: Uuid,
    human_id: String,
    title: String,
    plan: String,
    owner: String,
    assignee: sqlx::types::Json<crate::models::poam::PoamAssigneeView>,
    target_date: Option<NaiveDate>,
    risk: String,
    status: String,
    revision: i64,
    overdue: bool,
    finding_count: i64,
    cve_finding_count: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
    closure_attempt_id: Option<Uuid>,
}

impl RelatedPoamSummary {
    fn into_parts(self) -> (Uuid, bool, PoamSummary) {
        (
            self.relation_id,
            self.relation_active,
            PoamSummary {
                id: self.id,
                human_id: self.human_id,
                title: self.title,
                plan: self.plan,
                owner: self.owner,
                assignee: self.assignee.0,
                target_date: self.target_date,
                risk: self.risk,
                status: self.status,
                revision: self.revision,
                overdue: self.overdue,
                finding_count: self.finding_count,
                cve_finding_count: self.cve_finding_count,
                created_at: self.created_at,
                updated_at: self.updated_at,
                closed_at: self.closed_at,
                closure_attempt_id: self.closure_attempt_id,
            },
        )
    }
}

/// Loads a bounded, deterministic catalog of eligible POA&M assignees.
///
/// The query intentionally excludes roles, environment memberships, claims,
/// and all other authorization data.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode either query.
pub async fn assignee_catalog(pool: &PgPool, limit: i64) -> Result<PoamAssigneeCatalog> {
    let people = sqlx::query_as::<_, PoamAssigneePerson>(
        r#"SELECT id AS user_id,
                  COALESCE(NULLIF(btrim(concat_ws(' ',NULLIF(btrim(first_name),''),NULLIF(btrim(last_name),''))),''),
                           NULLIF(btrim(username),''),email) AS label
           FROM users
           WHERE is_active AND user_type='human'
           ORDER BY lower(COALESCE(NULLIF(btrim(concat_ws(' ',NULLIF(btrim(first_name),''),NULLIF(btrim(last_name),''))),''),
                                   NULLIF(btrim(username),''),email)),id
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let groups = sqlx::query_as::<_, PoamAssigneeGroup>(
        r#"SELECT group_name FROM oidc_group_mappings
           WHERE group_name=lower(btrim(group_name))
             AND octet_length(group_name)<=128
             AND group_name~'^[a-z0-9_.:/-]+$'
           ORDER BY group_name,id LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(PoamAssigneeCatalog { people, groups })
}

/// Loads visible assessment-to-finding identities in request order.
///
/// Missing or inaccessible assessments are omitted.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn visible_assessment_findings(
    pool: &PgPool,
    assessment_ids: &[Uuid],
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<Vec<(Uuid, Uuid, Uuid, Uuid)>> {
    Ok(sqlx::query_as(
        r#"SELECT assessment.id,finding.id,assessment.system_id,assessment.policy_lineage_id
           FROM UNNEST($1::uuid[]) WITH ORDINALITY requested(id,ordinal)
           JOIN composite_policy_assessments assessment ON assessment.id=requested.id
           JOIN poam_findings finding ON finding.system_id=assessment.system_id
             AND finding.policy_lineage_id=assessment.policy_lineage_id
           JOIN systems system ON system.id=assessment.system_id
           WHERE $2 OR system.environment_id=ANY($3)
           ORDER BY requested.ordinal"#,
    )
    .bind(assessment_ids)
    .bind(is_admin)
    .bind(environment_ids)
    .fetch_all(pool)
    .await?)
}

/// Loads visible stable finding identities in request order.
///
/// Missing or inaccessible findings are omitted.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn visible_findings(
    pool: &PgPool,
    finding_ids: &[Uuid],
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<Vec<(Uuid, Uuid, Uuid)>> {
    Ok(sqlx::query_as(
        r#"SELECT finding.id,finding.system_id,finding.policy_lineage_id
           FROM UNNEST($1::uuid[]) WITH ORDINALITY requested(id,ordinal)
           JOIN poam_findings finding ON finding.id=requested.id
           JOIN systems system ON system.id=finding.system_id
           WHERE $2 OR system.environment_id=ANY($3)
           ORDER BY requested.ordinal"#,
    )
    .bind(finding_ids)
    .bind(is_admin)
    .bind(environment_ids)
    .fetch_all(pool)
    .await?)
}

/// Loads active and historical POA&M summaries for visible findings.
///
/// Active relationships are always returned. When `history_page` is present,
/// each finding receives at most one extra historical row for continuation
/// detection beyond the requested page.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn finding_poam_summaries(
    pool: &PgPool,
    finding_ids: &[Uuid],
    today: NaiveDate,
    is_admin: bool,
    environment_ids: &[Uuid],
    history_page: Option<(i64, i64)>,
) -> Result<Vec<(Uuid, bool, PoamSummary)>> {
    let mut builder = QueryBuilder::<Postgres>::new(
        "WITH base AS (SELECT DISTINCT ON (link.finding_id,p.id) link.finding_id,p.id,(p.status<>'completed' AND link.retired_at IS NULL) AS relation_active,link.linked_at AS relationship_at,link.id AS relationship_id FROM poam_finding_links link JOIN poams p ON p.id=link.poam_id WHERE link.finding_id=ANY(",
    );
    builder
        .push_bind(finding_ids)
        .push(") AND (")
        .push_bind(is_admin)
        .push(" OR poam_visible_to_environments(p.id,")
        .push_bind(environment_ids)
        .push(")) ORDER BY link.finding_id,p.id,(p.status<>'completed' AND link.retired_at IS NULL) DESC,link.linked_at DESC,link.id DESC),related AS (SELECT base.*,COUNT(*) FILTER (WHERE NOT base.relation_active) OVER (PARTITION BY base.finding_id ORDER BY base.relationship_at DESC,base.relationship_id DESC ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS history_row FROM base) SELECT related.finding_id AS relation_id,related.relation_active,");
    builder
        .push(SUMMARY_COLUMNS_BEFORE_TODAY)
        .push_bind(today)
        .push(SUMMARY_COLUMNS_AFTER_TODAY)
        .push(" FROM related JOIN poams p ON p.id=related.id");
    if let Some((history_limit, history_offset)) = history_page {
        builder
            .push(" WHERE related.relation_active OR related.history_row BETWEEN ")
            .push_bind(history_offset + 1)
            .push(" AND ")
            .push_bind(history_offset + history_limit + 1);
    }
    builder.push(" ORDER BY related.finding_id,related.relation_active DESC,related.relationship_at DESC,related.relationship_id DESC");
    Ok(builder
        .build_query_as::<RelatedPoamSummary>()
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(RelatedPoamSummary::into_parts)
        .collect())
}

/// Loads active and historical POA&M summaries for exact-CVE findings.
///
/// Each finding receives at most one extra historical row for continuation
/// detection. Missing finding IDs produce no rows.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn cve_finding_poam_summaries(
    pool: &PgPool,
    finding_ids: &[Uuid],
    today: NaiveDate,
    is_admin: bool,
    environment_ids: &[Uuid],
    history_page: (i64, i64),
) -> Result<Vec<(Uuid, bool, PoamSummary)>> {
    let mut tx = pool.begin().await?;
    let summaries = cve_finding_poam_summaries_tx(
        &mut tx,
        finding_ids,
        today,
        is_admin,
        environment_ids,
        history_page,
    )
    .await?;
    tx.commit().await?;
    Ok(summaries)
}

/// Loads exact-CVE POA&M summaries inside the caller's transaction.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub(crate) async fn cve_finding_poam_summaries_tx(
    tx: &mut Transaction<'_, Postgres>,
    finding_ids: &[Uuid],
    today: NaiveDate,
    is_admin: bool,
    environment_ids: &[Uuid],
    history_page: (i64, i64),
) -> Result<Vec<(Uuid, bool, PoamSummary)>> {
    if finding_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut builder = QueryBuilder::<Postgres>::new(
        "WITH base AS (SELECT DISTINCT ON (link.cve_finding_id,p.id) link.cve_finding_id,p.id,(p.status<>'completed' AND link.retired_at IS NULL) AS relation_active,link.linked_at AS relationship_at,link.id AS relationship_id FROM poam_cve_finding_links link JOIN poams p ON p.id=link.poam_id WHERE link.cve_finding_id=ANY(",
    );
    builder
        .push_bind(finding_ids)
        .push(") AND (")
        .push_bind(is_admin)
        .push(" OR poam_visible_to_environments(p.id,")
        .push_bind(environment_ids)
        .push(")) ORDER BY link.cve_finding_id,p.id,(p.status<>'completed' AND link.retired_at IS NULL) DESC,link.linked_at DESC,link.id DESC),related AS (SELECT base.*,COUNT(*) FILTER (WHERE NOT base.relation_active) OVER (PARTITION BY base.cve_finding_id ORDER BY base.relationship_at DESC,base.relationship_id DESC ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS history_row FROM base) SELECT related.cve_finding_id AS relation_id,related.relation_active,")
        .push(SUMMARY_COLUMNS_BEFORE_TODAY)
        .push_bind(today)
        .push(SUMMARY_COLUMNS_AFTER_TODAY)
        .push(" FROM related JOIN poams p ON p.id=related.id WHERE related.relation_active OR related.history_row BETWEEN ")
        .push_bind(history_page.1 + 1)
        .push(" AND ")
        .push_bind(history_page.1 + history_page.0 + 1)
        .push(" ORDER BY related.cve_finding_id,related.relation_active DESC,related.relationship_at DESC,related.relationship_id DESC");
    Ok(builder
        .build_query_as::<RelatedPoamSummary>()
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(RelatedPoamSummary::into_parts)
        .collect())
}

/// Loads visible active POA&Ms compatible with a finding policy lineage.
///
/// Results exclude POA&Ms already related to the finding and include one extra
/// row for continuation detection.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn compatible_poams(
    pool: &PgPool,
    finding_id: Uuid,
    policy_lineage_id: Uuid,
    q: Option<&str>,
    today: NaiveDate,
    limit: i64,
    offset: i64,
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<Vec<PoamSummary>> {
    let mut builder = QueryBuilder::<Postgres>::new("SELECT ");
    builder
        .push(SUMMARY_COLUMNS_BEFORE_TODAY)
        .push_bind(today)
        .push(SUMMARY_COLUMNS_AFTER_TODAY)
        .push(" FROM poams p WHERE p.status<>'completed' AND (")
        .push_bind(is_admin)
        .push(" OR poam_visible_to_environments(p.id,")
        .push_bind(environment_ids)
        .push(")) AND EXISTS (SELECT 1 FROM poam_finding_links link JOIN poam_findings finding ON finding.id=link.finding_id WHERE link.poam_id=p.id AND link.retired_at IS NULL AND finding.policy_lineage_id=")
        .push_bind(policy_lineage_id)
        .push(") AND (SELECT COUNT(*) FROM poam_finding_links link WHERE link.poam_id=p.id AND link.retired_at IS NULL)<100")
        .push(" AND NOT EXISTS (SELECT 1 FROM poam_finding_links link WHERE link.poam_id=p.id AND link.finding_id=")
        .push_bind(finding_id)
        .push(") AND NOT EXISTS (SELECT 1 FROM poam_finding_links link WHERE link.finding_id=")
        .push_bind(finding_id)
        .push(" AND link.retired_at IS NULL)");
    if let Some(q) = q {
        builder
            .push(" AND (p.title ILIKE ")
            .push_bind(format!("%{q}%"))
            .push(" OR p.owner ILIKE ")
            .push_bind(format!("%{q}%"))
            .push(" OR ('POAM-'||lpad(p.human_number::text,4,'0')) ILIKE ")
            .push_bind(format!("%{q}%"))
            .push(")");
    }
    builder
        .push(" ORDER BY p.updated_at DESC,p.id LIMIT ")
        .push_bind(limit + 1)
        .push(" OFFSET ")
        .push_bind(offset);
    Ok(builder.build_query_as().fetch_all(pool).await?)
}

/// Loads visible immutable assignment-version IDs in request order.
///
/// Missing or inaccessible versions are omitted.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn visible_assignment_versions(
    pool: &PgPool,
    assignment_version_ids: &[Uuid],
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        r#"SELECT version.id
           FROM UNNEST($1::uuid[]) WITH ORDINALITY requested(id,ordinal)
           JOIN compliance_bundle_assignment_versions version ON version.id=requested.id
           JOIN compliance_bundle_assignments assignment ON assignment.id=version.assignment_id
           LEFT JOIN systems system ON system.id=assignment.system_id
           WHERE $2 OR COALESCE(assignment.environment_id,system.environment_id)=ANY($3)
           ORDER BY requested.ordinal"#,
    )
    .bind(assignment_version_ids)
    .bind(is_admin)
    .bind(environment_ids)
    .fetch_all(pool)
    .await?)
}

/// Loads POA&M summaries related to visible assignment versions.
///
/// When `history_page` is present, each assignment receives at most one extra
/// row for continuation detection beyond the requested page.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn assignment_poam_summaries(
    pool: &PgPool,
    assignment_version_ids: &[Uuid],
    today: NaiveDate,
    is_admin: bool,
    environment_ids: &[Uuid],
    history_page: Option<(i64, i64)>,
) -> Result<Vec<(Uuid, PoamSummary)>> {
    let mut builder = QueryBuilder::<Postgres>::new(
        "WITH related AS (SELECT reference.assignment_version_id,p.id,reference.added_at AS relationship_at,ROW_NUMBER() OVER (PARTITION BY reference.assignment_version_id ORDER BY reference.added_at DESC,reference.poam_id DESC) AS history_row FROM poam_assignment_references reference JOIN poams p ON p.id=reference.poam_id WHERE reference.assignment_version_id=ANY(",
    );
    builder
        .push_bind(assignment_version_ids)
        .push(") AND (")
        .push_bind(is_admin)
        .push(" OR poam_visible_to_environments(p.id,")
        .push_bind(environment_ids)
        .push("))) SELECT related.assignment_version_id AS relation_id,false AS relation_active,");
    builder
        .push(SUMMARY_COLUMNS_BEFORE_TODAY)
        .push_bind(today)
        .push(SUMMARY_COLUMNS_AFTER_TODAY)
        .push(" FROM related JOIN poams p ON p.id=related.id");
    if let Some((history_limit, history_offset)) = history_page {
        builder
            .push(" WHERE related.history_row BETWEEN ")
            .push_bind(history_offset + 1)
            .push(" AND ")
            .push_bind(history_offset + history_limit + 1);
    }
    builder.push(
        " ORDER BY related.assignment_version_id,related.relationship_at DESC,related.id DESC",
    );
    Ok(builder
        .build_query_as::<RelatedPoamSummary>()
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            let (id, _, summary) = row.into_parts();
            (id, summary)
        })
        .collect())
}

/// Loads environment IDs assigned to a user.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn user_environment_ids(pool: &PgPool, user_id: Uuid) -> Result<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT environment_id FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

/// Returns whether a POA&M exists and is visible in the supplied actor scope.
///
/// Administrators are checked only for resource existence.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn poam_visible(
    pool: &PgPool,
    poam_id: Uuid,
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<bool> {
    if is_admin {
        return Ok(
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM poams WHERE id = $1)")
                .bind(poam_id)
                .fetch_one(pool)
                .await?,
        );
    }
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM poams WHERE id=$1) AND poam_visible_to_environments($1,$2)",
    )
    .bind(poam_id)
    .bind(environment_ids)
    .fetch_one(pool)
    .await?)
}

/// Loads one page of visible POA&M summaries.
///
/// The query applies persistence-native filters and returns one extra row only
/// internally to derive continuation metadata.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn list(
    pool: &PgPool,
    query: &PoamListQuery,
    today: NaiveDate,
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<Page<PoamSummary>> {
    let mut tx = pool.begin().await?;
    let page = list_tx(&mut tx, query, today, is_admin, environment_ids).await?;
    tx.commit().await?;
    Ok(page)
}

/// Loads one visible POA&M page inside the caller's transaction.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn list_tx(
    tx: &mut Transaction<'_, Postgres>,
    query: &PoamListQuery,
    today: NaiveDate,
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<Page<PoamSummary>> {
    let limit = query.limit.unwrap_or(25).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);
    let mut builder = QueryBuilder::<Postgres>::new("SELECT ");
    builder
        .push(SUMMARY_COLUMNS_BEFORE_TODAY)
        .push_bind(today)
        .push(SUMMARY_COLUMNS_AFTER_TODAY)
        .push(" FROM poams p WHERE TRUE ");
    builder
        .push(" AND (")
        .push_bind(is_admin)
        .push(" OR poam_visible_to_environments(p.id,")
        .push_bind(environment_ids)
        .push(")) ");
    if let Some(status) = query.status.as_deref() {
        if status == "active" {
            builder.push(" AND p.status <> 'completed' ");
        } else {
            builder.push(" AND p.status = ").push_bind(status);
        }
    }
    if let Some(risk) = query.risk.as_deref() {
        builder.push(" AND p.risk = ").push_bind(risk);
    }
    if let Some(owner) = query.owner.as_deref() {
        builder
            .push(" AND p.owner ILIKE ")
            .push_bind(format!("%{owner}%"));
    }
    if query.overdue == Some(true) {
        builder
            .push(" AND p.status <> 'completed' AND p.target_date < ")
            .push_bind(today);
    } else if query.overdue == Some(false) {
        builder
            .push(" AND NOT COALESCE(p.status <> 'completed' AND p.target_date < ")
            .push_bind(today)
            .push(", FALSE)");
    }
    if let Some(q) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        builder
            .push(" AND (to_tsvector('simple',coalesce(p.title,'')||' '||coalesce(p.plan,'')||' '||coalesce(p.owner,'')) @@ plainto_tsquery('simple',")
            .push_bind(q)
            .push(")")
            .push(" OR ('POAM-' || lpad(p.human_number::text, 4, '0')) ILIKE ")
            .push_bind(format!("%{q}%"))
            // SECURITY: Historical links cannot search a moved host's current
            // B hostname (or other hidden current-system facts) for an A-only
            // actor, even when A's scheduled POA&M remains visible.
            .push(" OR EXISTS (SELECT 1 FROM poam_cve_finding_links cve_link JOIN poam_cve_findings cve_finding ON cve_finding.id=cve_link.cve_finding_id JOIN systems cve_system ON cve_system.id=cve_finding.system_id WHERE cve_link.poam_id=p.id AND (")
            .push_bind(is_admin)
            .push(" OR cve_system.environment_id=ANY(")
            .push_bind(environment_ids)
            .push(")) AND (cve_finding.canonical_cve_id ILIKE ")
            .push_bind(format!("%{q}%"))
            .push(" OR cve_finding.canonical_package_name ILIKE ")
            .push_bind(format!("%{q}%"))
            .push(" OR cve_system.hostname ILIKE ")
            .push_bind(format!("%{q}%"))
            .push("))")
            .push(")");
    }
    if let Some(system_id) = query.system_id {
        builder.push(" AND EXISTS (SELECT 1 FROM poam_context_systems context WHERE context.poam_id=p.id AND context.system_id=").push_bind(system_id).push(")");
    }
    if let Some(policy_id) = query.policy_lineage_id {
        builder.push(" AND EXISTS (SELECT 1 FROM poam_current_finding_links l JOIN poam_findings f ON f.id=l.finding_id WHERE l.poam_id=p.id AND f.policy_lineage_id=").push_bind(policy_id).push(")");
    }
    if let Some(bundle_id) = query.bundle_id {
        builder.push(" AND (EXISTS (SELECT 1 FROM poam_assignment_references reference JOIN compliance_bundle_assignment_versions assignment_version ON assignment_version.id=reference.assignment_version_id JOIN compliance_bundle_versions bundle_version ON bundle_version.id=assignment_version.bundle_version_id WHERE reference.poam_id=p.id AND bundle_version.bundle_id=")
            .push_bind(bundle_id)
            .push(") OR EXISTS (SELECT 1 FROM poam_current_finding_links link JOIN poam_findings finding ON finding.id=link.finding_id JOIN systems system ON system.id=finding.system_id JOIN compliance_bundle_assignments assignment ON assignment.active AND (assignment.system_id=system.id OR assignment.environment_id=system.environment_id) JOIN compliance_bundle_assignment_versions assignment_version ON assignment_version.id=assignment.current_version_id JOIN compliance_bundle_versions bundle_version ON bundle_version.id=assignment_version.bundle_version_id WHERE link.poam_id=p.id AND bundle_version.bundle_id=")
            .push_bind(bundle_id)
            .push(" AND (EXISTS (SELECT 1 FROM compliance_assignment_additions addition JOIN deployment_policy_versions policy_version ON policy_version.id=addition.policy_version_id WHERE addition.assignment_version_id=assignment_version.id AND policy_version.policy_id=finding.policy_lineage_id) OR EXISTS (SELECT 1 FROM compliance_bundle_version_policies membership JOIN deployment_policy_versions policy_version ON policy_version.id=membership.policy_version_id WHERE membership.bundle_version_id=assignment_version.bundle_version_id AND membership.selected AND policy_version.policy_id=finding.policy_lineage_id AND NOT EXISTS (SELECT 1 FROM compliance_assignment_exclusions exclusion WHERE exclusion.assignment_version_id=assignment_version.id AND exclusion.policy_version_id=membership.policy_version_id)))) OR EXISTS (SELECT 1 FROM poam_verification_items item WHERE item.attempt_id=p.closure_attempt_id AND ")
            .push_bind(bundle_id)
            .push("=ANY(item.bundle_ids)))");
    }
    if let Some(requirement) = query.requirement.as_deref() {
        let pattern = format!("%{}%", requirement.trim());
        builder.push(" AND (EXISTS (SELECT 1 FROM poam_current_finding_links link JOIN poam_findings finding ON finding.id=link.finding_id JOIN composite_policy_assessments assessment ON assessment.system_id=finding.system_id AND assessment.policy_lineage_id=finding.policy_lineage_id JOIN policy_requirement_mappings mapping ON mapping.policy_version_id=assessment.policy_version_id JOIN compliance_requirement_versions requirement ON requirement.id=mapping.requirement_version_id WHERE link.poam_id=p.id AND (requirement.external_id ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR requirement.title ILIKE ")
            .push_bind(pattern.clone())
            .push(")) OR EXISTS (SELECT 1 FROM poam_verification_items item JOIN compliance_requirement_versions requirement ON requirement.id=ANY(item.requirement_version_ids) WHERE item.attempt_id=p.closure_attempt_id AND (requirement.external_id ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR requirement.title ILIKE ")
            .push_bind(pattern)
            .push(")))");
    }
    builder
        .push(" ORDER BY p.updated_at DESC, p.id LIMIT ")
        .push_bind(limit + 1)
        .push(" OFFSET ")
        .push_bind(offset);
    let mut items = builder
        .build_query_as::<PoamSummary>()
        .fetch_all(&mut **tx)
        .await?;
    let has_more = items.len() as i64 > limit;
    if has_more {
        items.truncate(limit as usize);
    }
    Ok(Page {
        items,
        limit,
        offset,
        has_more,
        next_offset: has_more.then_some(offset + limit),
    })
}

#[derive(sqlx::FromRow)]
struct RegisterContext {
    poam_id: Uuid,
    environment_ids: Vec<Uuid>,
    system_ids: Vec<Uuid>,
    systems: sqlx::types::Json<Vec<RegisterSystemScope>>,
    bundle_ids: Vec<Uuid>,
    bundle_version_ids: Vec<Uuid>,
    assignment_version_ids: Vec<Uuid>,
    first_requirement: Option<String>,
    first_cve: Option<String>,
    milestone_count: i64,
    completed_milestone_count: i64,
    last_activity_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Hydrates one already-authorized list page in a single bounded SQL query.
///
/// The visibility predicate is repeated at the query boundary so a changed
/// actor scope cannot expose context from a POA&M that is no longer visible.
/// Retired moved-host links never supply current system/environment names.
///
/// # Errors
///
/// Returns a database error if the register context cannot be loaded.
pub async fn register_page(
    pool: &PgPool,
    page: Page<PoamSummary>,
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<Page<PoamRegisterSummary>> {
    let mut tx = pool.begin().await?;
    let page = register_page_tx(&mut tx, page, is_admin, environment_ids).await?;
    tx.commit().await?;
    Ok(page)
}

/// Hydrates a register page in the caller's transaction and actor scope.
///
/// # Errors
///
/// Returns a database error if context cannot be loaded or visibility changed.
pub async fn register_page_tx(
    tx: &mut Transaction<'_, Postgres>,
    page: Page<PoamSummary>,
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<Page<PoamRegisterSummary>> {
    let ids = page.items.iter().map(|item| item.id).collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(Page {
            items: Vec::new(),
            limit: page.limit,
            offset: page.offset,
            has_more: page.has_more,
            next_offset: page.next_offset,
        });
    }
    let contexts = sqlx::query_as::<_, RegisterContext>(r#"
        SELECT p.id AS poam_id,
          ARRAY(SELECT DISTINCT s.environment_id FROM poam_context_systems c
            JOIN systems s ON s.id=c.system_id
            WHERE c.poam_id=p.id AND s.environment_id IS NOT NULL
              AND ($2 OR s.environment_id=ANY($3))
            UNION SELECT d.environment_id FROM cve_current_environment_dispositions d
            WHERE d.poam_id=p.id AND d.state='scheduled' AND p.status<>'completed'
              AND ($2 OR d.environment_id=ANY($3))) AS environment_ids,
           ARRAY(SELECT DISTINCT c.system_id FROM poam_context_systems c
             JOIN systems s ON s.id=c.system_id WHERE c.poam_id=p.id
               AND ($2 OR s.environment_id=ANY($3))) AS system_ids,
           (SELECT COALESCE(jsonb_agg(jsonb_build_object(
               'system_id', scoped.id, 'hostname', scoped.hostname,
               'environment_id', scoped.environment_id) ORDER BY scoped.id), '[]'::jsonb)
            FROM (SELECT DISTINCT s.id, s.hostname, s.environment_id
              FROM poam_context_systems c JOIN systems s ON s.id=c.system_id
              WHERE c.poam_id=p.id AND ($2 OR s.environment_id=ANY($3))) scoped) AS systems,
          ARRAY(SELECT DISTINCT a.bundle_id FROM poam_assignment_references r
            JOIN compliance_bundle_assignments a ON a.id=r.assignment_id
            LEFT JOIN systems s ON s.id=a.system_id
            WHERE r.poam_id=p.id AND ($2 OR COALESCE(a.environment_id,s.environment_id)=ANY($3))
            UNION SELECT bv.bundle_id FROM poam_current_finding_links link
            JOIN poam_findings finding ON finding.id=link.finding_id
            JOIN systems s ON s.id=finding.system_id
            JOIN compliance_bundle_assignments a ON a.active AND
              (a.system_id=s.id OR a.environment_id=s.environment_id)
            JOIN compliance_bundle_assignment_versions av ON av.id=a.current_version_id
            JOIN compliance_bundle_versions bv ON bv.id=av.bundle_version_id
            WHERE link.poam_id=p.id AND ($2 OR s.environment_id=ANY($3)) AND
              (EXISTS (SELECT 1 FROM compliance_assignment_additions addition
                JOIN deployment_policy_versions pv ON pv.id=addition.policy_version_id
                WHERE addition.assignment_version_id=av.id
                  AND pv.policy_id=finding.policy_lineage_id)
               OR EXISTS (SELECT 1 FROM compliance_bundle_version_policies membership
                JOIN deployment_policy_versions pv ON pv.id=membership.policy_version_id
                WHERE membership.bundle_version_id=av.bundle_version_id
                  AND membership.selected AND pv.policy_id=finding.policy_lineage_id
                  AND NOT EXISTS (SELECT 1 FROM compliance_assignment_exclusions exclusion
                    WHERE exclusion.assignment_version_id=av.id
                      AND exclusion.policy_version_id=membership.policy_version_id)))
            UNION SELECT unnest(item.bundle_ids) FROM poam_verification_items item
            JOIN systems s ON s.id=item.system_id
            WHERE item.attempt_id=p.closure_attempt_id
              AND ($2 OR s.environment_id=ANY($3))) AS bundle_ids,
          ARRAY(SELECT DISTINCT v.bundle_version_id FROM poam_assignment_references r
            JOIN compliance_bundle_assignment_versions v ON v.id=r.assignment_version_id
            JOIN compliance_bundle_assignments a ON a.id=r.assignment_id
            LEFT JOIN systems s ON s.id=a.system_id
            WHERE r.poam_id=p.id AND ($2 OR COALESCE(a.environment_id,s.environment_id)=ANY($3))) AS bundle_version_ids,
          ARRAY(SELECT r.assignment_version_id FROM poam_assignment_references r
            JOIN compliance_bundle_assignments a ON a.id=r.assignment_id
            LEFT JOIN systems s ON s.id=a.system_id
            WHERE r.poam_id=p.id AND ($2 OR COALESCE(a.environment_id,s.environment_id)=ANY($3))
            ORDER BY r.assignment_version_id) AS assignment_version_ids,
          (SELECT source.external_id FROM (
            SELECT requirement.external_id FROM poam_verification_items item
            JOIN systems s ON s.id=item.system_id
            JOIN compliance_requirement_versions requirement ON requirement.id=ANY(item.requirement_version_ids)
            WHERE item.attempt_id=p.closure_attempt_id AND ($2 OR s.environment_id=ANY($3))
            UNION
            SELECT requirement.external_id FROM poam_current_finding_links link
            JOIN poam_findings f ON f.id=link.finding_id
            JOIN systems s ON s.id=f.system_id
            JOIN composite_policy_assessments assessment ON assessment.system_id=f.system_id
              AND assessment.policy_lineage_id=f.policy_lineage_id
            JOIN policy_requirement_mappings mapping ON mapping.policy_version_id=assessment.policy_version_id
            JOIN compliance_requirement_versions requirement ON requirement.id=mapping.requirement_version_id
            WHERE p.status<>'completed' AND link.poam_id=p.id AND ($2 OR s.environment_id=ANY($3))
          ) source ORDER BY source.external_id LIMIT 1) AS first_requirement,
          COALESCE((SELECT f.canonical_cve_id FROM poam_current_cve_finding_links link
            JOIN poam_cve_findings f ON f.id=link.cve_finding_id
            JOIN systems s ON s.id=f.system_id
            WHERE link.poam_id=p.id AND ($2 OR s.environment_id=ANY($3))
            ORDER BY f.canonical_cve_id,f.id LIMIT 1),
            (SELECT d.canonical_cve_id FROM cve_current_environment_dispositions d
             WHERE d.poam_id=p.id AND d.state='scheduled' AND p.status<>'completed'
               AND ($2 OR d.environment_id=ANY($3))
             ORDER BY d.canonical_cve_id LIMIT 1)) AS first_cve,
          (SELECT count(*) FROM poam_milestones m WHERE m.poam_id=p.id) AS milestone_count,
          (SELECT count(*) FROM poam_milestones m WHERE m.poam_id=p.id AND m.completed_at IS NOT NULL) AS completed_milestone_count,
          (SELECT max(activity.created_at) FROM poam_activity activity
            WHERE activity.poam_id=p.id AND ($2 OR
              (activity.payload->>'finding_id' IS NULL AND activity.payload->>'cve_finding_id' IS NULL
               AND activity.payload#>>'{finding,finding_id}' IS NULL
               AND activity.payload#>>'{finding,cve_finding_id}' IS NULL
               AND activity.payload->'items' IS NULL AND activity.payload->'cve_items' IS NULL))) AS last_activity_at
        FROM poams p WHERE p.id=ANY($1) AND ($2 OR poam_visible_to_environments(p.id,$3))"#)
         .bind(&ids).bind(is_admin).bind(environment_ids).fetch_all(&mut **tx).await?;
    let contexts = contexts
        .into_iter()
        .map(|row| (row.poam_id, row))
        .collect::<std::collections::HashMap<_, _>>();
    // CONCURRENCY: In READ COMMITTED, scope can change between list and
    // hydration. Fail the page rather than return a stale summary or offset.
    anyhow::ensure!(
        contexts.len() == ids.len(),
        "register scope changed while loading the page; retry"
    );
    let items = page
        .items
        .into_iter()
        .map(|summary| {
            let row = &contexts[&summary.id];
            PoamRegisterSummary {
                summary,
                environment_ids: row.environment_ids.clone(),
                system_ids: row.system_ids.clone(),
                systems: row.systems.0.clone(),
                bundle_ids: row.bundle_ids.clone(),
                bundle_version_ids: row.bundle_version_ids.clone(),
                assignment_version_ids: row.assignment_version_ids.clone(),
                first_requirement: row.first_requirement.clone(),
                first_cve: row.first_cve.clone(),
                milestone_count: row.milestone_count,
                completed_milestone_count: row.completed_milestone_count,
                last_activity_at: row.last_activity_at,
            }
        })
        .collect();
    Ok(Page {
        items,
        limit: page.limit,
        offset: page.offset,
        has_more: page.has_more,
        next_offset: page.next_offset,
    })
}

#[cfg(test)]
mod register_tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires the verified isolated PG35457 test cluster"]
    async fn register_tx_matches_pool_filters_and_is_read_only(pool: PgPool) {
        use crate::services::poam::{PoamActor, SystemClock, list_register, list_register_tx};

        let user: Uuid =
            sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email) VALUES($1,'Register','Reader',$2) RETURNING id")
                .bind(format!("register-tx-{}", Uuid::new_v4()))
                .bind(format!("register-tx-{}@example.invalid", Uuid::new_v4()))
                .fetch_one(&pool)
                .await
                .unwrap();
        let a: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("register-tx-a-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .unwrap();
        let b: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("register-tx-b-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .unwrap();
        let policy: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies(name,policy_type,config,enabled) VALUES($1,'custom_check','{}',false) RETURNING id",
        )
        .bind(format!("register-tx-policy-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut visible_ids = Vec::new();
        for (index, environment) in [a, a, b].into_iter().enumerate() {
            let host = format!("register-tx-{index}-{}", Uuid::new_v4());
            let system: Uuid = sqlx::query_scalar(
                "INSERT INTO systems(hostname,public_key,derivation,environment_id) VALUES($1,$2,$2,$3) RETURNING id",
            )
            .bind(&host)
            .bind(format!("test-key-{host}"))
            .bind(environment)
            .fetch_one(&pool)
            .await
            .unwrap();
            let finding: Uuid = sqlx::query_scalar(
                "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
            )
            .bind(system)
            .bind(policy)
            .fetch_one(&pool)
            .await
            .unwrap();
            let mut tx = pool.begin().await.unwrap();
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO poams(title,risk,created_by) VALUES($1,'high',$2) RETURNING id",
            )
            .bind(format!("Register tx {index}"))
            .bind(user)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)",
            )
            .bind(id)
            .bind(finding)
            .bind(user)
            .execute(&mut *tx)
            .await
            .unwrap();
            tx.commit().await.unwrap();
            if environment == a {
                visible_ids.push(id);
            }
        }
        let actor = PoamActor {
            user_id: user,
            identifier: "register-tx".into(),
            is_admin: false,
            can_mutate: false,
            environment_ids: vec![a],
            request_origin: None,
        };
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await
            .unwrap();
        for offset in [0, 1] {
            let query = PoamListQuery {
                policy_lineage_id: Some(policy),
                risk: Some("high".into()),
                q: Some(" Register tx ".into()),
                limit: Some(1),
                offset: Some(offset),
                ..Default::default()
            };
            let expected = list_register(&pool, &actor, &query, &SystemClock)
                .await
                .unwrap();
            let actual = list_register_tx(&mut tx, &actor, &query, &SystemClock)
                .await
                .unwrap();
            assert_eq!(
                serde_json::to_value(&actual).unwrap(),
                serde_json::to_value(&expected).unwrap()
            );
            assert_eq!(actual.items.len(), 1);
            assert!(visible_ids.contains(&actual.items[0].summary.id));
            assert_eq!(actual.items[0].environment_ids, vec![a]);
            assert_eq!(actual.has_more, offset == 0);
        }
        tx.commit().await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires the verified isolated PG35457 test cluster"]
    async fn register_scopes_and_pages_past_one_hundred(pool: PgPool) {
        let user: Uuid = sqlx::query_scalar(
            "INSERT INTO users(username,first_name,last_name,email) VALUES($1,'Register','Reader',$2) RETURNING id",
        )
        .bind(format!("register-{}", Uuid::new_v4()))
        .bind(format!("register-{}@example.invalid", Uuid::new_v4()))
        .fetch_one(&pool).await.unwrap();
        let a: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("register-a-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .unwrap();
        let b: Uuid = sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
            .bind(format!("register-b-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .unwrap();
        let policy: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies(name,policy_type,config,enabled) VALUES($1,'custom_check','{}',false) RETURNING id",
        )
        .bind(format!("register-policy-{}", Uuid::new_v4()))
        .fetch_one(&pool).await.unwrap();
        let mut visible_ids = Vec::new();
        let mut hidden_id = Uuid::nil();
        for n in 0..106 {
            let environment = if n == 105 { b } else { a };
            let host = format!("register-{n}-{}", Uuid::new_v4());
            let system: Uuid = sqlx::query_scalar(
                "INSERT INTO systems(hostname,public_key,derivation,environment_id) VALUES($1,$2,$2,$3) RETURNING id",
            ).bind(&host).bind(format!("test-key-{host}"))
                .bind(environment).fetch_one(&pool).await.unwrap();
            let finding: Uuid = sqlx::query_scalar(
                "INSERT INTO poam_findings(system_id,policy_lineage_id) VALUES($1,$2) RETURNING id",
            )
            .bind(system)
            .bind(policy)
            .fetch_one(&pool)
            .await
            .unwrap();
            let mut tx = pool.begin().await.unwrap();
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO poams(title,risk,created_by) VALUES($1,'high',$2) RETURNING id",
            )
            .bind(format!("Register {n}"))
            .bind(user)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)",
            )
            .bind(id)
            .bind(finding)
            .bind(user)
            .execute(&mut *tx)
            .await
            .unwrap();
            if n == 104 {
                sqlx::query("INSERT INTO poam_milestones(poam_id,ordinal,title,target_date,created_by,updated_by) VALUES($1,0,'Validate',CURRENT_DATE,$2,$2)")
                    .bind(id).bind(user).execute(&mut *tx).await.unwrap();
                sqlx::query("INSERT INTO poam_activity(poam_id,actor_user_id,kind,payload) VALUES($1,$2,'created','{}')")
                    .bind(id).bind(user).execute(&mut *tx).await.unwrap();
            }
            tx.commit().await.unwrap();
            if n == 105 {
                hidden_id = id;
            } else {
                visible_ids.push((id, system));
            }
        }
        let query = PoamListQuery {
            limit: Some(100),
            ..Default::default()
        };
        let first = list(&pool, &query, chrono::Utc::now().date_naive(), false, &[a])
            .await
            .unwrap();
        assert_eq!(first.items.len(), 100);
        assert!(first.has_more);
        let first = register_page(&pool, first, false, &[a]).await.unwrap();
        assert_eq!(first.items.len(), 100);
        assert!(
            first
                .items
                .iter()
                .all(|item| item.environment_ids == vec![a]
                    && item.system_ids.len() == 1
                    && item.bundle_version_ids.is_empty()
                    && item.assignment_version_ids.is_empty()
                    && item.first_cve.is_none())
        );
        let milestone = first
            .items
            .iter()
            .find(|item| item.summary.id == visible_ids[104].0)
            .unwrap();
        let hostname: String = sqlx::query_scalar("SELECT hostname FROM systems WHERE id=$1")
            .bind(visible_ids[104].1)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(milestone.systems.len(), 1);
        assert_eq!(milestone.systems[0].system_id, visible_ids[104].1);
        assert_eq!(milestone.systems[0].hostname, hostname);
        assert_eq!(milestone.systems[0].environment_id, Some(a));
        assert_eq!(milestone.milestone_count, 1);
        assert_eq!(milestone.completed_milestone_count, 0);
        assert!(milestone.last_activity_at.is_some());
        let second = list(
            &pool,
            &PoamListQuery {
                offset: Some(100),
                ..query
            },
            chrono::Utc::now().date_naive(),
            false,
            &[a],
        )
        .await
        .unwrap();
        assert_eq!(second.items.len(), 5);
        assert!(!second.has_more);
        let second = register_page(&pool, second, false, &[a]).await.unwrap();
        assert_eq!(second.items.len(), 5);
        assert!(second.items.iter().all(|item| {
            visible_ids
                .iter()
                .any(|(id, system)| *id == item.summary.id && item.system_ids == vec![*system])
        }));
        let admin = list(
            &pool,
            &PoamListQuery {
                limit: Some(100),
                offset: Some(100),
                ..Default::default()
            },
            chrono::Utc::now().date_naive(),
            true,
            &[],
        )
        .await
        .unwrap();
        assert_eq!(admin.items.len(), 6);
        let hidden = list(
            &pool,
            &PoamListQuery {
                q: Some("Register 105".into()),
                ..Default::default()
            },
            chrono::Utc::now().date_naive(),
            false,
            &[a],
        )
        .await
        .unwrap();
        assert!(hidden.items.is_empty());
        let admin = register_page(&pool, admin, true, &[]).await.unwrap();
        assert_eq!(admin.items.len(), 6);
        let hidden_admin = list(
            &pool,
            &PoamListQuery {
                q: Some("Register 105".into()),
                ..Default::default()
            },
            chrono::Utc::now().date_naive(),
            true,
            &[],
        )
        .await
        .unwrap();
        let hidden_admin = register_page(&pool, hidden_admin, true, &[]).await.unwrap();
        assert_eq!(hidden_admin.items.len(), 1);
        assert_eq!(hidden_admin.items[0].summary.id, hidden_id);
        assert_eq!(hidden_admin.items[0].environment_ids, vec![b]);
        assert_eq!(hidden_admin.items[0].systems.len(), 1);
        assert_eq!(hidden_admin.items[0].systems[0].environment_id, Some(b));
    }
}

/// Loads display metadata for immutable requirement versions in one batch.
///
/// Missing requirement versions are omitted. Callers retain the original UUID
/// arrays as the compatibility and identity contract.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn finding_requirement_metadata(
    tx: &mut Transaction<'_, Postgres>,
    requirement_version_ids: &[Uuid],
) -> Result<Vec<FindingRequirementView>> {
    sqlx::query_as::<_, FindingRequirementView>(
        r#"SELECT requirement.id AS requirement_version_id,
                  requirement.external_id,requirement.title,
                  framework.id AS framework_id,framework.name AS framework_name,
                  framework_version.id AS framework_version_id,
                  framework_version.version AS framework_version,
                  framework_version.title AS framework_title
           FROM compliance_requirement_versions requirement
           JOIN compliance_framework_versions framework_version
             ON framework_version.id=requirement.framework_version_id
           JOIN compliance_frameworks framework
             ON framework.id=framework_version.framework_id
           WHERE requirement.id=ANY($1)
           ORDER BY framework.name,framework_version.version,requirement.external_id,requirement.id"#,
    )
    .bind(requirement_version_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(Into::into)
}

/// Loads a visible POA&M and its bounded history collections.
///
/// Returns `None` only when the POA&M row is absent. The service layer checks
/// resource visibility and refreshes current evidence.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode any detail query.
pub async fn detail(
    tx: &mut Transaction<'_, Postgres>,
    poam_id: Uuid,
    today: NaiveDate,
    is_admin: bool,
    environment_ids: &[Uuid],
    finding_limit: i64,
    finding_before_at: Option<chrono::DateTime<chrono::Utc>>,
    finding_before_id: Option<Uuid>,
    activity_limit: i64,
    activity_before_at: Option<chrono::DateTime<chrono::Utc>>,
    activity_before_id: Option<Uuid>,
    verification_limit: i64,
    verification_before_at: Option<chrono::DateTime<chrono::Utc>>,
    verification_before_id: Option<Uuid>,
) -> Result<Option<PoamDetail>> {
    let sql = format!("SELECT {SUMMARY_COLUMNS} FROM poams p WHERE p.id = $2");
    let Some(poam) = sqlx::query_as::<_, PoamSummary>(&sql)
        .bind(today)
        .bind(poam_id)
        .fetch_optional(&mut **tx)
        .await?
    else {
        return Ok(None);
    };
    // PAGINATION: Select one keyset page across both finding families before
    // hydrating either response array. Otherwise each family can consume the
    // full limit and the shared cursor can skip or repeat the other family.
    let finding_page = sqlx::query_as::<_, (String, Uuid, chrono::DateTime<chrono::Utc>)>(
        r#"
        WITH visible_findings AS (
          SELECT 'policy'::text AS family,l.id AS link_id,l.linked_at
          FROM poam_finding_links l
          JOIN poam_findings finding ON finding.id=l.finding_id
          JOIN systems system ON system.id=finding.system_id
          WHERE l.poam_id=$1 AND ($2 OR system.environment_id=ANY($3))
          UNION ALL
          SELECT 'cve'::text AS family,link.id AS link_id,link.linked_at
           FROM poam_cve_finding_links link
           JOIN poam_cve_findings finding ON finding.id=link.cve_finding_id
           JOIN systems system ON system.id=finding.system_id
           -- SECURITY: The active A schedule owns moved link history, not the
           -- moved system's current B scope. Do not treat B as A context.
           WHERE link.poam_id=$1 AND ($2 OR system.environment_id=ANY($3)
             OR (link.retirement_reason='environment_moved'
               AND EXISTS (
                 SELECT 1 FROM cve_current_environment_dispositions disposition
                 JOIN poams parent ON parent.id=disposition.poam_id
                 WHERE disposition.poam_id=link.poam_id
                   AND disposition.state='scheduled' AND parent.status<>'completed'
                   AND disposition.environment_id=ANY($3)
                   AND disposition.canonical_cve_id=link.canonical_cve_id
                   AND disposition.canonical_package_name=link.canonical_package_name)))
        )
        SELECT family,link_id,linked_at FROM visible_findings
        WHERE ($5::timestamptz IS NULL OR (linked_at,link_id)<($5,$6))
        ORDER BY linked_at DESC,link_id DESC LIMIT $4"#,
    )
    .bind(poam_id)
    .bind(is_admin)
    .bind(environment_ids)
    .bind(finding_limit + 1)
    .bind(finding_before_at)
    .bind(finding_before_id)
    .fetch_all(&mut **tx)
    .await?;
    let findings_has_more = finding_page.len() as i64 > finding_limit;
    let finding_page = finding_page
        .into_iter()
        .take(finding_limit as usize)
        .collect::<Vec<_>>();
    let findings_next_cursor = findings_has_more
        .then(|| {
            finding_page
                .last()
                .map(|(_, id, at)| HistoryCursor { at: *at, id: *id })
        })
        .flatten();
    let policy_link_ids = finding_page
        .iter()
        .filter(|(family, _, _)| family == "policy")
        .map(|(_, id, _)| *id)
        .collect::<Vec<_>>();
    let cve_link_ids = finding_page
        .iter()
        .filter(|(family, _, _)| family == "cve")
        .map(|(_, id, _)| *id)
        .collect::<Vec<_>>();
    let findings = sqlx::query_as::<_, FindingView>(r#"
                SELECT f.id, f.system_id, s.hostname, s.environment_id, f.policy_lineage_id,
               policy.name AS policy_name, l.id AS link_id, l.linked_at,
               l.linked_by,l.retired_at,l.retired_by,l.retirement_reason,
               l.retired_at IS NULL AS link_active,
               closure_item.assessment_id AS current_assessment_id,
               closure_item.observed_outcome AS current_outcome,
               closure_item.policy_version_id AS current_policy_version_id,
               closure_item.target_store_path AS current_target_store_path,
               closure_item.assessment_updated_at,
               COALESCE(closure_item.result,'unknown') AS resolution_state,
               closure_item.effective_set_digest,closure_item.effective_config_digest,
               COALESCE(closure_item.bundle_ids,'{}'::uuid[]) AS bundle_ids,
               COALESCE(closure_item.bundle_version_ids,'{}'::uuid[]) AS bundle_version_ids,
                COALESCE(closure_item.requirement_version_ids,'{}'::uuid[]) AS requirement_version_ids,
                '[]'::jsonb AS requirements
        FROM poam_finding_links l JOIN poam_findings f ON f.id=l.finding_id
        JOIN systems s ON s.id=f.system_id JOIN deployment_policies policy ON policy.id=f.policy_lineage_id
        LEFT JOIN poam_verification_attempts closure_attempt
          ON l.retirement_reason='closed:'||closure_attempt.id::text AND closure_attempt.poam_id=l.poam_id
        LEFT JOIN poam_verification_items closure_item
          ON closure_item.attempt_id=closure_attempt.id AND closure_item.finding_id=l.finding_id
        WHERE l.poam_id=$1 AND ($2 OR s.environment_id=ANY($3))
          AND l.id=ANY($4)
        ORDER BY l.linked_at DESC,l.id DESC"#)
        .bind(poam_id).bind(is_admin).bind(environment_ids).bind(&policy_link_ids)
        .fetch_all(&mut **tx).await?;
    let cve_findings = sqlx::query_as::<_, CveFindingView>(
        r#"
         -- No link-time hostname snapshot exists. Keep the stable system ID
         -- and baseline audit, but do not expose the current B name or scope.
         SELECT finding.id,finding.system_id,
                CASE WHEN $2 OR system.environment_id=ANY($3)
                  THEN system.hostname ELSE '' END AS hostname,
                CASE WHEN $2 OR system.environment_id=ANY($3)
                  THEN system.environment_id ELSE NULL END AS environment_id,
               finding.canonical_cve_id,finding.canonical_package_name,
               link.id AS link_id,link.linked_at,link.linked_by,link.retired_at,
               link.retired_by,link.retirement_reason,
               link.retired_at IS NULL AS link_active,
               link.baseline_scan_id,link.baseline_scan_completed_at,
               link.baseline_generation_snapshot_id,
               link.baseline_generation,link.baseline_target_store_path,
               link.baseline_occurrence_derivation_path,
               link.baseline_observed_package_version,
               closure_item.scan_derivation_id AS current_derivation_id,
                closure_item.target_store_path AS current_target_store_path,
                closure_item.scan_id AS current_scan_id,
                closure_item.scan_completed_at AS current_scan_completed_at,
                closure_item.generation AS current_generation,
                closure_item.generation_snapshot_id AS current_generation_snapshot_id,
                closure_item.occurrence_derivation_path AS current_occurrence_derivation_path,
                closure_item.observed_package_version AS current_observed_package_version,
               COALESCE(closure_item.result,'unknown') AS resolution_state
        FROM poam_cve_finding_links link
        JOIN poam_cve_findings finding ON finding.id=link.cve_finding_id
        JOIN systems system ON system.id=finding.system_id
        LEFT JOIN poam_verification_attempts closure_attempt
          ON link.retirement_reason='closed:'||closure_attempt.id::text
         AND closure_attempt.poam_id=link.poam_id
        LEFT JOIN poam_cve_verification_items closure_item
          ON closure_item.attempt_id=closure_attempt.id
         AND closure_item.cve_finding_id=link.cve_finding_id
         WHERE link.poam_id=$1 AND link.id=ANY($4)
           AND ($2 OR system.environment_id=ANY($3)
             OR (link.retirement_reason='environment_moved'
               AND EXISTS (
                 SELECT 1 FROM cve_current_environment_dispositions disposition
                 JOIN poams parent ON parent.id=disposition.poam_id
                 WHERE disposition.poam_id=link.poam_id
                   AND disposition.state='scheduled' AND parent.status<>'completed'
                   AND disposition.environment_id=ANY($3)
                   AND disposition.canonical_cve_id=link.canonical_cve_id
                   AND disposition.canonical_package_name=link.canonical_package_name)))
        ORDER BY link.linked_at DESC,link.id DESC"#,
    )
    .bind(poam_id)
    .bind(is_admin)
    .bind(environment_ids)
    .bind(&cve_link_ids)
    .fetch_all(&mut **tx)
    .await?;
    let milestones = sqlx::query_as::<_, MilestoneView>(
        "SELECT id, ordinal, title, target_date, completed_at, completed_by, created_by, updated_by, created_at, updated_at FROM poam_milestones WHERE poam_id=$1 ORDER BY ordinal")
        .bind(poam_id).fetch_all(&mut **tx).await?;
    let assignment_references = sqlx::query_as::<_, AssignmentReferenceView>(
        r#"SELECT reference.assignment_id,reference.assignment_version_id,reference.added_by,reference.added_at,
          assignment.bundle_id,version.bundle_version_id,bundle.name AS bundle_name,
          bundle_version.version AS bundle_version,assignment.system_id,
          system.hostname AS system_hostname,assignment.environment_id,
          environment.name AS environment_name
          FROM poam_assignment_references reference
          JOIN compliance_bundle_assignment_versions version ON version.id=reference.assignment_version_id
          JOIN compliance_bundle_assignments assignment ON assignment.id=reference.assignment_id
          JOIN compliance_bundles bundle ON bundle.id=assignment.bundle_id
          JOIN compliance_bundle_versions bundle_version ON bundle_version.id=version.bundle_version_id
          LEFT JOIN systems system ON system.id=assignment.system_id
          LEFT JOIN environments environment ON environment.id=assignment.environment_id
           WHERE reference.poam_id=$1
             AND ($2 OR assignment.environment_id=ANY($3)
               OR system.environment_id=ANY($3))
           ORDER BY reference.added_at,reference.assignment_version_id"#)
        .bind(poam_id).bind(is_admin).bind(environment_ids).fetch_all(&mut **tx).await?;
    let attempt_rows = sqlx::query_as::<_, (Uuid,String,i64,Uuid,chrono::DateTime<chrono::Utc>)>(
        "SELECT id,outcome,poam_revision,attempted_by,attempted_at FROM poam_verification_attempts WHERE poam_id=$1 AND ($3::timestamptz IS NULL OR (attempted_at,id)<($3,$4)) ORDER BY attempted_at DESC,id DESC LIMIT $2")
        .bind(poam_id).bind(verification_limit + 1).bind(verification_before_at).bind(verification_before_id).fetch_all(&mut **tx).await?;
    let verification_has_more = attempt_rows.len() as i64 > verification_limit;
    let attempt_rows = attempt_rows
        .into_iter()
        .take(verification_limit as usize)
        .collect::<Vec<_>>();
    let verification_next_cursor = verification_has_more
        .then(|| {
            attempt_rows.last().map(|row| HistoryCursor {
                at: row.4,
                id: row.0,
            })
        })
        .flatten();
    let attempt_ids = attempt_rows.iter().map(|row| row.0).collect::<Vec<_>>();
    let verification_items=sqlx::query_as::<_,VerificationItemView>(r#"SELECT item.attempt_id,item.finding_id,item.system_id,
        COALESCE(item.system_hostname,system.hostname) AS hostname,item.policy_lineage_id,
        COALESCE(policy_version.name,policy.name) AS policy_name,policy_version.version AS policy_version,item.result,
        item.policy_version_id,item.assessment_id,item.derivation_id,item.target_store_path,item.effective_set_digest,item.effective_config_digest,
        item.effective_config,item.observed_outcome,item.observation_token,item.observation_snapshot,item.assessment_updated_at,item.bundle_ids,item.bundle_version_ids,
        item.requirement_version_ids,'[]'::jsonb AS requirements,item.waiver_id,item.observed_at,item.detail
        FROM poam_verification_items item
        JOIN systems system ON system.id=item.system_id
        JOIN deployment_policies policy ON policy.id=item.policy_lineage_id
        LEFT JOIN deployment_policy_versions policy_version ON policy_version.id=item.policy_version_id
        WHERE item.attempt_id=ANY($1) AND ($2 OR system.environment_id=ANY($3))
        ORDER BY item.finding_id"#)
        .bind(&attempt_ids).bind(is_admin).bind(environment_ids).fetch_all(&mut **tx).await?;
    let cve_verification_items = sqlx::query_as::<_, CveVerificationItemView>(
        r#"
        SELECT attempt_id,cve_finding_id,system_id,canonical_cve_id,
               canonical_package_name,baseline_scan_id,
               baseline_scan_derivation_id,baseline_scan_completed_at,
               baseline_generation_snapshot_id,baseline_generation,
               baseline_target_store_path,baseline_occurrence_derivation_path,
               baseline_observed_package_version,result,scan_id,scan_derivation_id,
               scan_completed_at,generation_snapshot_id,generation,
               target_store_path,occurrence_present,
               occurrence_derivation_path,observed_package_version,observed_at,detail
        FROM poam_cve_verification_items item
        JOIN systems system ON system.id=item.system_id
        WHERE attempt_id=ANY($1) AND ($2 OR system.environment_id=ANY($3))
        ORDER BY cve_finding_id"#,
    )
    .bind(&attempt_ids)
    .bind(is_admin)
    .bind(environment_ids)
    .fetch_all(&mut **tx)
    .await?;
    let verification_attempts = attempt_rows
        .into_iter()
        .map(|row| VerificationAttemptView {
            id: row.0,
            outcome: row.1,
            poam_revision: row.2,
            attempted_by: row.3,
            attempted_at: row.4,
            items: verification_items
                .iter()
                .filter(|item| item.attempt_id == row.0)
                .cloned()
                .collect(),
            cve_items: cve_verification_items
                .iter()
                .filter(|item| item.attempt_id == row.0)
                .cloned()
                .collect(),
        })
        .collect();
    let activity = sqlx::query_as::<_, ActivityView>(r#"
        SELECT activity.id,activity.actor_user_id,
               COALESCE(actor.username,actor.email) AS actor_display,
               activity.kind,activity.payload,activity.created_at
        FROM poam_activity activity
        LEFT JOIN users actor ON actor.id=activity.actor_user_id
        WHERE activity.poam_id=$1
          AND ($3::timestamptz IS NULL OR (activity.created_at,activity.id)<($3,$4))
          AND ($5 OR (
            (
              (COALESCE(activity.payload->>'finding_id',activity.payload#>>'{finding,finding_id}') IS NULL
               AND COALESCE(activity.payload->>'cve_finding_id',activity.payload#>>'{finding,cve_finding_id}') IS NULL)
              OR EXISTS (
                SELECT 1 FROM poam_findings finding
                JOIN systems system ON system.id=finding.system_id
                WHERE finding.id=COALESCE(
                  activity.payload->>'finding_id',activity.payload#>>'{finding,finding_id}'
                )::uuid AND system.environment_id=ANY($6)
              )
              OR EXISTS (
                SELECT 1 FROM poam_cve_findings finding
                JOIN systems system ON system.id=finding.system_id
                WHERE finding.id=COALESCE(
                  activity.payload->>'cve_finding_id',activity.payload#>>'{finding,cve_finding_id}'
                )::uuid AND system.environment_id=ANY($6)
              )
            )
            AND NOT EXISTS (
              SELECT 1
              FROM jsonb_array_elements(COALESCE(activity.payload->'cve_items','[]'::jsonb)) item
              WHERE item->>'cve_finding_id' IS NOT NULL
                AND NOT EXISTS (
                  SELECT 1 FROM poam_cve_findings finding
                  JOIN systems system ON system.id=finding.system_id
                  WHERE finding.id=(item->>'cve_finding_id')::uuid
                    AND system.environment_id=ANY($6)
                )
            )
            AND NOT EXISTS (
              SELECT 1
              FROM jsonb_array_elements(COALESCE(activity.payload->'items','[]'::jsonb)) item
              WHERE item->>'finding_id' IS NOT NULL
                AND NOT EXISTS (
                  SELECT 1 FROM poam_findings finding
                  JOIN systems system ON system.id=finding.system_id
                  WHERE finding.id=(item->>'finding_id')::uuid
                    AND system.environment_id=ANY($6)
                )
            )
          ))
        ORDER BY activity.created_at DESC,activity.id DESC LIMIT $2"#)
        .bind(poam_id).bind(activity_limit + 1).bind(activity_before_at).bind(activity_before_id)
        .bind(is_admin).bind(environment_ids).fetch_all(&mut **tx).await?;
    let activity_has_more = activity.len() as i64 > activity_limit;
    let activity = activity
        .into_iter()
        .take(activity_limit as usize)
        .collect::<Vec<_>>();
    let activity_next_cursor = activity_has_more
        .then(|| {
            activity.last().map(|row| HistoryCursor {
                at: row.created_at,
                id: row.id,
            })
        })
        .flatten();
    Ok(Some(PoamDetail {
        poam,
        findings,
        cve_findings,
        findings_has_more,
        findings_next_cursor,
        milestones,
        assignment_references,
        verification_attempts,
        verification_has_more,
        verification_next_cursor,
        activity,
        activity_has_more,
        activity_next_cursor,
    }))
}

/// Loads visible findings that share a POA&M's policy lineage.
///
/// The service layer validates current evidence before exposing candidates.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn compatible_findings(
    pool: &PgPool,
    poam_id: Uuid,
    q: Option<&str>,
    limit: i64,
    offset: i64,
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<Vec<CompatibleFinding>> {
    Ok(sqlx::query_as::<_, CompatibleFinding>(r#"
        WITH lineage AS (
          SELECT f.policy_lineage_id FROM poam_finding_links l JOIN poam_findings f ON f.id=l.finding_id
          WHERE l.poam_id=$1 AND l.retired_at IS NULL LIMIT 1
        )
        SELECT f.id AS finding_id, f.system_id, s.hostname, s.environment_id,
               f.policy_lineage_id, p.name AS policy_name, NULL::uuid AS assessment_id,
               NULL::text AS outcome
        FROM poam_findings f JOIN lineage ON lineage.policy_lineage_id=f.policy_lineage_id
        JOIN systems s ON s.id=f.system_id JOIN deployment_policies p ON p.id=f.policy_lineage_id
        WHERE NOT EXISTS (SELECT 1 FROM poam_finding_links active WHERE active.finding_id=f.id AND active.retired_at IS NULL)
          AND ($2 OR s.environment_id = ANY($3))
          AND ($4::text IS NULL OR s.hostname ILIKE '%'||$4||'%' OR p.name ILIKE '%'||$4||'%')
         ORDER BY s.hostname, f.id LIMIT $5 OFFSET $6"#)
        .bind(poam_id).bind(is_admin).bind(environment_ids).bind(q).bind(limit.clamp(1, 100)).bind(offset.max(0))
        .fetch_all(pool).await?)
}

/// Loads aggregate POA&M dashboard counts for an actor scope.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn dashboard(
    pool: &PgPool,
    today: NaiveDate,
    is_admin: bool,
    envs: &[Uuid],
) -> Result<DashboardSummary> {
    Ok(sqlx::query_as::<_, DashboardSummary>(
        r#"
        WITH visible AS (
          SELECT p.* FROM poams p WHERE $2 OR poam_visible_to_environments(p.id,$3)
        ) SELECT COUNT(*) AS total, COUNT(*) FILTER(WHERE status<>'completed') AS active,
          COUNT(*) FILTER(WHERE status<>'completed' AND target_date<$1) AS overdue,
          COUNT(*) FILTER(WHERE status='awaiting_verification') AS awaiting_verification,
          COUNT(*) FILTER(WHERE status='completed') AS completed FROM visible"#,
    )
    .bind(today)
    .bind(is_admin)
    .bind(envs)
    .fetch_one(pool)
    .await?)
}

/// Loads a page of visible overdue or awaiting-verification POA&Ms.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn watchlist(
    pool: &PgPool,
    today: NaiveDate,
    is_admin: bool,
    environment_ids: &[Uuid],
    limit: i64,
    offset: i64,
) -> Result<Page<PoamSummary>> {
    let mut builder = QueryBuilder::<Postgres>::new("SELECT ");
    builder
        .push(SUMMARY_COLUMNS_BEFORE_TODAY)
        .push_bind(today)
        .push(SUMMARY_COLUMNS_AFTER_TODAY)
        .push(" FROM poams p WHERE p.status <> 'completed' AND (p.target_date < ")
        .push_bind(today)
        .push(" OR p.status = 'awaiting_verification') AND (")
        .push_bind(is_admin)
        .push(" OR poam_visible_to_environments(p.id,")
        .push_bind(environment_ids)
        .push(")) ORDER BY (p.target_date < ")
        .push_bind(today)
        .push(") DESC, p.target_date NULLS LAST, p.updated_at DESC, p.id LIMIT ")
        .push_bind(limit + 1)
        .push(" OFFSET ")
        .push_bind(offset);
    let mut items = builder
        .build_query_as::<PoamSummary>()
        .fetch_all(pool)
        .await?;
    let has_more = items.len() as i64 > limit;
    if has_more {
        items.truncate(limit as usize);
    }
    Ok(Page {
        items,
        limit,
        offset,
        has_more,
        next_offset: has_more.then_some(offset + limit),
    })
}

/// Loads POA&M aggregate counts for visible requested systems.
///
/// Finding-state counts are initialized to zero and are populated from current
/// authoritative evidence by the service layer.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn system_rollups(
    pool: &PgPool,
    system_ids: &[Uuid],
    today: NaiveDate,
    is_admin: bool,
    envs: &[Uuid],
) -> Result<Vec<Rollup>> {
    Ok(sqlx::query_as::<_, Rollup>(r#"
       WITH requested AS (SELECT unnest($1::uuid[]) AS scope_id),
       visible_scope AS (SELECT r.scope_id FROM requested r JOIN systems s ON s.id=r.scope_id
         WHERE $3 OR s.environment_id=ANY($4)),
        visible AS (SELECT p.* FROM poams p WHERE $3 OR poam_visible_to_environments(p.id,$4)), pairs AS (
         SELECT DISTINCT context.system_id AS scope_id,p.id,p.status,p.target_date FROM visible p
         JOIN poam_context_systems context ON context.poam_id=p.id
         WHERE context.system_id=ANY($1)), poam_stats AS (
        SELECT scope_id,COUNT(*) total,COUNT(*) FILTER(WHERE status<>'completed') active,
          COUNT(*) FILTER(WHERE status<>'completed' AND target_date<$2) overdue,
          COUNT(*) FILTER(WHERE status='awaiting_verification') awaiting_verification,
          COUNT(*) FILTER(WHERE status='completed') completed FROM pairs GROUP BY scope_id)
       SELECT r.scope_id,COALESCE(p.total,0) total,COALESCE(p.active,0) active,
         COALESCE(p.overdue,0) overdue,COALESCE(p.awaiting_verification,0) awaiting_verification,
         COALESCE(p.completed,0) completed,0::bigint AS open_findings,
         0::bigint AS on_poam_findings,0::bigint AS no_poam_findings
       FROM visible_scope r LEFT JOIN poam_stats p USING(scope_id)
      ORDER BY r.scope_id"#)
      .bind(system_ids).bind(today).bind(is_admin).bind(envs).fetch_all(pool).await?)
}

/// Initializes rollup rows for requested bundle lineages that exist.
///
/// The service layer resolves assignment scope and populates all counts.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn bundle_rollups(
    pool: &PgPool,
    bundle_ids: &[Uuid],
    _today: NaiveDate,
    _is_admin: bool,
    _envs: &[Uuid],
) -> Result<Vec<Rollup>> {
    Ok(sqlx::query_as::<_, Rollup>(
        r#"SELECT scope_id,0::bigint AS total,0::bigint AS active,0::bigint AS overdue,
           0::bigint AS awaiting_verification,0::bigint AS completed,
           0::bigint AS open_findings,0::bigint AS on_poam_findings,
           0::bigint AS no_poam_findings
            FROM unnest($1::uuid[]) AS requested(scope_id)
            JOIN compliance_bundles bundle ON bundle.id=requested.scope_id
            ORDER BY scope_id"#,
    )
    .bind(bundle_ids)
    .fetch_all(pool)
    .await?)
}

/// Inserts matching POA&M activity and administrative audit events.
///
/// Both events use the caller's transaction and must commit or roll back
/// together with the mutation they describe.
///
/// # Errors
///
/// Returns an error when either event cannot be inserted.
pub async fn insert_activity_and_audit(
    tx: &mut Transaction<'_, Postgres>,
    poam_id: Uuid,
    actor_id: Uuid,
    actor_identifier: &str,
    kind: &str,
    payload: &serde_json::Value,
    request_origin: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO poam_activity(poam_id,actor_user_id,kind,payload,created_at) VALUES($1,$2,$3,$4,clock_timestamp())",
    )
    .bind(poam_id)
    .bind(actor_id)
    .bind(kind)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    sqlx::query("INSERT INTO admin_audit_events(actor_user_id,actor_identifier,action,target,request_origin,metadata) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(actor_id).bind(actor_identifier).bind(format!("poam_{kind}"))
        .bind(format!("poam:{poam_id}")).bind(request_origin).bind(payload).execute(&mut **tx).await?;
    Ok(())
}

/// Loads one page of waiver records.
///
/// Actor authorization is enforced by the service layer.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn list_waivers(pool: &PgPool, query: &WaiverListQuery) -> Result<Page<WaiverView>> {
    let limit = query.limit.unwrap_or(25).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);
    let mut builder = QueryBuilder::<Postgres>::new(
        r#"SELECT waiver.id,waiver.finding_id,finding.system_id,
      finding.policy_lineage_id,waiver.status,waiver.justification,waiver.policy_version_id,
      waiver.assessment_id,waiver.observation_token,waiver.observation_snapshot,waiver.accepted_by,waiver.accepted_at,
      waiver.expires_at,waiver.created_by,waiver.created_at,waiver.updated_at
      FROM finding_waivers waiver JOIN poam_findings finding ON finding.id=waiver.finding_id WHERE TRUE"#,
    );
    if let Some(status) = query.status.as_deref() {
        builder.push(" AND waiver.status=").push_bind(status);
    }
    if let Some(finding_id) = query.finding_id {
        builder
            .push(" AND waiver.finding_id=")
            .push_bind(finding_id);
    }
    builder
        .push(" ORDER BY waiver.created_at DESC,waiver.id DESC LIMIT ")
        .push_bind(limit + 1)
        .push(" OFFSET ")
        .push_bind(offset);
    let mut items = builder
        .build_query_as::<WaiverView>()
        .fetch_all(pool)
        .await?;
    let has_more = items.len() as i64 > limit;
    if has_more {
        items.truncate(limit as usize);
    }
    Ok(Page {
        items,
        limit,
        offset,
        has_more,
        next_offset: has_more.then_some(offset + limit),
    })
}

/// Loads one waiver record by ID.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute or decode the query.
pub async fn waiver(pool: &PgPool, id: Uuid) -> Result<Option<WaiverView>> {
    Ok(sqlx::query_as::<_, WaiverView>(
        r#"SELECT waiver.id,waiver.finding_id,finding.system_id,
      finding.policy_lineage_id,waiver.status,waiver.justification,waiver.policy_version_id,
      waiver.assessment_id,waiver.observation_token,waiver.observation_snapshot,waiver.accepted_by,waiver.accepted_at,
      waiver.expires_at,waiver.created_by,waiver.created_at,waiver.updated_at
      FROM finding_waivers waiver JOIN poam_findings finding ON finding.id=waiver.finding_id
      WHERE waiver.id=$1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}
