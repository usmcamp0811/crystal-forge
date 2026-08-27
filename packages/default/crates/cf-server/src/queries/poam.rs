use anyhow::Result;
use chrono::NaiveDate;
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::models::poam::{
    ActivityView, AssignmentReferenceView, CompatibleFinding, DashboardSummary, FindingView,
    HistoryCursor, MilestoneView, Page, PoamDetail, PoamListQuery, PoamSummary, Rollup,
    VerificationAttemptView, VerificationItemView, WaiverListQuery, WaiverView,
};

const SUMMARY_COLUMNS: &str = r#"
    p.id, 'POAM-' || lpad(p.human_number::text, 4, '0') AS human_id,
    p.title, p.plan, p.owner, p.target_date, p.risk, p.status, p.revision,
    COALESCE(p.status <> 'completed' AND p.target_date < $1, FALSE) AS overdue,
    (SELECT COUNT(DISTINCT l.finding_id) FROM poam_finding_links l
      WHERE l.poam_id = p.id AND ((p.status <> 'completed' AND l.retired_at IS NULL)
        OR (p.status = 'completed' AND l.retirement_reason='closed:'||p.closure_attempt_id::text))) AS finding_count,
    p.created_at, p.updated_at, p.closed_at, p.closure_attempt_id
"#;

const SUMMARY_COLUMNS_BEFORE_TODAY: &str = r#"
    p.id, 'POAM-' || lpad(p.human_number::text, 4, '0') AS human_id,
    p.title, p.plan, p.owner, p.target_date, p.risk, p.status, p.revision,
    COALESCE(p.status <> 'completed' AND p.target_date <
"#;

const SUMMARY_COLUMNS_AFTER_TODAY: &str = r#", FALSE) AS overdue,
    (SELECT COUNT(DISTINCT l.finding_id) FROM poam_finding_links l
      WHERE l.poam_id = p.id AND ((p.status <> 'completed' AND l.retired_at IS NULL)
        OR (p.status = 'completed' AND l.retirement_reason='closed:'||p.closure_attempt_id::text))) AS finding_count,
    p.created_at, p.updated_at, p.closed_at, p.closure_attempt_id
"#;

pub async fn user_environment_ids(pool: &PgPool, user_id: Uuid) -> Result<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT environment_id FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

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

pub async fn list(
    pool: &PgPool,
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
        builder.push(" AND p.status = ").push_bind(status);
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
               COALESCE(closure_item.result,'unresolved') AS resolution_state,
               closure_item.effective_set_digest,closure_item.effective_config_digest,
               COALESCE(closure_item.bundle_ids,'{}'::uuid[]) AS bundle_ids,
               COALESCE(closure_item.bundle_version_ids,'{}'::uuid[]) AS bundle_version_ids,
               COALESCE(closure_item.requirement_version_ids,'{}'::uuid[]) AS requirement_version_ids
        FROM poam_finding_links l JOIN poam_findings f ON f.id=l.finding_id
        JOIN systems s ON s.id=f.system_id JOIN deployment_policies policy ON policy.id=f.policy_lineage_id
        LEFT JOIN poam_verification_attempts closure_attempt
          ON l.retirement_reason='closed:'||closure_attempt.id::text AND closure_attempt.poam_id=l.poam_id
        LEFT JOIN poam_verification_items closure_item
          ON closure_item.attempt_id=closure_attempt.id AND closure_item.finding_id=l.finding_id
        WHERE l.poam_id=$1 AND ($2 OR s.environment_id=ANY($3))
          AND ($5::timestamptz IS NULL OR (l.linked_at,l.id)<($5,$6))
        ORDER BY l.linked_at DESC,l.id DESC LIMIT $4"#)
        .bind(poam_id).bind(is_admin).bind(environment_ids).bind(finding_limit + 1)
        .bind(finding_before_at).bind(finding_before_id).fetch_all(&mut **tx).await?;
    let findings_has_more = findings.len() as i64 > finding_limit;
    let findings = findings
        .into_iter()
        .take(finding_limit as usize)
        .collect::<Vec<_>>();
    let findings_next_cursor = findings_has_more
        .then(|| {
            findings.last().map(|finding| HistoryCursor {
                at: finding.linked_at,
                id: finding.link_id,
            })
        })
        .flatten();
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
          WHERE reference.poam_id=$1 ORDER BY reference.added_at,reference.assignment_version_id"#)
        .bind(poam_id).fetch_all(&mut **tx).await?;
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
    let verification_items=sqlx::query_as::<_,VerificationItemView>(r#"SELECT attempt_id,finding_id,system_id,policy_lineage_id,result,
        policy_version_id,assessment_id,derivation_id,target_store_path,effective_set_digest,effective_config_digest,
        effective_config,observed_outcome,observation_token,observation_snapshot,assessment_updated_at,bundle_ids,bundle_version_ids,
        requirement_version_ids,waiver_id,observed_at,detail
        FROM poam_verification_items WHERE attempt_id=ANY($1) ORDER BY finding_id"#)
        .bind(&attempt_ids).fetch_all(&mut **tx).await?;
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
        })
        .collect();
    let activity = sqlx::query_as::<_, ActivityView>(
        "SELECT id, actor_user_id, kind, payload, created_at FROM poam_activity WHERE poam_id=$1 AND ($3::timestamptz IS NULL OR (created_at,id)<($3,$4)) ORDER BY created_at DESC, id DESC LIMIT $2")
        .bind(poam_id).bind(activity_limit + 1).bind(activity_before_at).bind(activity_before_id).fetch_all(&mut **tx).await?;
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
