use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::compliance::canonical::semantic_digest;
use crate::compliance::resolver::{
    ResolutionOutcome, resolve_system_effective_policies_in_tx,
    resolve_systems_effective_policies_in_tx,
};
use crate::models::poam::*;
use crate::queries::poam::{self, insert_activity_and_audit};

pub trait PoamClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
    fn today(&self) -> NaiveDate {
        self.now().date_naive()
    }
}

pub struct SystemClock;
const MAX_POAM_RELATIONSHIPS: i64 = 100;
const MAX_SHORT_TEXT_BYTES: usize = 256;
const MAX_SEARCH_BYTES: usize = 256;
const MAX_NOTE_BYTES: usize = 4_096;
const MAX_PLAN_BYTES: usize = 16_384;
const MAX_CANDIDATES_SCANNED: i64 = 1_000;
const MAX_RESOLVER_FINDINGS: usize = 1_000;
impl PoamClock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone)]
pub struct PoamActor {
    pub user_id: Uuid,
    pub identifier: String,
    pub is_admin: bool,
    pub can_mutate: bool,
    pub environment_ids: Vec<Uuid>,
    pub request_origin: Option<String>,
}

#[derive(Debug)]
pub enum PoamError {
    NotFound,
    Forbidden,
    Validation(&'static str, String),
    Conflict(&'static str, String),
    Precondition(&'static str, String, Option<Value>),
    Database(anyhow::Error),
}

impl From<sqlx::Error> for PoamError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value.into())
    }
}
impl From<anyhow::Error> for PoamError {
    fn from(value: anyhow::Error) -> Self {
        Self::Database(value)
    }
}

fn db_conflict(error: &sqlx::Error) -> Option<PoamError> {
    let constraint = error.as_database_error()?.constraint()?;
    match constraint {
        "poam_finding_links_one_active_remediation" => Some(PoamError::Conflict(
            "finding_already_managed",
            "The finding already has an active remediation".into(),
        )),
        "finding_waivers_one_accepted" => Some(PoamError::Conflict(
            "accepted_waiver_exists",
            "The finding already has an accepted waiver".into(),
        )),
        _ => None,
    }
}

async fn begin_serializable(pool: &PgPool) -> Result<Transaction<'_, Postgres>, PoamError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

async fn poam_finding_keys(pool: &PgPool, id: Uuid) -> Result<Vec<(Uuid, Uuid, Uuid)>, PoamError> {
    Ok(sqlx::query_as(
        r#"SELECT f.id,f.system_id,f.policy_lineage_id
           FROM poam_finding_links link
           JOIN poam_findings f ON f.id=link.finding_id
           WHERE link.poam_id=$1 AND link.retired_at IS NULL
           ORDER BY f.system_id,f.policy_lineage_id"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?)
}

async fn lock_finding_keys_tx(
    tx: &mut Transaction<'_, Postgres>,
    findings: &[(Uuid, Uuid, Uuid)],
) -> Result<(), PoamError> {
    if findings.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"SELECT lock_poam_finding_key(key.system_id,key.policy_lineage_id)
           FROM (
             SELECT input.system_id,input.policy_lineage_id
             FROM UNNEST($1::uuid[],$2::uuid[]) input(system_id,policy_lineage_id)
             ORDER BY input.system_id,input.policy_lineage_id
           ) key"#,
    )
    .bind(findings.iter().map(|row| row.1).collect::<Vec<_>>())
    .bind(findings.iter().map(|row| row.2).collect::<Vec<_>>())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn actor_can_access_systems_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    system_ids: &[Uuid],
) -> Result<bool, PoamError> {
    if actor.is_admin {
        return Ok(true);
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM systems WHERE id=ANY($1) AND environment_id=ANY($2)",
    )
    .bind(system_ids)
    .bind(&actor.environment_ids)
    .fetch_one(&mut **tx)
    .await?;
    Ok(count == system_ids.iter().copied().collect::<BTreeSet<_>>().len() as i64)
}

async fn require_visible(pool: &PgPool, actor: &PoamActor, poam_id: Uuid) -> Result<(), PoamError> {
    if poam::poam_visible(pool, poam_id, actor.is_admin, &actor.environment_ids).await? {
        Ok(())
    } else {
        Err(PoamError::NotFound)
    }
}

fn require_mutator(actor: &PoamActor) -> Result<(), PoamError> {
    if actor.can_mutate {
        Ok(())
    } else {
        Err(PoamError::Forbidden)
    }
}

fn page_bounds(limit: Option<i64>, offset: Option<i64>) -> Result<(i64, i64), PoamError> {
    let limit = limit.unwrap_or(25);
    let offset = offset.unwrap_or(0);
    if !(1..=100).contains(&limit) {
        return Err(PoamError::Validation(
            "invalid_limit",
            "limit must be between 1 and 100".into(),
        ));
    }
    if !(0..=10_000).contains(&offset) {
        return Err(PoamError::Validation(
            "invalid_offset",
            "offset must be between 0 and 10000".into(),
        ));
    }
    Ok((limit, offset))
}

fn validate_text_length(
    value: &str,
    max: usize,
    code: &'static str,
    field: &str,
) -> Result<(), PoamError> {
    if value.len() > max {
        return Err(PoamError::Validation(
            code,
            format!("{field} must not exceed {max} bytes"),
        ));
    }
    Ok(())
}

fn normalized_search(value: &mut Option<String>) {
    *value = value
        .take()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
}

async fn require_poam_contexts_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    id: Uuid,
) -> Result<(), PoamError> {
    if actor.is_admin {
        return Ok(());
    }
    let inaccessible: bool = sqlx::query_scalar("SELECT NOT poam_visible_to_environments($1,$2)")
        .bind(id)
        .bind(&actor.environment_ids)
        .fetch_one(&mut **tx)
        .await?;
    if inaccessible {
        Err(PoamError::NotFound)
    } else {
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct AssessmentContext {
    assessment_id: Uuid,
    finding_id: Uuid,
    system_id: Uuid,
    policy_lineage_id: Uuid,
    policy_version_id: Uuid,
    overall_outcome: String,
    target_store_path: String,
    effective_set_digest: String,
    effective_config_digest: String,
    effective_config: Value,
}

async fn assessment_context_tx(
    tx: &mut Transaction<'_, Postgres>,
    assessment_id: Uuid,
) -> Result<Option<AssessmentContext>, PoamError> {
    Ok(sqlx::query_as::<_, AssessmentContext>(
        r#"
        SELECT a.id AS assessment_id, f.id AS finding_id, a.system_id,
               a.policy_lineage_id, a.policy_version_id, a.overall_outcome,
               a.target_store_path, a.effective_set_digest,
               a.effective_config_digest, a.effective_config
        FROM composite_policy_assessments a
        JOIN poam_findings f ON f.system_id=a.system_id AND f.policy_lineage_id=a.policy_lineage_id
        WHERE a.id=$1 FOR SHARE OF a, f"#,
    )
    .bind(assessment_id)
    .fetch_optional(&mut **tx)
    .await?)
}

async fn assessment_finding_key_tx(
    tx: &mut Transaction<'_, Postgres>,
    assessment_id: Uuid,
) -> Result<Option<(Uuid, Uuid)>, PoamError> {
    Ok(sqlx::query_as(
        "SELECT system_id,policy_lineage_id FROM composite_policy_assessments WHERE id=$1",
    )
    .bind(assessment_id)
    .fetch_optional(&mut **tx)
    .await?)
}

async fn validate_current_assessment_tx(
    tx: &mut Transaction<'_, Postgres>,
    context: &AssessmentContext,
) -> Result<(), PoamError> {
    let current_store: Option<String> = sqlx::query_scalar(
        r#"
        SELECT ss.store_path FROM systems s JOIN system_states ss ON ss.hostname=s.hostname
        WHERE s.id=$1 AND ss.store_path IS NOT NULL AND btrim(ss.store_path)<>''
        ORDER BY ss.timestamp DESC,ss.id DESC LIMIT 1"#,
    )
    .bind(context.system_id)
    .fetch_optional(&mut **tx)
    .await?;
    if current_store.as_deref() != Some(context.target_store_path.as_str()) {
        return Err(PoamError::Precondition(
            "stale_finding",
            "Assessment is not for the currently deployed system target".into(),
            None,
        ));
    }
    let resolved = resolve_system_effective_policies_in_tx(tx, context.system_id).await?;
    let ResolutionOutcome::Resolved(resolved) = resolved else {
        return Err(PoamError::Precondition(
            "policy_conflict",
            "Current effective policy set has conflicts".into(),
            None,
        ));
    };
    let Some(policy) = resolved
        .policies
        .iter()
        .find(|policy| policy.policy_lineage_id == context.policy_lineage_id)
    else {
        return Err(PoamError::Precondition(
            "stale_finding",
            "Finding policy is no longer effective for the system".into(),
            None,
        ));
    };
    if policy.policy_version_id != context.policy_version_id
        || resolved.effective_set_digest != context.effective_set_digest
        || semantic_digest(&policy.effective_config) != context.effective_config_digest
        || policy.effective_config != context.effective_config
    {
        return Err(PoamError::Precondition(
            "stale_finding",
            "Finding does not match the current effective policy context".into(),
            None,
        ));
    }
    let current_assessment_id: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM composite_policy_assessments
           WHERE system_id=$1 AND policy_lineage_id=$2 AND policy_version_id=$3
             AND target_store_path=$4 AND effective_set_digest=$5
             AND effective_config_digest=$6 AND effective_config=$7
           ORDER BY updated_at DESC,id DESC LIMIT 1"#,
    )
    .bind(context.system_id)
    .bind(context.policy_lineage_id)
    .bind(context.policy_version_id)
    .bind(&context.target_store_path)
    .bind(&context.effective_set_digest)
    .bind(&context.effective_config_digest)
    .bind(&context.effective_config)
    .fetch_optional(&mut **tx)
    .await?;
    if current_assessment_id != Some(context.assessment_id) {
        return Err(PoamError::Precondition(
            "stale_finding",
            "Assessment was superseded by a newer authoritative observation".into(),
            None,
        ));
    }
    Ok(())
}

async fn lock_assessment_finding_key_tx(
    tx: &mut Transaction<'_, Postgres>,
    key: (Uuid, Uuid),
) -> Result<(), PoamError> {
    sqlx::query("SELECT lock_poam_finding_key($1,$2)")
        .bind(key.0)
        .bind(key.1)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn observation_snapshot_tx(
    tx: &mut Transaction<'_, Postgres>,
    assessment_id: Uuid,
) -> Result<Option<Value>, PoamError> {
    let snapshot: Option<Value> = sqlx::query_scalar(
        r#"SELECT jsonb_build_object(
             'assessment',to_jsonb(assessment),
             'rules',COALESCE((SELECT jsonb_agg(to_jsonb(result) ORDER BY result.ordinal,result.rule_id)
               FROM composite_policy_rule_results result
               WHERE result.assessment_id=assessment.id),'[]'::jsonb))
           FROM composite_policy_assessments assessment WHERE assessment.id=$1"#,
    )
    .bind(assessment_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(snapshot)
}

async fn validate_assignment_refs_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    ids: &[Uuid],
) -> Result<(), PoamError> {
    if ids.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, Option<Uuid>)>(
        r#"
        SELECT av.id, a.system_id, COALESCE(a.environment_id, s.environment_id)
        FROM compliance_bundle_assignment_versions av
        JOIN compliance_bundle_assignments a ON a.id=av.assignment_id
        LEFT JOIN systems s ON s.id=a.system_id WHERE av.id=ANY($1)"#,
    )
    .bind(ids)
    .fetch_all(&mut **tx)
    .await?;
    if rows.len() != ids.iter().copied().collect::<BTreeSet<_>>().len()
        || (!actor.is_admin
            && rows.iter().any(|(_, _, environment)| {
                environment.is_none_or(|id| !actor.environment_ids.contains(&id))
            }))
    {
        return Err(PoamError::NotFound);
    }
    Ok(())
}

async fn validate_assignment_compatibility_tx(
    tx: &mut Transaction<'_, Postgres>,
    assignment_version_ids: &[Uuid],
    finding_contexts: &[(Uuid, Uuid)],
) -> Result<(), PoamError> {
    if assignment_version_ids.is_empty() {
        return Ok(());
    }
    let system_ids = finding_contexts.iter().map(|row| row.0).collect::<Vec<_>>();
    let lineage_ids = finding_contexts.iter().map(|row| row.1).collect::<Vec<_>>();
    let compatible_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(DISTINCT version.id)
           FROM compliance_bundle_assignment_versions version
           JOIN compliance_bundle_assignments assignment ON assignment.id=version.assignment_id
           WHERE version.id=ANY($1)
              AND EXISTS (
                SELECT 1
                FROM UNNEST($2::uuid[],$3::uuid[]) context(system_id,policy_lineage_id)
                JOIN systems system ON system.id=context.system_id
                WHERE (assignment.system_id=context.system_id
                    OR assignment.environment_id=system.environment_id)
                  AND (
                    EXISTS (
                      SELECT 1 FROM compliance_assignment_additions addition
                      JOIN deployment_policy_versions policy_version ON policy_version.id=addition.policy_version_id
                      WHERE addition.assignment_version_id=version.id
                        AND policy_version.policy_id=context.policy_lineage_id
                    )
                    OR EXISTS (
                      SELECT 1 FROM compliance_bundle_version_policies membership
                      JOIN deployment_policy_versions policy_version ON policy_version.id=membership.policy_version_id
                      WHERE membership.bundle_version_id=version.bundle_version_id
                        AND membership.selected
                        AND policy_version.policy_id=context.policy_lineage_id
                        AND NOT EXISTS (
                          SELECT 1 FROM compliance_assignment_exclusions exclusion
                          WHERE exclusion.assignment_version_id=version.id
                            AND exclusion.policy_version_id=membership.policy_version_id
                        )
                    )
                  )
                )"#,
    )
    .bind(assignment_version_ids)
    .bind(system_ids)
    .bind(lineage_ids)
    .fetch_one(&mut **tx)
    .await?;
    if compatible_count
        != assignment_version_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len() as i64
    {
        return Err(PoamError::Validation(
            "incompatible_assignment_reference",
            "Assignment references must overlap a linked finding scope and policy lineage".into(),
        ));
    }
    Ok(())
}

pub async fn create(
    pool: &PgPool,
    actor: &PoamActor,
    request: CreatePoamRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    let title = request.title.trim();
    if title.is_empty() {
        return Err(PoamError::Validation(
            "invalid_title",
            "Title is required".into(),
        ));
    }
    validate_text_length(title, MAX_SHORT_TEXT_BYTES, "text_too_long", "title")?;
    validate_text_length(&request.plan, MAX_PLAN_BYTES, "text_too_long", "plan")?;
    validate_text_length(
        &request.owner,
        MAX_SHORT_TEXT_BYTES,
        "text_too_long",
        "owner",
    )?;
    if request.assignment_version_ids.len() > 100 {
        return Err(PoamError::Validation(
            "too_many_assignment_references",
            "At most 100 assignment references are allowed".into(),
        ));
    }
    let mut assignment_version_ids = request.assignment_version_ids.clone();
    assignment_version_ids.sort_unstable();
    assignment_version_ids.dedup();
    let mut tx = pool.begin().await?;
    let key = assessment_finding_key_tx(&mut tx, request.assessment_id)
        .await?
        .ok_or(PoamError::NotFound)?;
    lock_assessment_finding_key_tx(&mut tx, key).await?;
    let context = assessment_context_tx(&mut tx, request.assessment_id)
        .await?
        .ok_or(PoamError::NotFound)?;
    if !actor_can_access_systems_tx(&mut tx, actor, &[context.system_id]).await? {
        return Err(PoamError::NotFound);
    }
    validate_current_assessment_tx(&mut tx, &context).await?;
    if context.overall_outcome != "fail" {
        return Err(PoamError::Precondition(
            "finding_not_failed",
            "A POA&M can only be created from a current Fail finding".into(),
            None,
        ));
    }
    validate_assignment_refs_tx(&mut tx, actor, &assignment_version_ids).await?;
    validate_assignment_compatibility_tx(
        &mut tx,
        &assignment_version_ids,
        &[(context.system_id, context.policy_lineage_id)],
    )
    .await?;
    let poam_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO poams(title,plan,owner,target_date,risk,created_by)
        VALUES($1,$2,$3,$4,$5,$6) RETURNING id"#,
    )
    .bind(title)
    .bind(request.plan.trim())
    .bind(request.owner.trim())
    .bind(request.target_date.or_else(|| {
        request
            .default_milestones
            .then(|| clock.today() + Duration::days(56))
    }))
    .bind(request.risk.as_str())
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await?;
    if let Err(error) =
        sqlx::query("INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)")
            .bind(poam_id)
            .bind(context.finding_id)
            .bind(actor.user_id)
            .execute(&mut *tx)
            .await
    {
        return Err(db_conflict(&error).unwrap_or_else(|| error.into()));
    }
    for assignment_version_id in &assignment_version_ids {
        sqlx::query("INSERT INTO poam_assignment_references(poam_id,assignment_id,assignment_version_id,added_by) SELECT $1,assignment_id,id,$3 FROM compliance_bundle_assignment_versions WHERE id=$2")
            .bind(poam_id).bind(assignment_version_id).bind(actor.user_id).execute(&mut *tx).await?;
    }
    if request.default_milestones {
        let offsets = [14_i64, 28, 35, 49, 56];
        let titles = [
            "Update NixOS module",
            "Deploy to staging",
            "Validate new configuration",
            "Deploy to production",
            "Verify compliance evaluation passes",
        ];
        for (ordinal, (offset, milestone_title)) in offsets.into_iter().zip(titles).enumerate() {
            sqlx::query("INSERT INTO poam_milestones(poam_id,ordinal,title,target_date,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$5)")
                .bind(poam_id).bind(ordinal as i32).bind(milestone_title)
                .bind(clock.today() + Duration::days(offset)).bind(actor.user_id).execute(&mut *tx).await?;
        }
    }
    let mut payload:Value=sqlx::query_scalar(r#"SELECT jsonb_build_object(
      'poam',jsonb_build_object('id',id,'human_number',human_number,'title',title,'plan',plan,
        'owner',owner,'target_date',target_date,'risk',risk,'status',status,'revision',revision,
        'created_by',created_by,'created_at',created_at),
      'finding',jsonb_build_object('finding_id',$2::uuid,'assessment_id',$3::uuid),
      'assignments',COALESCE((SELECT jsonb_agg(jsonb_build_object('assignment_id',assignment_id,
        'assignment_version_id',assignment_version_id,'added_by',added_by,'added_at',added_at)
        ORDER BY assignment_version_id) FROM poam_assignment_references WHERE poam_id=$1),'[]'::jsonb),
      'milestones',COALESCE((SELECT jsonb_agg(to_jsonb(milestone) ORDER BY ordinal)
        FROM poam_milestones milestone WHERE poam_id=$1),'[]'::jsonb)) FROM poams WHERE id=$1"#)
      .bind(poam_id).bind(context.finding_id).bind(request.assessment_id).fetch_one(&mut *tx).await?;
    payload["poam_id"] = json!(poam_id);
    payload["revision"] = json!(1);
    insert_activity_and_audit(
        &mut tx,
        poam_id,
        actor.user_id,
        &actor.identifier,
        "created",
        &payload,
        actor.request_origin.as_deref(),
    )
    .await?;
    for assignment_version_id in &assignment_version_ids {
        insert_activity_and_audit(&mut tx,poam_id,actor.user_id,&actor.identifier,"assignment_linked",
          &json!({"poam_id":poam_id,"revision":1,"assignment_version_id":assignment_version_id,"initial":true}),
          actor.request_origin.as_deref()).await?;
    }
    if request.default_milestones {
        let milestones=sqlx::query_as::<_,(Uuid,i32,String,NaiveDate)>("SELECT id,ordinal,title,target_date FROM poam_milestones WHERE poam_id=$1 ORDER BY ordinal")
          .bind(poam_id).fetch_all(&mut *tx).await?;
        for (milestone_id, ordinal, title, target_date) in milestones {
            insert_activity_and_audit(&mut tx,poam_id,actor.user_id,&actor.identifier,"milestone_added",
              &json!({"poam_id":poam_id,"revision":1,"milestone_id":milestone_id,"ordinal":ordinal,
                "title":title,"target_date":target_date,"initial":true}),actor.request_origin.as_deref()).await?;
        }
    }
    tx.commit().await?;
    detail(pool, actor, poam_id, clock).await
}

pub async fn list(
    pool: &PgPool,
    actor: &PoamActor,
    query: &PoamListQuery,
    clock: &dyn PoamClock,
) -> Result<Page<PoamSummary>, PoamError> {
    let mut query = query.clone();
    normalized_search(&mut query.owner);
    normalized_search(&mut query.requirement);
    normalized_search(&mut query.q);
    for value in [&query.owner, &query.requirement, &query.q]
        .into_iter()
        .flatten()
    {
        validate_text_length(value, MAX_SEARCH_BYTES, "search_too_long", "search")?;
    }
    let (limit, offset) = page_bounds(query.limit, query.offset)?;
    if query.status.as_deref().is_some_and(|status| {
        !matches!(
            status,
            "open" | "in_progress" | "blocked" | "awaiting_verification" | "completed"
        )
    }) {
        return Err(PoamError::Validation(
            "invalid_status",
            "Unknown POA&M status".into(),
        ));
    }
    if query
        .risk
        .as_deref()
        .is_some_and(|risk| !matches!(risk, "high" | "medium" | "low"))
    {
        return Err(PoamError::Validation(
            "invalid_risk",
            "Unknown POA&M risk".into(),
        ));
    }
    if query.policy_lineage_id.is_none() && query.bundle_id.is_none() && query.requirement.is_none()
    {
        return Ok(poam::list(
            pool,
            &query,
            clock.today(),
            actor.is_admin,
            &actor.environment_ids,
        )
        .await?);
    }

    let needed = offset.saturating_add(limit).saturating_add(1) as usize;
    let requirement_ids = if let Some(requirement) = query.requirement.as_deref() {
        let ids=sqlx::query_scalar::<_,Uuid>("SELECT id FROM compliance_requirement_versions WHERE external_id ILIKE $1 OR title ILIKE $1 LIMIT $2")
          .bind(format!("%{}%",requirement.trim())).bind(MAX_RESOLVER_FINDINGS as i64 + 1).fetch_all(pool).await?;
        if ids.len() > MAX_RESOLVER_FINDINGS {
            return Err(PoamError::Validation(
                "candidate_scan_limit",
                "The query is too broad; add a narrower filter".into(),
            ));
        }
        ids
    } else {
        Vec::new()
    };
    let mut candidate_query = query.clone();
    candidate_query.bundle_id = None;
    candidate_query.requirement = None;
    candidate_query.limit = Some(100);
    candidate_query.offset = Some(0);
    let mut matches = Vec::new();
    let mut scanned = 0i64;
    loop {
        let candidates = poam::list(
            pool,
            &candidate_query,
            clock.today(),
            actor.is_admin,
            &actor.environment_ids,
        )
        .await?;
        if candidates.items.is_empty() {
            break;
        }
        scanned += candidates.items.len() as i64;
        let matching_ids =
            canonical_context_match_ids(pool, &query, &candidates.items, &requirement_ids, clock)
                .await?;
        matches.extend(
            candidates
                .items
                .into_iter()
                .filter(|summary| matching_ids.contains(&summary.id)),
        );
        if matches.len() >= needed || !candidates.has_more {
            break;
        }
        if scanned >= MAX_CANDIDATES_SCANNED {
            return Err(PoamError::Validation(
                "candidate_scan_limit",
                "The query is too broad; add a narrower filter".into(),
            ));
        }
        candidate_query.offset = candidates.next_offset;
    }

    let has_more = matches.len() > offset.saturating_add(limit) as usize;
    let items = matches
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();
    Ok(Page {
        items,
        limit,
        offset,
        has_more,
        next_offset: has_more.then_some(offset + limit),
    })
}

async fn canonical_context_match_ids(
    pool: &PgPool,
    query: &PoamListQuery,
    candidates: &[PoamSummary],
    requirement_ids: &[Uuid],
    clock: &dyn PoamClock,
) -> Result<BTreeSet<Uuid>, PoamError> {
    let poam_ids = candidates.iter().map(|poam| poam.id).collect::<Vec<_>>();
    let active = sqlx::query_as::<_, (Uuid, Uuid, Uuid, Uuid)>(
        r#"SELECT link.poam_id,finding.id,finding.system_id,finding.policy_lineage_id
      FROM poam_current_finding_links link JOIN poams poam ON poam.id=link.poam_id
      JOIN poam_findings finding ON finding.id=link.finding_id
      WHERE link.poam_id=ANY($1) AND poam.status<>'completed'"#,
    )
    .bind(&poam_ids)
    .fetch_all(pool)
    .await?;
    let tuples = active
        .iter()
        .map(|row| (row.1, row.2, row.3))
        .collect::<Vec<_>>();
    let active_items = if tuples.is_empty() {
        Vec::new()
    } else {
        let mut tx = pool.begin().await?;
        let items = current_verification_items_tx(&mut tx, &tuples, clock.now()).await?;
        tx.commit().await?;
        items
    };
    let completed = sqlx::query_as::<_, (Uuid, Uuid, Vec<Uuid>, Vec<Uuid>)>(
        r#"SELECT poam.id,item.policy_lineage_id,
      item.bundle_ids,item.requirement_version_ids FROM poams poam JOIN poam_verification_items item
      ON item.attempt_id=poam.closure_attempt_id WHERE poam.id=ANY($1)"#,
    )
    .bind(&poam_ids)
    .fetch_all(pool)
    .await?;
    let assignment_bundle_poams = if let Some(bundle_id) = query.bundle_id {
        sqlx::query_scalar::<_,Uuid>(r#"SELECT DISTINCT reference.poam_id FROM poam_assignment_references reference
          JOIN compliance_bundle_assignment_versions version ON version.id=reference.assignment_version_id
          JOIN compliance_bundle_versions bundle_version ON bundle_version.id=version.bundle_version_id
          WHERE reference.poam_id=ANY($1) AND bundle_version.bundle_id=$2"#).bind(&poam_ids).bind(bundle_id).fetch_all(pool).await?
    } else {
        Vec::new()
    };
    Ok(candidates
        .iter()
        .filter_map(|summary| {
            let active_matches = active
                .iter()
                .filter(|row| row.0 == summary.id)
                .filter_map(|row| active_items.iter().find(|item| item.finding_id == row.1));
            let completed_matches = completed.iter().filter(|row| row.0 == summary.id);
            let policy_ok = query.policy_lineage_id.is_none_or(|lineage| {
                active
                    .iter()
                    .any(|row| row.0 == summary.id && row.3 == lineage)
                    || completed_matches.clone().any(|row| row.1 == lineage)
            });
            let bundle_ok = query.bundle_id.is_none_or(|bundle| {
                assignment_bundle_poams.contains(&summary.id)
                    || active_matches
                        .clone()
                        .any(|item| item.bundle_ids.contains(&bundle))
                    || completed_matches.clone().any(|row| row.2.contains(&bundle))
            });
            let requirement_ok = query.requirement.is_none()
                || active_matches.clone().any(|item| {
                    item.requirement_version_ids
                        .iter()
                        .any(|id| requirement_ids.contains(id))
                })
                || completed_matches
                    .clone()
                    .any(|row| row.3.iter().any(|id| requirement_ids.contains(id)));
            (policy_ok && bundle_ok && requirement_ok).then_some(summary.id)
        })
        .collect())
}

pub async fn detail(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    detail_with_history(pool, actor, id, &PoamDetailQuery::default(), clock).await
}

pub async fn detail_with_history(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    query: &PoamDetailQuery,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_visible(pool, actor, id).await?;
    let (finding_limit, _) = page_bounds(Some(query.finding_limit.unwrap_or(100)), None)?;
    let (activity_limit, _) = page_bounds(Some(query.activity_limit.unwrap_or(100)), None)?;
    let (verification_limit, _) = page_bounds(Some(query.verification_limit.unwrap_or(10)), None)?;
    for (at, id) in [
        (query.finding_before_at, query.finding_before_id),
        (query.activity_before_at, query.activity_before_id),
        (query.verification_before_at, query.verification_before_id),
    ] {
        if at.is_some() != id.is_some() {
            return Err(PoamError::Validation(
                "invalid_history_cursor",
                "History cursor timestamps and IDs must be supplied together".into(),
            ));
        }
    }
    if verification_limit > 10 {
        return Err(PoamError::Validation(
            "invalid_verification_limit",
            "verification_limit must be between 1 and 10".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    let mut detail = poam::detail(
        &mut tx,
        id,
        clock.today(),
        actor.is_admin,
        &actor.environment_ids,
        finding_limit,
        query.finding_before_at,
        query.finding_before_id,
        activity_limit,
        query.activity_before_at,
        query.activity_before_id,
        verification_limit,
        query.verification_before_at,
        query.verification_before_id,
    )
    .await?
    .ok_or(PoamError::NotFound)?;
    if detail.poam.status != "completed" {
        let tuples = detail
            .findings
            .iter()
            .filter(|finding| finding.link_active)
            .map(|finding| (finding.id, finding.system_id, finding.policy_lineage_id))
            .collect::<Vec<_>>();
        let items = current_verification_items_tx(&mut tx, &tuples, clock.now()).await?;
        for finding in &mut detail.findings {
            if !finding.link_active {
                continue;
            }
            if let Some(item) = items.iter().find(|item| item.finding_id == finding.id) {
                finding.current_assessment_id = item.assessment_id;
                finding.current_outcome = item.observed_outcome.clone();
                finding.current_policy_version_id = item.policy_version_id;
                finding.current_target_store_path = item.target_store_path.clone();
                finding.assessment_updated_at = item.assessment_updated_at;
                finding.effective_set_digest = item.effective_set_digest.clone();
                finding.effective_config_digest = item.effective_config_digest.clone();
                finding.bundle_ids = item.bundle_ids.clone();
                finding.bundle_version_ids = item.bundle_version_ids.clone();
                finding.requirement_version_ids = item.requirement_version_ids.clone();
                finding.resolution_state = item.result.clone();
            }
        }
    }
    tx.commit().await?;
    Ok(detail)
}

async fn lock_mutable_poam(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    id: Uuid,
    revision: i64,
) -> Result<String, PoamError> {
    let row = sqlx::query_as::<_, (i64, String)>(
        "SELECT revision,status FROM poams WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PoamError::NotFound)?;
    require_poam_contexts_tx(tx, actor, id).await?;
    if row.0 != revision {
        return Err(PoamError::Conflict(
            "stale_revision",
            "POA&M revision is stale".into(),
        ));
    }
    if row.1 == "completed" {
        return Err(PoamError::Conflict(
            "poam_completed",
            "Completed POA&M must be reopened before mutation".into(),
        ));
    }
    Ok(row.1)
}

async fn bump_and_audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    id: Uuid,
    kind: &str,
    mut payload: Value,
) -> Result<i64, PoamError> {
    let revision: i64 = sqlx::query_scalar(
        "UPDATE poams SET revision=revision+1,updated_at=NOW() WHERE id=$1 RETURNING revision",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;
    payload["poam_id"] = json!(id);
    payload["revision"] = json!(revision);
    insert_activity_and_audit(
        tx,
        id,
        actor.user_id,
        &actor.identifier,
        kind,
        &payload,
        actor.request_origin.as_deref(),
    )
    .await?;
    Ok(revision)
}

pub async fn update(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    request: UpdatePoamRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    if request
        .title
        .as_deref()
        .is_some_and(|v| v.trim().is_empty())
    {
        return Err(PoamError::Validation(
            "invalid_title",
            "Title cannot be empty".into(),
        ));
    }
    if let Some(title) = request.title.as_deref() {
        validate_text_length(title.trim(), MAX_SHORT_TEXT_BYTES, "text_too_long", "title")?;
    }
    if let Some(plan) = request.plan.as_deref() {
        validate_text_length(plan.trim(), MAX_PLAN_BYTES, "text_too_long", "plan")?;
    }
    if let Some(owner) = request.owner.as_deref() {
        validate_text_length(owner.trim(), MAX_SHORT_TEXT_BYTES, "text_too_long", "owner")?;
    }
    let mut tx = pool.begin().await?;
    lock_mutable_poam(&mut tx, actor, id, request.revision).await?;
    let old:Value=sqlx::query_scalar("SELECT jsonb_build_object('title',title,'plan',plan,'owner',owner,'target_date',target_date,'risk',risk) FROM poams WHERE id=$1")
      .bind(id).fetch_one(&mut *tx).await?;
    sqlx::query(r#"UPDATE poams SET title=COALESCE($2,title),plan=COALESCE($3,plan),owner=COALESCE($4,owner),
        target_date=CASE WHEN $5 THEN $6 ELSE target_date END,risk=COALESCE($7,risk) WHERE id=$1"#)
        .bind(id).bind(request.title.as_deref().map(str::trim)).bind(request.plan.as_deref().map(str::trim))
        .bind(request.owner.as_deref().map(str::trim)).bind(request.target_date.is_some())
        .bind(request.target_date.flatten()).bind(request.risk.map(PoamRisk::as_str)).execute(&mut *tx).await?;
    let new:Value=sqlx::query_scalar("SELECT jsonb_build_object('title',title,'plan',plan,'owner',owner,'target_date',target_date,'risk',risk) FROM poams WHERE id=$1")
      .bind(id).fetch_one(&mut *tx).await?;
    if old == new {
        tx.commit().await?;
        return detail(pool, actor, id, clock).await;
    }
    bump_and_audit(&mut tx, actor, id, "updated", json!({"old":old,"new":new})).await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

fn transition_allowed(from: &str, to: PoamStatus) -> bool {
    let to = to.as_str();
    if from == to {
        return false;
    }
    matches!(
        (from, to),
        ("open", "in_progress" | "blocked")
            | ("in_progress", "open" | "blocked" | "awaiting_verification")
            | ("blocked", "open" | "in_progress" | "awaiting_verification")
            | ("awaiting_verification", "in_progress" | "blocked")
    )
}

pub async fn transition(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    request: TransitionPoamRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    if request.status == PoamStatus::Completed {
        return Err(PoamError::Validation(
            "close_required",
            "Completed is only entered through close".into(),
        ));
    }
    if let Some(note) = request.note.as_deref() {
        validate_text_length(note, MAX_NOTE_BYTES, "text_too_long", "note")?;
    }
    let mut tx = pool.begin().await?;
    let from = lock_mutable_poam(&mut tx, actor, id, request.revision).await?;
    if !transition_allowed(&from, request.status) {
        return Err(PoamError::Conflict(
            "invalid_transition",
            format!(
                "Cannot transition from {from} to {}",
                request.status.as_str()
            ),
        ));
    }
    sqlx::query("UPDATE poams SET status=$2 WHERE id=$1")
        .bind(id)
        .bind(request.status.as_str())
        .execute(&mut *tx)
        .await?;
    bump_and_audit(
        &mut tx,
        actor,
        id,
        "status_changed",
        json!({"from":from,"to":request.status,"note":request.note}),
    )
    .await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

pub async fn add_note(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    request: AddNoteRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    let text = request.text.trim();
    if text.is_empty() {
        return Err(PoamError::Validation(
            "invalid_note",
            "Note is required".into(),
        ));
    }
    validate_text_length(text, MAX_NOTE_BYTES, "text_too_long", "note")?;
    let mut tx = pool.begin().await?;
    lock_mutable_poam(&mut tx, actor, id, request.revision).await?;
    bump_and_audit(&mut tx, actor, id, "note", json!({"text":text})).await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

pub async fn add_milestone(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    request: AddMilestoneRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    if request.title.trim().is_empty() {
        return Err(PoamError::Validation(
            "invalid_milestone",
            "Milestone title is required".into(),
        ));
    }
    validate_text_length(
        request.title.trim(),
        MAX_SHORT_TEXT_BYTES,
        "text_too_long",
        "milestone title",
    )?;
    let mut tx = pool.begin().await?;
    lock_mutable_poam(&mut tx, actor, id, request.revision).await?;
    let milestone_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM poam_milestones WHERE poam_id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if milestone_count >= MAX_POAM_RELATIONSHIPS {
        return Err(PoamError::Validation(
            "too_many_milestones",
            "A POA&M can contain at most 100 milestones".into(),
        ));
    }
    let (mid,ordinal):(Uuid,i32)=sqlx::query_as("INSERT INTO poam_milestones(poam_id,ordinal,title,target_date,created_by,updated_by) VALUES($1,(SELECT COALESCE(MAX(ordinal)+1,0) FROM poam_milestones WHERE poam_id=$1),$2,$3,$4,$4) RETURNING id,ordinal")
      .bind(id).bind(request.title.trim()).bind(request.target_date).bind(actor.user_id).fetch_one(&mut *tx).await?;
    bump_and_audit(&mut tx,actor,id,"milestone_added",json!({"milestone_id":mid,"ordinal":ordinal,"title":request.title.trim(),"target_date":request.target_date})).await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

pub async fn update_milestone(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    milestone_id: Uuid,
    request: UpdateMilestoneRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    if request
        .title
        .as_deref()
        .is_some_and(|v| v.trim().is_empty())
    {
        return Err(PoamError::Validation(
            "invalid_milestone",
            "Milestone title cannot be empty".into(),
        ));
    }
    if let Some(title) = request.title.as_deref() {
        validate_text_length(
            title.trim(),
            MAX_SHORT_TEXT_BYTES,
            "text_too_long",
            "milestone title",
        )?;
    }
    let mut tx = pool.begin().await?;
    lock_mutable_poam(&mut tx, actor, id, request.revision).await?;
    let old: Option<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(milestone) FROM poam_milestones milestone WHERE poam_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(milestone_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(old) = old else {
        return Err(PoamError::NotFound);
    };
    sqlx::query("UPDATE poam_milestones SET title=COALESCE($3,title),target_date=COALESCE($4,target_date),completed_at=CASE WHEN $5::bool IS NULL THEN completed_at WHEN $5 THEN COALESCE(completed_at,NOW()) ELSE NULL END,completed_by=CASE WHEN $5::bool IS NULL THEN completed_by WHEN $5 THEN COALESCE(completed_by,$6) ELSE NULL END,updated_by=$6,updated_at=NOW() WHERE poam_id=$1 AND id=$2")
      .bind(id).bind(milestone_id).bind(request.title.as_deref().map(str::trim)).bind(request.target_date).bind(request.completed).bind(actor.user_id).execute(&mut *tx).await?;
    let new: Value = sqlx::query_scalar(
        "SELECT to_jsonb(milestone) FROM poam_milestones milestone WHERE poam_id=$1 AND id=$2",
    )
    .bind(id)
    .bind(milestone_id)
    .fetch_one(&mut *tx)
    .await?;
    if old == new {
        tx.commit().await?;
        return detail(pool, actor, id, clock).await;
    }
    bump_and_audit(
        &mut tx,
        actor,
        id,
        "milestone_updated",
        json!({"milestone_id":milestone_id,"old":old,"new":new}),
    )
    .await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

pub async fn remove_milestone(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    milestone_id: Uuid,
    revision: i64,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    let mut tx = pool.begin().await?;
    lock_mutable_poam(&mut tx, actor, id, revision).await?;
    let old: Option<Value> = sqlx::query_scalar(
        "DELETE FROM poam_milestones WHERE poam_id=$1 AND id=$2 RETURNING to_jsonb(poam_milestones)",
    )
        .bind(id)
        .bind(milestone_id)
        .fetch_optional(&mut *tx)
        .await?
        ;
    let Some(old) = old else {
        return Err(PoamError::NotFound);
    };
    bump_and_audit(
        &mut tx,
        actor,
        id,
        "milestone_removed",
        json!({"milestone_id":milestone_id,"old":old}),
    )
    .await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

pub async fn link_finding(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    request: AddFindingRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    let mut tx = pool.begin().await?;
    let key = assessment_finding_key_tx(&mut tx, request.assessment_id)
        .await?
        .ok_or(PoamError::NotFound)?;
    lock_assessment_finding_key_tx(&mut tx, key).await?;
    let context = assessment_context_tx(&mut tx, request.assessment_id)
        .await?
        .ok_or(PoamError::NotFound)?;
    if !actor_can_access_systems_tx(&mut tx, actor, &[context.system_id]).await? {
        return Err(PoamError::NotFound);
    }
    validate_current_assessment_tx(&mut tx, &context).await?;
    if context.overall_outcome != "fail" {
        return Err(PoamError::Precondition(
            "finding_not_failed",
            "Only current Fail findings can be linked".into(),
            None,
        ));
    }
    // Closure and assessment writers lock finding keys before POA&M rows. Keep
    // link mutation in the same order so a concurrent close cannot deadlock.
    lock_mutable_poam(&mut tx, actor, id, request.revision).await?;
    let finding_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if finding_count >= MAX_POAM_RELATIONSHIPS {
        return Err(PoamError::Validation(
            "too_many_findings",
            "A POA&M can contain at most 100 active findings".into(),
        ));
    }
    let compatible: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
      SELECT 1 FROM poam_finding_links l JOIN poam_findings f ON f.id=l.finding_id
      WHERE l.poam_id=$1 AND l.retired_at IS NULL AND f.policy_lineage_id=$2)"#,
    )
    .bind(id)
    .bind(context.policy_lineage_id)
    .fetch_one(&mut *tx)
    .await?;
    if !compatible {
        return Err(PoamError::Validation(
            "incompatible_finding",
            "Findings must share deployment-policy lineage".into(),
        ));
    }
    if let Err(error) =
        sqlx::query("INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) VALUES($1,$2,$3)")
            .bind(id)
            .bind(context.finding_id)
            .bind(actor.user_id)
            .execute(&mut *tx)
            .await
    {
        return Err(db_conflict(&error).unwrap_or_else(|| error.into()));
    }
    bump_and_audit(&mut tx,actor,id,"finding_linked",json!({"finding_id":context.finding_id,"assessment_id":request.assessment_id,"system_id":context.system_id,"policy_lineage_id":context.policy_lineage_id})).await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

pub async fn unlink_finding(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    finding_id: Uuid,
    revision: i64,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    let mut tx = pool.begin().await?;
    lock_mutable_poam(&mut tx, actor, id, revision).await?;
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if active_count <= 1 {
        return Err(PoamError::Validation(
            "finding_required",
            "A POA&M must retain at least one finding".into(),
        ));
    }
    let affected=sqlx::query("UPDATE poam_finding_links SET retired_at=NOW(),retired_by=$3,retirement_reason='unlinked' WHERE poam_id=$1 AND finding_id=$2 AND retired_at IS NULL")
      .bind(id).bind(finding_id).bind(actor.user_id).execute(&mut *tx).await?.rows_affected();
    if affected == 0 {
        return Err(PoamError::NotFound);
    }
    bump_and_audit(
        &mut tx,
        actor,
        id,
        "finding_unlinked",
        json!({"finding_id":finding_id}),
    )
    .await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

pub async fn link_assignment(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    request: AssignmentReferenceRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    let mut tx = pool.begin().await?;
    lock_mutable_poam(&mut tx, actor, id, request.revision).await?;
    let assignment_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM poam_assignment_references WHERE poam_id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if assignment_count >= MAX_POAM_RELATIONSHIPS {
        return Err(PoamError::Validation(
            "too_many_assignment_references",
            "A POA&M can contain at most 100 assignment references".into(),
        ));
    }
    validate_assignment_refs_tx(&mut tx, actor, &[request.assignment_version_id]).await?;
    let finding_contexts = sqlx::query_as::<_, (Uuid, Uuid)>(
        r#"SELECT finding.system_id,finding.policy_lineage_id
           FROM poam_finding_links link
           JOIN poam_findings finding ON finding.id=link.finding_id
           WHERE link.poam_id=$1 AND link.retired_at IS NULL
           ORDER BY finding.system_id,finding.policy_lineage_id"#,
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    validate_assignment_compatibility_tx(
        &mut tx,
        &[request.assignment_version_id],
        &finding_contexts,
    )
    .await?;
    let inserted = sqlx::query("INSERT INTO poam_assignment_references(poam_id,assignment_id,assignment_version_id,added_by) SELECT $1,assignment_id,id,$3 FROM compliance_bundle_assignment_versions WHERE id=$2 ON CONFLICT DO NOTHING")
      .bind(id).bind(request.assignment_version_id).bind(actor.user_id).execute(&mut *tx).await?;
    if inserted.rows_affected() == 0 {
        tx.commit().await?;
        return detail(pool, actor, id, clock).await;
    }
    bump_and_audit(
        &mut tx,
        actor,
        id,
        "assignment_linked",
        json!({"assignment_version_id":request.assignment_version_id}),
    )
    .await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

pub async fn unlink_assignment(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    assignment_version_id: Uuid,
    revision: i64,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    let mut tx = pool.begin().await?;
    lock_mutable_poam(&mut tx, actor, id, revision).await?;
    validate_assignment_refs_tx(&mut tx, actor, &[assignment_version_id]).await?;
    if sqlx::query(
        "DELETE FROM poam_assignment_references WHERE poam_id=$1 AND assignment_version_id=$2",
    )
    .bind(id)
    .bind(assignment_version_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 0
    {
        return Err(PoamError::NotFound);
    }
    bump_and_audit(
        &mut tx,
        actor,
        id,
        "assignment_unlinked",
        json!({"assignment_version_id":assignment_version_id}),
    )
    .await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

pub async fn compatible(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    q: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Page<CompatibleFinding>, PoamError> {
    let (limit, offset) = page_bounds(Some(limit), Some(offset))?;
    let q = q.map(str::trim).filter(|value| !value.is_empty());
    if let Some(q) = q {
        validate_text_length(q, MAX_SEARCH_BYTES, "search_too_long", "search")?;
    }
    require_visible(pool, actor, id).await?;
    let needed = offset
        .checked_add(limit)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            PoamError::Validation("invalid_offset", "Pagination range overflowed".into())
        })?;
    let mut valid = Vec::new();
    let mut candidate_offset = 0;
    loop {
        let mut candidates = poam::compatible_findings(
            pool,
            id,
            q,
            100,
            candidate_offset,
            actor.is_admin,
            &actor.environment_ids,
        )
        .await?;
        if candidates.is_empty() {
            break;
        }
        let candidate_count = candidates.len() as i64;
        let tuples = candidates
            .iter()
            .map(|finding| {
                (
                    finding.finding_id,
                    finding.system_id,
                    finding.policy_lineage_id,
                )
            })
            .collect::<Vec<_>>();
        let mut tx = pool.begin().await?;
        let items = current_verification_items_tx(&mut tx, &tuples, Utc::now()).await?;
        tx.commit().await?;
        candidates.retain_mut(|finding| {
            let Some(item) = items.iter().find(|item| {
                item.finding_id == finding.finding_id
                    && item.observed_outcome.as_deref() == Some("fail")
                    && !matches!(item.result.as_str(), "stale" | "missing")
            }) else {
                return false;
            };
            finding.assessment_id = item.assessment_id;
            finding.outcome = item.observed_outcome.clone();
            true
        });
        valid.extend(candidates);
        if valid.len() as i64 >= needed || candidate_count < 100 {
            break;
        }
        candidate_offset += candidate_count;
        if candidate_offset >= MAX_CANDIDATES_SCANNED {
            return Err(PoamError::Validation(
                "candidate_scan_limit",
                "The query is too broad; add a narrower search".into(),
            ));
        }
    }
    let has_more = valid.len() as i64 > offset + limit;
    let items = valid
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();
    Ok(Page {
        items,
        limit,
        offset,
        has_more,
        next_offset: has_more.then_some(offset + limit),
    })
}

pub async fn finding_relationships(
    pool: &PgPool,
    actor: &PoamActor,
    assessment_ids: &[Uuid],
    clock: &dyn PoamClock,
) -> Result<Vec<FindingPoamRelationship>, PoamError> {
    if assessment_ids.is_empty() || assessment_ids.len() > MAX_POAM_RELATIONSHIPS as usize {
        return Err(PoamError::Validation(
            "invalid_assessment_ids",
            "Between 1 and 100 assessment IDs are required".into(),
        ));
    }
    let candidates = poam::visible_assessment_findings(
        pool,
        assessment_ids,
        actor.is_admin,
        &actor.environment_ids,
    )
    .await?;
    let tuples = candidates
        .iter()
        .map(|row| (row.1, row.2, row.3))
        .collect::<Vec<_>>();
    let authoritative = if tuples.is_empty() {
        Vec::new()
    } else {
        let mut tx = pool.begin().await?;
        let items = current_verification_items_tx(&mut tx, &tuples, clock.now()).await?;
        tx.commit().await?;
        items
    };
    let visible = candidates
        .into_iter()
        .filter(|candidate| {
            authoritative.iter().any(|item| {
                item.finding_id == candidate.1
                    && item.assessment_id == Some(candidate.0)
                    && !matches!(item.result.as_str(), "stale" | "missing")
            })
        })
        .collect::<Vec<_>>();
    let finding_ids = visible.iter().map(|row| row.1).collect::<Vec<_>>();
    let summaries = poam::finding_poam_summaries(
        pool,
        &finding_ids,
        clock.today(),
        actor.is_admin,
        &actor.environment_ids,
    )
    .await?;
    Ok(visible
        .into_iter()
        .map(|(assessment_id, finding_id, _, _)| {
            let active_poam = summaries
                .iter()
                .find(|(related_finding, active, _)| *related_finding == finding_id && *active)
                .map(|(_, _, summary)| summary.clone());
            let active_id = active_poam.as_ref().map(|summary| summary.id);
            let mut seen = BTreeSet::new();
            FindingPoamRelationship {
                assessment_id,
                finding_id,
                active_poam,
                historical_poams: summaries
                    .iter()
                    .filter(|(related_finding, active, summary)| {
                        *related_finding == finding_id
                            && !*active
                            && Some(summary.id) != active_id
                            && seen.insert(summary.id)
                    })
                    .map(|(_, _, summary)| summary.clone())
                    .collect(),
            }
        })
        .collect())
}

pub async fn compatible_for_assessment(
    pool: &PgPool,
    actor: &PoamActor,
    assessment_id: Uuid,
    q: Option<&str>,
    limit: Option<i64>,
    offset: Option<i64>,
    clock: &dyn PoamClock,
) -> Result<Page<PoamSummary>, PoamError> {
    let (limit, offset) = page_bounds(limit, offset)?;
    let q = q.map(str::trim).filter(|value| !value.is_empty());
    if let Some(q) = q {
        validate_text_length(q, MAX_SEARCH_BYTES, "search_too_long", "search")?;
    }
    let mut tx = pool.begin().await?;
    let context = assessment_context_tx(&mut tx, assessment_id)
        .await?
        .ok_or(PoamError::NotFound)?;
    if !actor_can_access_systems_tx(&mut tx, actor, &[context.system_id]).await? {
        return Err(PoamError::NotFound);
    }
    validate_current_assessment_tx(&mut tx, &context).await?;
    if context.overall_outcome != "fail" {
        return Err(PoamError::Precondition(
            "finding_not_failed",
            "Only a current Fail finding can search compatible POA&Ms".into(),
            None,
        ));
    }
    tx.commit().await?;
    let mut items = poam::compatible_poams(
        pool,
        context.finding_id,
        context.policy_lineage_id,
        q,
        clock.today(),
        limit,
        offset,
        actor.is_admin,
        &actor.environment_ids,
    )
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

pub async fn assignment_relationships(
    pool: &PgPool,
    actor: &PoamActor,
    assignment_version_ids: &[Uuid],
    clock: &dyn PoamClock,
) -> Result<Vec<AssignmentPoamRelationship>, PoamError> {
    if assignment_version_ids.is_empty()
        || assignment_version_ids.len() > MAX_POAM_RELATIONSHIPS as usize
    {
        return Err(PoamError::Validation(
            "invalid_assignment_version_ids",
            "Between 1 and 100 assignment-version IDs are required".into(),
        ));
    }
    let visible_ids = poam::visible_assignment_versions(
        pool,
        assignment_version_ids,
        actor.is_admin,
        &actor.environment_ids,
    )
    .await?;
    let summaries = poam::assignment_poam_summaries(
        pool,
        &visible_ids,
        clock.today(),
        actor.is_admin,
        &actor.environment_ids,
    )
    .await?;
    Ok(visible_ids
        .into_iter()
        .map(|assignment_version_id| AssignmentPoamRelationship {
            assignment_version_id,
            poams: summaries
                .iter()
                .filter(|(related_id, _)| *related_id == assignment_version_id)
                .map(|(_, summary)| summary.clone())
                .collect(),
        })
        .collect())
}

pub async fn create_waiver(
    pool: &PgPool,
    actor: &PoamActor,
    request: CreateWaiverRequest,
) -> Result<Value, PoamError> {
    require_mutator(actor)?;
    if request.justification.trim().is_empty() {
        return Err(PoamError::Validation(
            "invalid_justification",
            "Justification is required".into(),
        ));
    }
    validate_text_length(
        request.justification.trim(),
        MAX_NOTE_BYTES,
        "text_too_long",
        "justification",
    )?;
    // Finding writers take the same advisory locks. READ COMMITTED gives each
    // post-wait statement a fresh snapshot of the writer that just committed.
    let mut tx = pool.begin().await?;
    let finding_key = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT system_id,policy_lineage_id FROM poam_findings WHERE id=$1",
    )
    .bind(request.finding_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(PoamError::NotFound)?;
    sqlx::query("SELECT lock_poam_finding_key($1,$2)")
        .bind(finding_key.0)
        .bind(finding_key.1)
        .execute(&mut *tx)
        .await?;
    if !actor_can_access_systems_tx(&mut tx, actor, &[finding_key.0]).await? {
        return Err(PoamError::NotFound);
    }
    let context = assessment_context_tx(&mut tx, request.assessment_id)
        .await?
        .ok_or(PoamError::NotFound)?;
    if context.finding_id != request.finding_id {
        return Err(PoamError::Validation(
            "waiver_wrong_context",
            "Assessment does not belong to the finding".into(),
        ));
    }
    if !actor_can_access_systems_tx(&mut tx, actor, &[context.system_id]).await? {
        return Err(PoamError::NotFound);
    }
    validate_current_assessment_tx(&mut tx, &context).await?;
    if context.overall_outcome != "fail" {
        return Err(PoamError::Precondition(
            "finding_not_failed",
            "A waiver can only be requested for a current Fail finding".into(),
            None,
        ));
    }
    let observation_snapshot = observation_snapshot_tx(&mut tx, request.assessment_id)
        .await?
        .ok_or(PoamError::NotFound)?;
    let observation_token = semantic_digest(&observation_snapshot);
    let waiver_id:Uuid=sqlx::query_scalar("INSERT INTO finding_waivers(finding_id,justification,policy_version_id,assessment_id,observation_token,observation_snapshot,created_by) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
      .bind(request.finding_id).bind(request.justification.trim()).bind(context.policy_version_id).bind(request.assessment_id).bind(&observation_token).bind(&observation_snapshot).bind(actor.user_id).fetch_one(&mut *tx).await?;
    let payload = json!({"waiver_id":waiver_id,"finding_id":request.finding_id,"assessment_id":request.assessment_id,"status":"pending"});
    sqlx::query("INSERT INTO finding_waiver_events(waiver_id,actor_user_id,to_status,payload) VALUES($1,$2,'pending',$3)")
      .bind(waiver_id).bind(actor.user_id).bind(&payload).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO admin_audit_events(actor_user_id,actor_identifier,action,target,request_origin,metadata) VALUES($1,$2,'finding_waiver_created',$3,$4,$5)")
      .bind(actor.user_id).bind(&actor.identifier).bind(format!("finding:{}",request.finding_id)).bind(actor.request_origin.as_deref()).bind(&payload).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(payload)
}

pub async fn list_waivers(
    pool: &PgPool,
    actor: &PoamActor,
    query: &WaiverListQuery,
) -> Result<Page<WaiverView>, PoamError> {
    if !actor.is_admin {
        return Err(PoamError::Forbidden);
    }
    if query.status.as_deref().is_some_and(|status| {
        !matches!(
            status,
            "pending" | "accepted" | "rejected" | "expired" | "revoked"
        )
    }) {
        return Err(PoamError::Validation(
            "invalid_waiver_status",
            "Unknown waiver status".into(),
        ));
    }
    page_bounds(query.limit, query.offset)?;
    Ok(poam::list_waivers(pool, query).await?)
}

pub async fn waiver(pool: &PgPool, actor: &PoamActor, id: Uuid) -> Result<WaiverView, PoamError> {
    if !actor.is_admin {
        return Err(PoamError::Forbidden);
    }
    poam::waiver(pool, id).await?.ok_or(PoamError::NotFound)
}

pub async fn decide_waiver(
    pool: &PgPool,
    actor: &PoamActor,
    waiver_id: Uuid,
    request: WaiverDecisionRequest,
    clock: &dyn PoamClock,
) -> Result<Value, PoamError> {
    if !actor.is_admin {
        return Err(PoamError::Forbidden);
    }
    let decision = request.status.as_str();
    if decision != "accepted" && request.expires_at.is_some() {
        return Err(PoamError::Validation(
            "invalid_expiry",
            "expires_at is only valid when accepting a waiver".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    let key=sqlx::query_as::<_,(Uuid,Uuid)>("SELECT f.system_id,f.policy_lineage_id FROM finding_waivers w JOIN poam_findings f ON f.id=w.finding_id WHERE w.id=$1")
      .bind(waiver_id).fetch_optional(&mut *tx).await?.ok_or(PoamError::NotFound)?;
    sqlx::query("SELECT lock_poam_finding_key($1,$2)")
        .bind(key.0)
        .bind(key.1)
        .execute(&mut *tx)
        .await?;
    let row=sqlx::query_as::<_,(Uuid,String,Uuid,Uuid,String)>("SELECT w.finding_id,w.status,f.system_id,w.assessment_id,w.observation_token FROM finding_waivers w JOIN poam_findings f ON f.id=w.finding_id WHERE w.id=$1 FOR UPDATE OF w")
      .bind(waiver_id).fetch_optional(&mut *tx).await?.ok_or(PoamError::NotFound)?;
    if !actor_can_access_systems_tx(&mut tx, actor, &[row.2]).await? {
        return Err(PoamError::NotFound);
    }
    let allowed = matches!(
        (row.1.as_str(), decision),
        ("pending", "accepted" | "rejected") | ("accepted", "revoked" | "expired")
    );
    if !allowed {
        return Err(PoamError::Conflict(
            "invalid_waiver_transition",
            format!("Cannot transition waiver from {} to {}", row.1, decision),
        ));
    }
    if decision == "accepted" && request.expires_at.is_some_and(|at| at <= clock.now()) {
        return Err(PoamError::Validation(
            "invalid_expiry",
            "Accepted waiver expiry must be in the future".into(),
        ));
    }
    if decision == "accepted" {
        let context = assessment_context_tx(&mut tx, row.3)
            .await?
            .ok_or(PoamError::NotFound)?;
        if context.finding_id != row.0 || context.overall_outcome != "fail" {
            return Err(PoamError::Precondition(
                "waiver_wrong_context",
                "Only the exact current Fail finding context can be accepted".into(),
                None,
            ));
        }
        validate_current_assessment_tx(&mut tx, &context).await?;
        if observation_snapshot_tx(&mut tx, row.3)
            .await?
            .map(|snapshot| semantic_digest(&snapshot))
            .as_deref()
            != Some(row.4.as_str())
        {
            return Err(PoamError::Precondition(
                "waiver_observation_changed",
                "The exact Fail observation changed after waiver submission".into(),
                None,
            ));
        }
        let expired_ids=sqlx::query_scalar::<_,Uuid>("UPDATE finding_waivers SET status='expired',updated_at=$2 WHERE finding_id=$1 AND status='accepted' AND expires_at<=$2 RETURNING id")
            .bind(row.0).bind(clock.now()).fetch_all(&mut *tx).await?;
        for expired_id in expired_ids {
            let expired_payload = json!({"waiver_id":expired_id,"finding_id":row.0,"from":"accepted","to":"expired","reason":"elapsed"});
            sqlx::query("INSERT INTO finding_waiver_events(waiver_id,actor_user_id,from_status,to_status,payload) VALUES($1,$2,'accepted','expired',$3)")
                .bind(expired_id).bind(actor.user_id).bind(&expired_payload).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO admin_audit_events(actor_user_id,actor_identifier,action,target,request_origin,metadata) VALUES($1,$2,'finding_waiver_status_changed',$3,$4,$5)")
                .bind(actor.user_id).bind(&actor.identifier).bind(format!("finding:{}",row.0)).bind(actor.request_origin.as_deref()).bind(&expired_payload).execute(&mut *tx).await?;
        }
    }
    if let Err(error) = sqlx::query(r#"UPDATE finding_waivers SET status=$2,accepted_by=CASE WHEN $2='accepted' THEN $3 ELSE accepted_by END,
      accepted_at=CASE WHEN $2='accepted' THEN $4 ELSE accepted_at END,expires_at=CASE WHEN $2='accepted' THEN $5 ELSE expires_at END,updated_at=$4 WHERE id=$1"#)
      .bind(waiver_id).bind(decision).bind(actor.user_id).bind(clock.now()).bind(request.expires_at).execute(&mut *tx).await {
        return Err(db_conflict(&error).unwrap_or_else(|| error.into()));
    }
    let payload = json!({"waiver_id":waiver_id,"finding_id":row.0,"from":row.1,"to":decision,"expires_at":request.expires_at});
    sqlx::query("INSERT INTO finding_waiver_events(waiver_id,actor_user_id,from_status,to_status,payload) VALUES($1,$2,$3,$4,$5)")
      .bind(waiver_id).bind(actor.user_id).bind(&row.1).bind(decision).bind(&payload).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO admin_audit_events(actor_user_id,actor_identifier,action,target,request_origin,metadata) VALUES($1,$2,'finding_waiver_status_changed',$3,$4,$5)")
      .bind(actor.user_id).bind(&actor.identifier).bind(format!("finding:{}",row.0)).bind(actor.request_origin.as_deref()).bind(&payload).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(payload)
}

#[derive(Debug)]
struct VerificationItem {
    finding_id: Uuid,
    system_id: Uuid,
    policy_lineage_id: Uuid,
    result: String,
    policy_version_id: Option<Uuid>,
    assessment_id: Option<Uuid>,
    derivation_id: Option<i32>,
    target_store_path: Option<String>,
    effective_set_digest: Option<String>,
    effective_config_digest: Option<String>,
    effective_config: Option<Value>,
    observed_outcome: Option<String>,
    observation_token: Option<String>,
    observation_snapshot: Option<Value>,
    assessment_updated_at: Option<DateTime<Utc>>,
    bundle_ids: Vec<Uuid>,
    bundle_version_ids: Vec<Uuid>,
    requirement_version_ids: Vec<Uuid>,
    waiver_id: Option<Uuid>,
    detail: String,
}

#[derive(Debug, sqlx::FromRow)]
struct AssessmentObservation {
    system_id: Uuid,
    policy_lineage_id: Uuid,
    policy_version_id: Uuid,
    assessment_id: Uuid,
    derivation_id: i32,
    target_store_path: String,
    effective_set_digest: String,
    effective_config_digest: String,
    overall_outcome: String,
    effective_config: Value,
    assessment_updated_at: DateTime<Utc>,
    observation_snapshot: Value,
}

fn closure_result_is_accepted(result: &str) -> bool {
    match result {
        "pass" | "waiver" => true,
        "fail" | "error" | "not_checked" | "missing" | "stale" | "unknown" | "warn"
        | "not_applicable" => false,
        _ => false,
    }
}

async fn current_verification_items_tx(
    tx: &mut Transaction<'_, Postgres>,
    findings: &[(Uuid, Uuid, Uuid)],
    now: DateTime<Utc>,
) -> Result<Vec<VerificationItem>, PoamError> {
    let system_ids = findings.iter().map(|row| row.1).collect::<Vec<_>>();
    let lineage_ids = findings.iter().map(|row| row.2).collect::<Vec<_>>();
    sqlx::query(
        r#"SELECT lock_poam_finding_key(key.system_id, key.policy_lineage_id)
           FROM (
             SELECT DISTINCT input.system_id, input.policy_lineage_id
             FROM UNNEST($1::uuid[], $2::uuid[]) AS input(system_id, policy_lineage_id)
             ORDER BY input.system_id, input.policy_lineage_id
           ) key"#,
    )
    .bind(&system_ids)
    .bind(&lineage_ids)
    .execute(&mut **tx)
    .await?;

    let resolved_by_system = resolve_systems_effective_policies_in_tx(tx, &system_ids).await?;
    let policy_version_ids = resolved_by_system
        .values()
        .flat_map(|outcome| match outcome {
            ResolutionOutcome::Resolved(set) => set
                .policies
                .iter()
                .map(|policy| policy.policy_version_id)
                .collect::<Vec<_>>(),
            ResolutionOutcome::Conflict(_) => Vec::new(),
        })
        .collect::<Vec<_>>();
    let requirement_rows=sqlx::query_as::<_,(Uuid,Uuid)>("SELECT policy_version_id,requirement_version_id FROM policy_requirement_mappings WHERE policy_version_id=ANY($1) ORDER BY policy_version_id,requirement_version_id")
      .bind(&policy_version_ids).fetch_all(&mut **tx).await?;
    let mut requirements_by_policy = HashMap::<Uuid, Vec<Uuid>>::new();
    for (policy_version_id, requirement_version_id) in requirement_rows {
        requirements_by_policy
            .entry(policy_version_id)
            .or_default()
            .push(requirement_version_id);
    }

    // If a writer committed while this serializable transaction waited on the
    // advisory key, locking its old snapshot forces PostgreSQL to retry rather
    // than allowing closure to trust the pre-wait assessment image.
    sqlx::query(
        r#"SELECT a.id FROM composite_policy_assessments a
           JOIN UNNEST($1::uuid[],$2::uuid[]) key(system_id,policy_lineage_id)
             ON key.system_id=a.system_id AND key.policy_lineage_id=a.policy_lineage_id
           ORDER BY a.system_id,a.policy_lineage_id,a.id FOR SHARE OF a"#,
    )
    .bind(&system_ids)
    .bind(&lineage_ids)
    .execute(&mut **tx)
    .await?;

    let observations = sqlx::query_as::<_, AssessmentObservation>(
        r#"SELECT a.system_id,a.policy_lineage_id,a.policy_version_id,a.id AS assessment_id,
             a.derivation_id,a.target_store_path,a.effective_set_digest,
              a.effective_config_digest,a.overall_outcome,a.effective_config,a.updated_at AS assessment_updated_at,
             jsonb_build_object('assessment',to_jsonb(a),'rules',COALESCE((
               SELECT jsonb_agg(to_jsonb(result) ORDER BY result.ordinal,result.rule_id)
               FROM composite_policy_rule_results result WHERE result.assessment_id=a.id
             ),'[]'::jsonb)) AS observation_snapshot
           FROM composite_policy_assessments a
           JOIN UNNEST($1::uuid[],$2::uuid[]) key(system_id,policy_lineage_id)
             ON key.system_id=a.system_id AND key.policy_lineage_id=a.policy_lineage_id
           JOIN systems s ON s.id=a.system_id
           JOIN LATERAL (
             SELECT ss.store_path FROM system_states ss
             WHERE ss.hostname=s.hostname AND ss.store_path IS NOT NULL AND btrim(ss.store_path)<>''
             ORDER BY ss.timestamp DESC,ss.id DESC LIMIT 1
           ) deployed ON deployed.store_path=a.target_store_path
           ORDER BY a.system_id,a.policy_lineage_id,a.updated_at DESC,a.id DESC"#,
    )
    .bind(&system_ids)
    .bind(&lineage_ids)
    .fetch_all(&mut **tx)
    .await?;
    let assessment_ids = observations
        .iter()
        .map(|observation| observation.assessment_id)
        .collect::<Vec<_>>();
    let waiver_rows = if assessment_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as::<_, (Uuid, Uuid, Uuid, Uuid, String)>(
            r#"SELECT finding_id,assessment_id,policy_version_id,id,observation_token FROM finding_waivers
               WHERE assessment_id=ANY($1) AND status='accepted'
                 AND (expires_at IS NULL OR expires_at>$2)
               ORDER BY finding_id,assessment_id,accepted_at DESC FOR SHARE"#,
        )
        .bind(&assessment_ids)
        .bind(now)
        .fetch_all(&mut **tx)
        .await?
    };
    let waivers = waiver_rows
        .into_iter()
        .map(
            |(finding_id, assessment_id, policy_version_id, waiver_id, observation_token)| {
                (
                    (
                        finding_id,
                        assessment_id,
                        policy_version_id,
                        observation_token,
                    ),
                    waiver_id,
                )
            },
        )
        .collect::<HashMap<_, _>>();

    let mut items = Vec::with_capacity(findings.len());
    for (finding_id, system_id, lineage_id) in findings {
        let Some(ResolutionOutcome::Resolved(resolved)) = resolved_by_system.get(system_id) else {
            items.push(VerificationItem {
                finding_id: *finding_id,
                system_id: *system_id,
                policy_lineage_id: *lineage_id,
                result: "stale".into(),
                policy_version_id: None,
                assessment_id: None,
                derivation_id: None,
                target_store_path: None,
                effective_set_digest: None,
                effective_config_digest: None,
                effective_config: None,
                observed_outcome: None,
                observation_token: None,
                observation_snapshot: None,
                assessment_updated_at: None,
                bundle_ids: Vec::new(),
                bundle_version_ids: Vec::new(),
                requirement_version_ids: Vec::new(),
                waiver_id: None,
                detail: "Current policy resolution conflict".into(),
            });
            continue;
        };
        let Some(policy) = resolved
            .policies
            .iter()
            .find(|p| p.policy_lineage_id == *lineage_id)
        else {
            items.push(VerificationItem {
                finding_id: *finding_id,
                system_id: *system_id,
                policy_lineage_id: *lineage_id,
                result: "stale".into(),
                policy_version_id: None,
                assessment_id: None,
                derivation_id: None,
                target_store_path: None,
                effective_set_digest: None,
                effective_config_digest: None,
                effective_config: None,
                observed_outcome: None,
                observation_token: None,
                observation_snapshot: None,
                assessment_updated_at: None,
                bundle_ids: Vec::new(),
                bundle_version_ids: Vec::new(),
                requirement_version_ids: Vec::new(),
                waiver_id: None,
                detail: "Policy is no longer effective".into(),
            });
            continue;
        };
        let current_observations = observations.iter().filter(|observation| {
            observation.system_id == *system_id
                && observation.policy_lineage_id == *lineage_id
                && observation.policy_version_id == policy.policy_version_id
        });
        let exact_observation = current_observations.clone().find(|observation| {
            observation.effective_set_digest == resolved.effective_set_digest
                && observation.effective_config_digest == semantic_digest(&policy.effective_config)
                && observation.effective_config == policy.effective_config
        });
        let Some(observation) = exact_observation.or_else(|| current_observations.clone().next())
        else {
            items.push(VerificationItem {
                finding_id: *finding_id,
                system_id: *system_id,
                policy_lineage_id: *lineage_id,
                result: "missing".into(),
                policy_version_id: Some(policy.policy_version_id),
                assessment_id: None,
                derivation_id: None,
                target_store_path: None,
                effective_set_digest: None,
                effective_config_digest: None,
                effective_config: Some(policy.effective_config.clone()),
                observed_outcome: None,
                observation_token: None,
                observation_snapshot: None,
                assessment_updated_at: None,
                bundle_ids: policy
                    .provenance
                    .iter()
                    .filter(|entry| entry.authoritative)
                    .filter_map(|entry| entry.bundle_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                bundle_version_ids: policy
                    .provenance
                    .iter()
                    .filter(|entry| entry.authoritative)
                    .filter_map(|entry| entry.bundle_version_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                requirement_version_ids: requirements_by_policy
                    .get(&policy.policy_version_id)
                    .cloned()
                    .unwrap_or_default(),
                waiver_id: None,
                detail: "No assessment for the current deployed target".into(),
            });
            continue;
        };
        let observation_token = semantic_digest(&observation.observation_snapshot);
        let exact = observation.effective_set_digest == resolved.effective_set_digest
            && observation.effective_config_digest == semantic_digest(&policy.effective_config)
            && observation.effective_config == policy.effective_config;
        if !exact {
            items.push(VerificationItem {
                finding_id: *finding_id,
                system_id: *system_id,
                policy_lineage_id: *lineage_id,
                result: "stale".into(),
                policy_version_id: Some(policy.policy_version_id),
                assessment_id: Some(observation.assessment_id),
                derivation_id: Some(observation.derivation_id),
                target_store_path: Some(observation.target_store_path.clone()),
                effective_set_digest: Some(observation.effective_set_digest.clone()),
                effective_config_digest: Some(observation.effective_config_digest.clone()),
                effective_config: Some(observation.effective_config.clone()),
                observed_outcome: Some(observation.overall_outcome.clone()),
                observation_token: Some(observation_token.clone()),
                observation_snapshot: Some(observation.observation_snapshot.clone()),
                assessment_updated_at: Some(observation.assessment_updated_at),
                bundle_ids: policy
                    .provenance
                    .iter()
                    .filter(|entry| entry.authoritative)
                    .filter_map(|entry| entry.bundle_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                bundle_version_ids: policy
                    .provenance
                    .iter()
                    .filter(|entry| entry.authoritative)
                    .filter_map(|entry| entry.bundle_version_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                requirement_version_ids: requirements_by_policy
                    .get(&policy.policy_version_id)
                    .cloned()
                    .unwrap_or_default(),
                waiver_id: None,
                detail: "Assessment effective policy context is stale".into(),
            });
            continue;
        }
        if observation.overall_outcome == "pass" {
            items.push(VerificationItem {
                finding_id: *finding_id,
                system_id: *system_id,
                policy_lineage_id: *lineage_id,
                result: "pass".into(),
                policy_version_id: Some(policy.policy_version_id),
                assessment_id: Some(observation.assessment_id),
                derivation_id: Some(observation.derivation_id),
                target_store_path: Some(observation.target_store_path.clone()),
                effective_set_digest: Some(observation.effective_set_digest.clone()),
                effective_config_digest: Some(observation.effective_config_digest.clone()),
                effective_config: Some(observation.effective_config.clone()),
                observed_outcome: Some(observation.overall_outcome.clone()),
                observation_token: Some(observation_token.clone()),
                observation_snapshot: Some(observation.observation_snapshot.clone()),
                assessment_updated_at: Some(observation.assessment_updated_at),
                bundle_ids: policy
                    .provenance
                    .iter()
                    .filter(|entry| entry.authoritative)
                    .filter_map(|entry| entry.bundle_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                bundle_version_ids: policy
                    .provenance
                    .iter()
                    .filter(|entry| entry.authoritative)
                    .filter_map(|entry| entry.bundle_version_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                requirement_version_ids: requirements_by_policy
                    .get(&policy.policy_version_id)
                    .cloned()
                    .unwrap_or_default(),
                waiver_id: None,
                detail: "Exact current assessment passed".into(),
            });
            continue;
        }
        if observation.overall_outcome == "fail"
            && let Some(waiver_id) = waivers
                .get(&(
                    *finding_id,
                    observation.assessment_id,
                    policy.policy_version_id,
                    observation_token.clone(),
                ))
                .copied()
        {
            items.push(VerificationItem {
                finding_id: *finding_id,
                system_id: *system_id,
                policy_lineage_id: *lineage_id,
                result: "waiver".into(),
                policy_version_id: Some(policy.policy_version_id),
                assessment_id: Some(observation.assessment_id),
                derivation_id: Some(observation.derivation_id),
                target_store_path: Some(observation.target_store_path.clone()),
                effective_set_digest: Some(observation.effective_set_digest.clone()),
                effective_config_digest: Some(observation.effective_config_digest.clone()),
                effective_config: Some(observation.effective_config.clone()),
                observed_outcome: Some(observation.overall_outcome.clone()),
                observation_token: Some(observation_token.clone()),
                observation_snapshot: Some(observation.observation_snapshot.clone()),
                assessment_updated_at: Some(observation.assessment_updated_at),
                bundle_ids: policy
                    .provenance
                    .iter()
                    .filter(|entry| entry.authoritative)
                    .filter_map(|entry| entry.bundle_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                bundle_version_ids: policy
                    .provenance
                    .iter()
                    .filter(|entry| entry.authoritative)
                    .filter_map(|entry| entry.bundle_version_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                requirement_version_ids: requirements_by_policy
                    .get(&policy.policy_version_id)
                    .cloned()
                    .unwrap_or_default(),
                waiver_id: Some(waiver_id),
                detail: "Exact current finding has an accepted waiver".into(),
            });
        } else {
            items.push(VerificationItem {
                finding_id: *finding_id,
                system_id: *system_id,
                policy_lineage_id: *lineage_id,
                result: observation.overall_outcome.clone(),
                policy_version_id: Some(policy.policy_version_id),
                assessment_id: Some(observation.assessment_id),
                derivation_id: Some(observation.derivation_id),
                target_store_path: Some(observation.target_store_path.clone()),
                effective_set_digest: Some(observation.effective_set_digest.clone()),
                effective_config_digest: Some(observation.effective_config_digest.clone()),
                effective_config: Some(observation.effective_config.clone()),
                observed_outcome: Some(observation.overall_outcome.clone()),
                observation_token: Some(observation_token),
                observation_snapshot: Some(observation.observation_snapshot.clone()),
                assessment_updated_at: Some(observation.assessment_updated_at),
                bundle_ids: policy
                    .provenance
                    .iter()
                    .filter(|entry| entry.authoritative)
                    .filter_map(|entry| entry.bundle_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                bundle_version_ids: policy
                    .provenance
                    .iter()
                    .filter(|entry| entry.authoritative)
                    .filter_map(|entry| entry.bundle_version_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                requirement_version_ids: requirements_by_policy
                    .get(&policy.policy_version_id)
                    .cloned()
                    .unwrap_or_default(),
                waiver_id: None,
                detail: format!(
                    "Current assessment outcome is {}",
                    observation.overall_outcome
                ),
            });
        }
    }
    Ok(items)
}

async fn insert_verification_items(
    tx: &mut Transaction<'_, Postgres>,
    attempt_id: Uuid,
    items: &[VerificationItem],
    now: DateTime<Utc>,
) -> Result<(), PoamError> {
    let mut builder = sqlx::QueryBuilder::<Postgres>::new(
        "INSERT INTO poam_verification_items(attempt_id,finding_id,system_id,policy_lineage_id,result,policy_version_id,assessment_id,derivation_id,target_store_path,effective_set_digest,effective_config_digest,effective_config,observed_outcome,observation_token,observation_snapshot,assessment_updated_at,bundle_ids,bundle_version_ids,requirement_version_ids,waiver_id,observed_at,detail) ",
    );
    builder.push_values(items, |mut row, item| {
        row.push_bind(attempt_id)
            .push_bind(item.finding_id)
            .push_bind(item.system_id)
            .push_bind(item.policy_lineage_id)
            .push_bind(&item.result)
            .push_bind(item.policy_version_id)
            .push_bind(item.assessment_id)
            .push_bind(item.derivation_id)
            .push_bind(&item.target_store_path)
            .push_bind(&item.effective_set_digest)
            .push_bind(&item.effective_config_digest)
            .push_bind(&item.effective_config)
            .push_bind(&item.observed_outcome)
            .push_bind(&item.observation_token)
            .push_bind(&item.observation_snapshot)
            .push_bind(item.assessment_updated_at)
            .push_bind(&item.bundle_ids)
            .push_bind(&item.bundle_version_ids)
            .push_bind(&item.requirement_version_ids)
            .push_bind(item.waiver_id)
            .push_bind(now)
            .push_bind(&item.detail);
    });
    if !items.is_empty() {
        builder.build().execute(&mut **tx).await?;
    }
    Ok(())
}

fn is_serialization_failure(error: &anyhow::Error) -> bool {
    matches!(
        error
            .downcast_ref::<sqlx::Error>()
            .and_then(|error| error.as_database_error())
            .and_then(|error| error.code())
            .as_deref(),
        Some("40001" | "40P01")
    )
}

pub async fn verify(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    revision: i64,
    clock: &dyn PoamClock,
) -> Result<Value, PoamError> {
    for retry in 0..3 {
        match verify_once(pool, actor, id, revision, clock).await {
            Err(PoamError::Database(error)) if retry < 2 && is_serialization_failure(&error) => {
                continue;
            }
            result => return result,
        }
    }
    unreachable!()
}

async fn verify_once(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    revision: i64,
    clock: &dyn PoamClock,
) -> Result<Value, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    let expected_findings = poam_finding_keys(pool, id).await?;
    // Acquire the shared writer locks before reading authoritative state. Under
    // READ COMMITTED, statements after a wait see the writer that just committed.
    let mut tx = pool.begin().await?;
    lock_finding_keys_tx(&mut tx, &expected_findings).await?;
    let status = lock_mutable_poam(&mut tx, actor, id, revision).await?;
    if status != "awaiting_verification" {
        return Err(PoamError::Conflict(
            "invalid_transition",
            "POA&M must be awaiting verification before verification".into(),
        ));
    }
    let findings=sqlx::query_as::<_,(Uuid,Uuid,Uuid)>(r#"SELECT f.id,f.system_id,f.policy_lineage_id FROM poam_finding_links l JOIN poam_findings f ON f.id=l.finding_id
      WHERE l.poam_id=$1 AND l.retired_at IS NULL ORDER BY f.system_id,f.policy_lineage_id FOR UPDATE OF l,f"#).bind(id).fetch_all(&mut *tx).await?;
    if findings != expected_findings {
        return Err(PoamError::Conflict(
            "concurrent_finding_change",
            "The active finding set changed; retry the request".into(),
        ));
    }
    if findings.is_empty() {
        return Err(PoamError::Validation(
            "finding_required",
            "POA&M has no active findings".into(),
        ));
    }
    if !actor_can_access_systems_tx(
        &mut tx,
        actor,
        &findings.iter().map(|r| r.1).collect::<Vec<_>>(),
    )
    .await?
    {
        return Err(PoamError::NotFound);
    }
    let now = clock.now();
    let items = current_verification_items_tx(&mut tx, &findings, now).await?;
    let accepted = items
        .iter()
        .all(|item| closure_result_is_accepted(&item.result));
    let attempt_id:Uuid=sqlx::query_scalar("INSERT INTO poam_verification_attempts(poam_id,attempted_by,outcome,poam_revision,attempted_at) VALUES($1,$2,$3,$4,$5) RETURNING id")
      .bind(id).bind(actor.user_id).bind(if accepted{"accepted"}else{"rejected"}).bind(revision).bind(now).fetch_one(&mut *tx).await?;
    insert_verification_items(&mut tx, attempt_id, &items, now).await?;
    sqlx::query("UPDATE poam_verification_attempts SET sealed_at=$2 WHERE id=$1")
        .bind(attempt_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let results=items.iter().map(|item|json!({"finding_id":item.finding_id,"result":item.result,"assessment_id":item.assessment_id,"waiver_id":item.waiver_id})).collect::<Vec<_>>();
    let new_revision=bump_and_audit(&mut tx,actor,id,"verification_attempted",json!({"attempt_id":attempt_id,"outcome":if accepted{"accepted"}else{"rejected"},"items":results})).await?;
    tx.commit().await?;
    Ok(
        json!({"attempt_id":attempt_id,"outcome":if accepted{"accepted"}else{"rejected"},"revision":new_revision,"items":results}),
    )
}

pub async fn close(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    revision: i64,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    for retry in 0..3 {
        match close_once(pool, actor, id, revision, clock).await {
            Err(PoamError::Database(error)) if retry < 2 && is_serialization_failure(&error) => {
                continue;
            }
            result => return result,
        }
    }
    unreachable!()
}

async fn close_once(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    revision: i64,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    let expected_findings = poam_finding_keys(pool, id).await?;
    let result = async {
        // Keep a fresh post-lock snapshot; serializable/repeatable-read would
        // retain the snapshot from before an advisory-lock wait.
        let mut tx = pool.begin().await?;
        lock_finding_keys_tx(&mut tx, &expected_findings).await?;
        let status = lock_mutable_poam(&mut tx, actor, id, revision).await?;
        if status != "awaiting_verification" {
            return Err(PoamError::Conflict(
                "invalid_transition",
                "POA&M must be awaiting verification before close".into(),
            ));
        }
        let findings=sqlx::query_as::<_,(Uuid,Uuid,Uuid)>(r#"SELECT f.id,f.system_id,f.policy_lineage_id FROM poam_finding_links l JOIN poam_findings f ON f.id=l.finding_id
          WHERE l.poam_id=$1 AND l.retired_at IS NULL ORDER BY f.system_id,f.policy_lineage_id FOR UPDATE OF l,f"#).bind(id).fetch_all(&mut *tx).await?;
        if findings != expected_findings {
            return Err(PoamError::Conflict(
                "concurrent_finding_change",
                "The active finding set changed; retry the request".into(),
            ));
        }
        if findings.is_empty() {
            return Err(PoamError::Validation(
                "finding_required",
                "POA&M has no active findings".into(),
            ));
        }
        if !actor_can_access_systems_tx(
            &mut tx,
            actor,
            &findings.iter().map(|r| r.1).collect::<Vec<_>>(),
        )
        .await?
        {
            return Err(PoamError::NotFound);
        }
        let now = clock.now();
        let items = current_verification_items_tx(&mut tx, &findings, now).await?;
        let accepted = items
            .iter()
            .all(|item| closure_result_is_accepted(&item.result));
        let attempt_id:Uuid=sqlx::query_scalar("INSERT INTO poam_verification_attempts(poam_id,attempted_by,outcome,poam_revision,attempted_at) VALUES($1,$2,$3,$4,$5) RETURNING id")
          .bind(id).bind(actor.user_id).bind(if accepted{"accepted"}else{"rejected"}).bind(revision).bind(now).fetch_one(&mut *tx).await?;
        insert_verification_items(&mut tx, attempt_id, &items, now).await?;
        sqlx::query("UPDATE poam_verification_attempts SET sealed_at=$2 WHERE id=$1")
            .bind(attempt_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        let results=items.iter().map(|i|json!({"finding_id":i.finding_id,"result":i.result,"assessment_id":i.assessment_id,"waiver_id":i.waiver_id})).collect::<Vec<_>>();
        let verification_revision=bump_and_audit(&mut tx,actor,id,"verification_attempted",json!({"attempt_id":attempt_id,"outcome":if accepted{"accepted"}else{"rejected"},"items":results})).await?;
        if !accepted {
            tx.commit().await?;
            return Err(PoamError::Precondition(
                "closure_not_ready",
                "Every finding requires an exact current Pass or accepted waiver".into(),
                Some(json!({"attempt_id":attempt_id,"revision":verification_revision,"items":results})),
            ));
        }
        sqlx::query("UPDATE poam_finding_links SET retired_at=$2,retired_by=$3,retirement_reason=$4 WHERE poam_id=$1 AND retired_at IS NULL")
          .bind(id).bind(now).bind(actor.user_id).bind(format!("closed:{attempt_id}")).execute(&mut *tx).await?;
        sqlx::query(
            "UPDATE poams SET status='completed',closed_at=$2,closure_attempt_id=$3 WHERE id=$1",
        )
        .bind(id)
        .bind(now)
        .bind(attempt_id)
        .execute(&mut *tx)
        .await?;
        bump_and_audit(
            &mut tx,
            actor,
            id,
            "closed",
            json!({"attempt_id":attempt_id,"closed_at":now}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    .await;
    result?;
    detail(pool, actor, id, clock).await
}

pub async fn reopen(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    revision: i64,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    let expected = sqlx::query_as::<_, (Uuid, Uuid, Uuid)>(
        r#"SELECT finding.id,finding.system_id,finding.policy_lineage_id
           FROM poams poam
           JOIN poam_finding_links link ON link.poam_id=poam.id
             AND link.retirement_reason='closed:'||poam.closure_attempt_id::text
           JOIN poam_findings finding ON finding.id=link.finding_id
           WHERE poam.id=$1
           ORDER BY finding.system_id,finding.policy_lineage_id"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let mut tx = begin_serializable(pool).await?;
    lock_finding_keys_tx(&mut tx, &expected).await?;
    let row = sqlx::query_as::<_, (i64, String, Option<Uuid>)>(
        "SELECT revision,status,closure_attempt_id FROM poams WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(PoamError::NotFound)?;
    require_poam_contexts_tx(&mut tx, actor, id).await?;
    if row.0 != revision {
        return Err(PoamError::Conflict(
            "stale_revision",
            "POA&M revision is stale".into(),
        ));
    }
    if row.1 != "completed" {
        return Err(PoamError::Conflict(
            "invalid_transition",
            "Only completed POA&M can be reopened".into(),
        ));
    }
    let attempt_id = row.2.ok_or_else(|| {
        PoamError::Database(anyhow::anyhow!("completed POA&M lacks closure attempt"))
    })?;
    let findings=sqlx::query_as::<_,(Uuid,Uuid)>("SELECT f.id,f.system_id FROM poam_finding_links l JOIN poam_findings f ON f.id=l.finding_id WHERE l.poam_id=$1 AND l.retirement_reason=$2 ORDER BY f.id FOR UPDATE OF l,f")
      .bind(id).bind(format!("closed:{attempt_id}")).fetch_all(&mut *tx).await?;
    let actual_ids = findings.iter().map(|row| row.0).collect::<BTreeSet<_>>();
    let expected_ids = expected.iter().map(|row| row.0).collect::<BTreeSet<_>>();
    if actual_ids != expected_ids {
        return Err(PoamError::Conflict(
            "concurrent_finding_change",
            "The closure finding set changed; retry the request".into(),
        ));
    }
    if !actor_can_access_systems_tx(
        &mut tx,
        actor,
        &findings.iter().map(|r| r.1).collect::<Vec<_>>(),
    )
    .await?
    {
        return Err(PoamError::NotFound);
    }
    let claimed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM poam_finding_links WHERE finding_id=ANY($1) AND retired_at IS NULL)")
      .bind(&findings.iter().map(|r|r.0).collect::<Vec<_>>()).fetch_one(&mut *tx).await?;
    if claimed {
        return Err(PoamError::Conflict(
            "finding_already_managed",
            "A finding is now managed by another active POA&M".into(),
        ));
    }
    if let Err(error) = sqlx::query("INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) SELECT $1,finding_id,$3 FROM poam_finding_links WHERE poam_id=$1 AND retirement_reason=$2")
        .bind(id).bind(format!("closed:{attempt_id}")).bind(actor.user_id)
        .execute(&mut *tx)
        .await
    {
        return Err(db_conflict(&error).unwrap_or_else(|| error.into()));
    }
    sqlx::query(
        "UPDATE poams SET status='in_progress',closed_at=NULL,closure_attempt_id=NULL WHERE id=$1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    bump_and_audit(
        &mut tx,
        actor,
        id,
        "reopened",
        json!({"previous_closure_attempt_id":attempt_id,"status":"in_progress"}),
    )
    .await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

pub async fn dashboard(
    pool: &PgPool,
    actor: &PoamActor,
    clock: &dyn PoamClock,
) -> Result<DashboardSummary, PoamError> {
    Ok(poam::dashboard(pool, clock.today(), actor.is_admin, &actor.environment_ids).await?)
}
pub async fn watchlist(
    pool: &PgPool,
    actor: &PoamActor,
    limit: i64,
    offset: i64,
    clock: &dyn PoamClock,
) -> Result<Page<PoamSummary>, PoamError> {
    let (limit, offset) = page_bounds(Some(limit), Some(offset))?;
    Ok(poam::watchlist(
        pool,
        clock.today(),
        actor.is_admin,
        &actor.environment_ids,
        limit,
        offset,
    )
    .await?)
}
pub async fn system_rollups(
    pool: &PgPool,
    actor: &PoamActor,
    ids: &[Uuid],
    clock: &dyn PoamClock,
) -> Result<Vec<Rollup>, PoamError> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(PoamError::Validation(
            "invalid_batch_size",
            "Batch requests require between 1 and 100 unique ids".into(),
        ));
    }
    let mut rollups = poam::system_rollups(
        pool,
        ids,
        clock.today(),
        actor.is_admin,
        &actor.environment_ids,
    )
    .await?;
    let visible_system_ids = rollups
        .iter()
        .map(|rollup| rollup.scope_id)
        .collect::<Vec<_>>();
    if visible_system_ids.is_empty() {
        return Ok(rollups);
    }
    let findings=sqlx::query_as::<_,(Uuid,Uuid,Uuid)>("SELECT id,system_id,policy_lineage_id FROM poam_findings WHERE system_id=ANY($1) ORDER BY system_id,policy_lineage_id LIMIT $2")
      .bind(&visible_system_ids).bind(MAX_RESOLVER_FINDINGS as i64 + 1).fetch_all(pool).await?;
    if findings.len() > MAX_RESOLVER_FINDINGS {
        return Err(PoamError::Validation(
            "rollup_scope_too_large",
            "The requested rollup expands to too many findings".into(),
        ));
    }
    let mut tx = begin_serializable(pool).await?;
    let items = current_verification_items_tx(&mut tx, &findings, clock.now()).await?;
    tx.commit().await?;
    let managed = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT link.finding_id FROM poam_finding_links link
           JOIN poams poam ON poam.id=link.poam_id
           WHERE link.retired_at IS NULL
             AND link.finding_id IN (SELECT id FROM poam_findings WHERE system_id=ANY($3))
             AND ($1 OR poam_visible_to_environments(poam.id,$2))"#,
    )
    .bind(actor.is_admin)
    .bind(&actor.environment_ids)
    .bind(&visible_system_ids)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect::<BTreeSet<_>>();
    for rollup in &mut rollups {
        let failures = items
            .iter()
            .filter(|item| {
                item.system_id == rollup.scope_id
                    && item.observed_outcome.as_deref() == Some("fail")
                    && !matches!(item.result.as_str(), "stale" | "missing")
            })
            .collect::<Vec<_>>();
        rollup.open_findings = failures.len() as i64;
        rollup.on_poam_findings = failures
            .iter()
            .filter(|item| managed.contains(&item.finding_id))
            .count() as i64;
        rollup.no_poam_findings = rollup.open_findings - rollup.on_poam_findings;
    }
    Ok(rollups)
}
pub async fn bundle_rollups(
    pool: &PgPool,
    actor: &PoamActor,
    ids: &[Uuid],
    clock: &dyn PoamClock,
) -> Result<Vec<Rollup>, PoamError> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(PoamError::Validation(
            "invalid_batch_size",
            "Batch requests require between 1 and 100 unique ids".into(),
        ));
    }
    let mut rollups = poam::bundle_rollups(
        pool,
        ids,
        clock.today(),
        actor.is_admin,
        &actor.environment_ids,
    )
    .await?;
    let findings=sqlx::query_as::<_,(Uuid,Uuid,Uuid)>(r#"SELECT DISTINCT finding.id,finding.system_id,finding.policy_lineage_id
      FROM poam_findings finding JOIN systems system ON system.id=finding.system_id
      JOIN compliance_bundle_assignments assignment ON assignment.active
        AND (assignment.system_id=system.id OR assignment.environment_id=system.environment_id)
      JOIN compliance_bundle_assignment_versions assignment_version ON assignment_version.id=assignment.current_version_id
      JOIN compliance_bundle_versions bundle_version ON bundle_version.id=assignment_version.bundle_version_id
      WHERE ($1 OR system.environment_id=ANY($2)) AND bundle_version.bundle_id=ANY($3)
        AND (EXISTS (SELECT 1 FROM compliance_assignment_additions addition
          JOIN deployment_policy_versions policy_version ON policy_version.id=addition.policy_version_id
          WHERE addition.assignment_version_id=assignment_version.id AND policy_version.policy_id=finding.policy_lineage_id)
        OR EXISTS (SELECT 1 FROM compliance_bundle_version_policies membership
          JOIN deployment_policy_versions policy_version ON policy_version.id=membership.policy_version_id
          WHERE membership.bundle_version_id=assignment_version.bundle_version_id AND membership.selected
            AND policy_version.policy_id=finding.policy_lineage_id
            AND NOT EXISTS (SELECT 1 FROM compliance_assignment_exclusions exclusion
              WHERE exclusion.assignment_version_id=assignment_version.id
                AND exclusion.policy_version_id=membership.policy_version_id)))
      ORDER BY finding.system_id,finding.policy_lineage_id LIMIT $4"#)
      .bind(actor.is_admin).bind(&actor.environment_ids).bind(ids).bind(MAX_RESOLVER_FINDINGS as i64 + 1).fetch_all(pool).await?;
    if findings.len() > MAX_RESOLVER_FINDINGS {
        return Err(PoamError::Validation(
            "rollup_scope_too_large",
            "The requested rollup expands to too many findings".into(),
        ));
    }
    let mut tx = begin_serializable(pool).await?;
    let items = current_verification_items_tx(&mut tx, &findings, clock.now()).await?;
    tx.commit().await?;
    let relevant_finding_ids = findings.iter().map(|row| row.0).collect::<Vec<_>>();
    let visible_poams = sqlx::query_as::<_, (Uuid, String, Option<NaiveDate>, Option<Uuid>)>(
        r#"SELECT poam.id,poam.status,poam.target_date,poam.closure_attempt_id FROM poams poam
           WHERE ($1 OR poam_visible_to_environments(poam.id,$2)) AND (
             EXISTS (SELECT 1 FROM poam_finding_links link WHERE link.poam_id=poam.id
               AND link.retired_at IS NULL AND link.finding_id=ANY($3))
             OR EXISTS (SELECT 1 FROM poam_verification_items item
               WHERE item.attempt_id=poam.closure_attempt_id AND item.bundle_ids&&$4)
             OR EXISTS (SELECT 1 FROM poam_assignment_references reference
               JOIN compliance_bundle_assignment_versions assignment_version ON assignment_version.id=reference.assignment_version_id
               JOIN compliance_bundle_versions bundle_version ON bundle_version.id=assignment_version.bundle_version_id
               WHERE reference.poam_id=poam.id AND bundle_version.bundle_id=ANY($4))
           )"#,
    )
    .bind(actor.is_admin)
    .bind(&actor.environment_ids)
    .bind(&relevant_finding_ids)
    .bind(ids)
    .fetch_all(pool)
    .await?;
    let visible_ids = visible_poams.iter().map(|row| row.0).collect::<Vec<_>>();
    let active_links = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT poam_id,finding_id FROM poam_finding_links WHERE poam_id=ANY($1) AND retired_at IS NULL",
    )
    .bind(&visible_ids)
    .fetch_all(pool)
    .await?;
    let closure_bundles = sqlx::query_as::<_, (Uuid, Vec<Uuid>)>(
        r#"SELECT poam.id,item.bundle_ids FROM poams poam
           JOIN poam_verification_items item ON item.attempt_id=poam.closure_attempt_id
           WHERE poam.id=ANY($1)"#,
    )
    .bind(&visible_ids)
    .fetch_all(pool)
    .await?;
    let assignment_bundles = sqlx::query_as::<_, (Uuid, Uuid)>(
        r#"SELECT reference.poam_id,bundle_version.bundle_id
           FROM poam_assignment_references reference
           JOIN compliance_bundle_assignment_versions assignment_version
             ON assignment_version.id=reference.assignment_version_id
           JOIN compliance_bundle_versions bundle_version
             ON bundle_version.id=assignment_version.bundle_version_id
           WHERE reference.poam_id=ANY($1)"#,
    )
    .bind(&visible_ids)
    .fetch_all(pool)
    .await?;
    let mut bundles_by_poam = HashMap::<Uuid, BTreeSet<Uuid>>::new();
    for (poam_id, finding_id) in active_links {
        if let Some(item) = items.iter().find(|item| item.finding_id == finding_id) {
            bundles_by_poam
                .entry(poam_id)
                .or_default()
                .extend(item.bundle_ids.iter().copied());
        }
    }
    for (poam_id, bundle_ids) in closure_bundles {
        bundles_by_poam
            .entry(poam_id)
            .or_default()
            .extend(bundle_ids);
    }
    for (poam_id, bundle_id) in assignment_bundles {
        bundles_by_poam
            .entry(poam_id)
            .or_default()
            .insert(bundle_id);
    }
    for rollup in &mut rollups {
        let matching = visible_poams.iter().filter(|poam| {
            bundles_by_poam
                .get(&poam.0)
                .is_some_and(|bundles| bundles.contains(&rollup.scope_id))
        });
        let matching = matching.collect::<Vec<_>>();
        rollup.total = matching.len() as i64;
        rollup.active = matching.iter().filter(|poam| poam.1 != "completed").count() as i64;
        rollup.overdue = matching
            .iter()
            .filter(|poam| poam.1 != "completed" && poam.2.is_some_and(|date| date < clock.today()))
            .count() as i64;
        rollup.awaiting_verification = matching
            .iter()
            .filter(|poam| poam.1 == "awaiting_verification")
            .count() as i64;
        rollup.completed = matching.iter().filter(|poam| poam.1 == "completed").count() as i64;
    }
    let managed = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT link.finding_id FROM poam_finding_links link
           JOIN poams poam ON poam.id=link.poam_id
           WHERE link.retired_at IS NULL
             AND link.finding_id=ANY($3)
             AND ($1 OR poam_visible_to_environments(poam.id,$2))"#,
    )
    .bind(actor.is_admin)
    .bind(&actor.environment_ids)
    .bind(&relevant_finding_ids)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect::<BTreeSet<_>>();
    for rollup in &mut rollups {
        let failures = items
            .iter()
            .filter(|item| {
                item.bundle_ids.contains(&rollup.scope_id)
                    && item.observed_outcome.as_deref() == Some("fail")
                    && !matches!(item.result.as_str(), "stale" | "missing")
            })
            .collect::<Vec<_>>();
        rollup.open_findings = failures.len() as i64;
        rollup.on_poam_findings = failures
            .iter()
            .filter(|item| managed.contains(&item.finding_id))
            .count() as i64;
        rollup.no_poam_findings = rollup.open_findings - rollup.on_poam_findings;
    }
    Ok(rollups)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct FixedClock(DateTime<Utc>);
    impl PoamClock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }
    #[test]
    fn defaults_and_overdue_boundaries_use_server_clock() {
        let clock = FixedClock(
            DateTime::parse_from_rfc3339("2026-08-26T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        assert_eq!(
            [14, 28, 35, 49, 56].map(|d| clock.today() + Duration::days(d)),
            [
                NaiveDate::from_ymd_opt(2026, 9, 9).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 23).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 14).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 21).unwrap()
            ]
        );
        assert!(!(clock.today() < clock.today()));
        assert!(clock.today() - Duration::days(1) < clock.today());
    }
    #[test]
    fn generic_transition_matrix_excludes_completion() {
        let statuses = [
            ("open", PoamStatus::Open),
            ("in_progress", PoamStatus::InProgress),
            ("blocked", PoamStatus::Blocked),
            ("awaiting_verification", PoamStatus::AwaitingVerification),
            ("completed", PoamStatus::Completed),
        ];
        let allowed = [
            ("open", "in_progress"),
            ("open", "blocked"),
            ("in_progress", "open"),
            ("in_progress", "blocked"),
            ("in_progress", "awaiting_verification"),
            ("blocked", "open"),
            ("blocked", "in_progress"),
            ("blocked", "awaiting_verification"),
            ("awaiting_verification", "in_progress"),
            ("awaiting_verification", "blocked"),
        ];
        for (from, _) in statuses {
            for (to_name, to) in statuses {
                assert_eq!(
                    transition_allowed(from, to),
                    allowed.contains(&(from, to_name)),
                    "unexpected transition result for {from} -> {to_name}"
                );
            }
        }
    }

    #[test]
    fn closure_outcome_mapping_is_explicitly_fail_closed() {
        for accepted in ["pass", "waiver"] {
            assert!(closure_result_is_accepted(accepted), "{accepted}");
        }
        for rejected in [
            "fail",
            "error",
            "not_checked",
            "missing",
            "stale",
            "unknown",
            "warn",
            "not_applicable",
            "future_outcome",
        ] {
            assert!(!closure_result_is_accepted(rejected), "{rejected}");
        }
    }
}
