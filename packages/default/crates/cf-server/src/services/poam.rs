//! Implements POA&M lifecycle, authorization, and evidence services.
//!
//! This module validates actor scope and lifecycle transitions before invoking
//! persistence queries. Mutations record activity and audit events in the same
//! transaction. Verification and closure bind results to exact current policy
//! evidence so stale observations cannot close a POA&M.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::models::{
    CveAffectedEnvironment, CveAffectedSystemDetail, CveDispositionActor,
    CveEnvironmentDisposition, CveEnvironmentTriageAction, CveTriageConflictSubject,
    FleetCveDetail, FleetCvePoamRequest, FleetCveTriageRequest, FleetCveTriageResponse,
    FleetCveTriageRollup, ScheduledPoamMetadata,
};
use crate::compliance::canonical::semantic_digest;
use crate::compliance::resolver::{
    EffectivePolicy, ResolutionOutcome, resolve_system_effective_policies_in_tx,
    resolve_systems_effective_policies_in_tx,
};
use crate::models::auth_identity::AuthRole;
use crate::models::poam::*;
use crate::queries::compliance::{
    CveObservationValues, cve_observation_reference, nix_policy_observation_reference,
    nix_policy_result,
};
use crate::queries::poam::{self, insert_activity_and_audit};
use crate::services::composite_enforcement::{
    PersistedAssessmentIdentity, PersistedRuleIdentity, enforce_composite_authorization_digest,
    policy_contexts, select_compatible_assessment_set,
};

/// Provides the current time used by POA&M lifecycle decisions.
pub trait PoamClock: Send + Sync {
    /// Returns the current UTC timestamp.
    fn now(&self) -> DateTime<Utc>;
    /// Returns the current UTC calendar date.
    fn today(&self) -> NaiveDate {
        self.now().date_naive()
    }
}

/// Provides production UTC time for POA&M operations.
pub struct SystemClock;
const MAX_POAM_RELATIONSHIPS: i64 = 100;
const LEGACY_RELATIONSHIP_HISTORY_LIMIT: i64 = 100;
const MAX_SHORT_TEXT_BYTES: usize = 256;
const MAX_SEARCH_BYTES: usize = 256;
const MAX_NOTE_BYTES: usize = 4_096;
const MAX_PLAN_BYTES: usize = 16_384;
const MAX_CANDIDATES_SCANNED: i64 = 1_000;
const MAX_CVE_RELATIONSHIP_ROWS: usize = 1_000;
const MAX_RESOLVER_FINDINGS: usize = 1_000;
const MAX_ROLLUP_POAMS: i64 = 1_000;
const MAX_ASSIGNEE_CATALOG_ITEMS: i64 = 1_000;
const MAX_FLEET_CVE_ENVIRONMENTS: usize = 100;
const MAX_FLEET_CVE_SUBJECTS: usize = 1_000;
const MAX_FLEET_CVE_CONFLICTS: usize = 100;
impl PoamClock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Describes the authenticated actor and authorization scope for an operation.
#[derive(Debug, Clone)]
pub struct PoamActor {
    /// Identifies the user recorded on mutations and audit events.
    pub user_id: Uuid,
    /// Contains the stable user identifier recorded in administrative audits.
    pub identifier: String,
    /// Indicates whether the actor can access every environment and admin API.
    pub is_admin: bool,
    /// Indicates whether the actor may mutate POA&M resources.
    pub can_mutate: bool,
    /// Lists environments visible to a non-admin actor.
    pub environment_ids: Vec<Uuid>,
    /// Identifies the request origin recorded with audit events, when available.
    pub request_origin: Option<String>,
}

/// Represents an expected POA&M service failure.
#[derive(Debug)]
pub enum PoamError {
    /// Indicates that the resource is absent or hidden by actor scope.
    NotFound,
    /// Indicates that the actor lacks permission for the operation.
    Forbidden,
    /// Contains a stable code and message for invalid request data.
    Validation(&'static str, String),
    /// Contains a stable code and message for a lifecycle or revision conflict.
    Conflict(&'static str, String),
    /// Contains a stable conflict code, message, and bounded conflict details.
    ConflictDetails(&'static str, String, Value),
    /// Contains a stable code, message, and optional evidence for a failed
    /// operation precondition.
    Precondition(&'static str, String, Option<Value>),
    /// Wraps an unexpected persistence or internal data error.
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
        "poam_finding_links_one_active_remediation"
        | "poam_cve_finding_links_one_active_remediation" => Some(PoamError::Conflict(
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

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct CveFindingKey {
    id: Uuid,
    system_id: Uuid,
    canonical_cve_id: String,
    canonical_package_name: String,
}

async fn poam_cve_finding_keys(pool: &PgPool, id: Uuid) -> Result<Vec<CveFindingKey>, PoamError> {
    Ok(sqlx::query_as(
        r#"SELECT finding.id,finding.system_id,finding.canonical_cve_id,
                  finding.canonical_package_name
           FROM poam_cve_finding_links link
           JOIN poam_cve_findings finding ON finding.id=link.cve_finding_id
           WHERE link.poam_id=$1 AND link.retired_at IS NULL
           ORDER BY finding.system_id,finding.canonical_cve_id,
                    finding.canonical_package_name,finding.id"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?)
}

async fn lock_cve_finding_keys_tx(
    tx: &mut Transaction<'_, Postgres>,
    findings: &[CveFindingKey],
) -> Result<(), PoamError> {
    if findings.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"SELECT lock_poam_cve_finding_key(key.system_id,key.cve_id,key.package_name)
           FROM (
             SELECT DISTINCT input.system_id,input.cve_id,input.package_name
             FROM UNNEST($1::uuid[],$2::text[],$3::text[])
               input(system_id,cve_id,package_name)
             ORDER BY input.system_id,input.cve_id,input.package_name
           ) key"#,
    )
    .bind(findings.iter().map(|row| row.system_id).collect::<Vec<_>>())
    .bind(
        findings
            .iter()
            .map(|row| row.canonical_cve_id.as_str())
            .collect::<Vec<_>>(),
    )
    .bind(
        findings
            .iter()
            .map(|row| row.canonical_package_name.as_str())
            .collect::<Vec<_>>(),
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn lock_cve_keys_tx(
    tx: &mut Transaction<'_, Postgres>,
    findings: &[CveFindingKey],
) -> Result<(), PoamError> {
    if findings.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"SELECT lock_poam_cve_key(key.cve_id)
           FROM (SELECT DISTINCT unnest($1::text[]) AS cve_id ORDER BY cve_id) key"#,
    )
    .bind(
        findings
            .iter()
            .map(|row| row.canonical_cve_id.as_str())
            .collect::<Vec<_>>(),
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn lock_policy_finding_keys_for_systems_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_ids: &[Uuid],
) -> Result<(), PoamError> {
    if system_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"SELECT lock_poam_finding_key(key.system_id,key.policy_lineage_id)
           FROM (
             SELECT finding.system_id,finding.policy_lineage_id
             FROM poam_findings finding
             WHERE finding.system_id=ANY($1)
             ORDER BY finding.system_id,finding.policy_lineage_id
           ) key"#,
    )
    .bind(system_ids)
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

async fn current_mutating_actor_tx(
    tx: &mut Transaction<'_, Postgres>,
    request_actor: &PoamActor,
) -> Result<PoamActor, PoamError> {
    // SECURITY: Lock the identity rows after domain writer locks. A revocation
    // that won the race becomes visible before authorization; a later
    // revocation waits and linearizes after this mutation.
    let user = sqlx::query_as::<_, (String, bool)>(
        "SELECT email,is_active FROM users WHERE id=$1 FOR SHARE",
    )
    .bind(request_actor.user_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PoamError::Forbidden)?;
    if !user.1 {
        return Err(PoamError::Forbidden);
    }
    let roles = sqlx::query_scalar::<_, AuthRole>(
        "SELECT role FROM user_role_assignments WHERE user_id=$1 ORDER BY role FOR SHARE",
    )
    .bind(request_actor.user_id)
    .fetch_all(&mut **tx)
    .await?;
    let is_admin = roles.contains(&AuthRole::Admin);
    let can_mutate = is_admin || roles.contains(&AuthRole::Operator);
    if !can_mutate {
        return Err(PoamError::Forbidden);
    }
    let environment_ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT environment_id FROM user_environment_memberships WHERE user_id=$1 ORDER BY environment_id FOR SHARE",
    )
    .bind(request_actor.user_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(PoamActor {
        user_id: request_actor.user_id,
        identifier: user.0,
        is_admin,
        can_mutate,
        environment_ids,
        request_origin: request_actor.request_origin.clone(),
    })
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

fn relationship_page_bounds(
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Option<(i64, i64)>, PoamError> {
    match (limit, offset) {
        // COMPATIBILITY: Deployed clients can omit pagination parameters. Keep
        // that request valid, but use the response's existing continuation
        // metadata so legacy requests cannot expand without a resource bound.
        (None, None) => Ok(Some((LEGACY_RELATIONSHIP_HISTORY_LIMIT, 0))),
        (None, Some(_)) => Err(PoamError::Validation(
            "invalid_relationship_pagination",
            "history_offset requires history_limit".into(),
        )),
        (Some(limit), offset) => page_bounds(Some(limit), offset).map(Some),
    }
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

#[derive(Debug)]
struct ResolvedAssignee {
    owner: String,
    kind: Option<&'static str>,
    user_id: Option<Uuid>,
    group_name: Option<String>,
}

fn normalize_oidc_group_name(value: &str) -> Result<String, PoamError> {
    // COMPATIBILITY: This is the same normalization and accepted character set
    // as the administrative OIDC group-mapping API.
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(PoamError::Validation(
            "invalid_assignee_group",
            "Group name is required".into(),
        ));
    }
    if normalized.len() > 128 {
        return Err(PoamError::Validation(
            "invalid_assignee_group",
            "Group name must be 128 characters or fewer".into(),
        ));
    }
    if !normalized
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/'))
    {
        return Err(PoamError::Validation(
            "invalid_assignee_group",
            "Group name may only contain letters, numbers, '-', '_', '.', ':', '/'".into(),
        ));
    }
    Ok(normalized)
}

async fn resolve_assignee_tx(
    tx: &mut Transaction<'_, Postgres>,
    request: &PoamAssigneeRequest,
) -> Result<ResolvedAssignee, PoamError> {
    match request {
        PoamAssigneeRequest::User { user_id } => {
            let user = sqlx::query_as::<_, (Option<String>, Option<String>, String, String)>(
                r#"SELECT first_name,last_name,username,email FROM users
                   WHERE id=$1 AND is_active AND user_type='human'"#,
            )
            .bind(user_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| {
                PoamError::Validation(
                    "invalid_assignee_user",
                    "Assignee user must exist and be an active human user".into(),
                )
            })?;
            let full_name = format!(
                "{} {}",
                user.0.as_deref().unwrap_or_default().trim(),
                user.1.as_deref().unwrap_or_default().trim()
            )
            .trim()
            .to_owned();
            let owner = if !full_name.is_empty() {
                full_name
            } else if !user.2.trim().is_empty() {
                user.2.trim().to_owned()
            } else {
                user.3.trim().to_owned()
            };
            Ok(ResolvedAssignee {
                owner,
                kind: Some("user"),
                user_id: Some(*user_id),
                group_name: None,
            })
        }
        PoamAssigneeRequest::OidcGroup { group_name } => {
            let group_name = normalize_oidc_group_name(group_name)?;
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM oidc_group_mappings WHERE group_name=$1)",
            )
            .bind(&group_name)
            .fetch_one(&mut **tx)
            .await?;
            if !exists {
                return Err(PoamError::Validation(
                    "invalid_assignee_group",
                    "Assignee group must have a current OIDC group mapping".into(),
                ));
            }
            Ok(ResolvedAssignee {
                owner: group_name.clone(),
                kind: Some("oidc_group"),
                user_id: None,
                group_name: Some(group_name),
            })
        }
        PoamAssigneeRequest::Unassigned => Ok(ResolvedAssignee {
            owner: String::new(),
            kind: None,
            user_id: None,
            group_name: None,
        }),
    }
}

/// Returns the bounded assignee catalog available to POA&M mutators.
///
/// # Errors
///
/// Returns [`PoamError::Forbidden`] when the actor cannot mutate POA&Ms. It
/// returns a database error when the catalog cannot be loaded.
pub async fn assignee_catalog(
    pool: &PgPool,
    actor: &PoamActor,
) -> Result<PoamAssigneeCatalog, PoamError> {
    require_mutator(actor)?;
    Ok(poam::assignee_catalog(pool, MAX_ASSIGNEE_CATALOG_ITEMS).await?)
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
    derivation_id: i32,
    overall_outcome: String,
    target_store_path: String,
    effective_config_digest: String,
    effective_config: Value,
}

#[derive(Debug)]
struct FindingActionContext {
    assessment_id: Option<Uuid>,
    finding_id: Uuid,
    system_id: Uuid,
    policy_lineage_id: Uuid,
    policy_version_id: Uuid,
    overall_outcome: String,
}

async fn finding_action_key_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    assessment_id: Option<Uuid>,
    finding_id: Option<Uuid>,
    observation: Option<&FindingObservationReference>,
) -> Result<(Uuid, Uuid), PoamError> {
    let key = match (assessment_id, finding_id, observation) {
        (Some(assessment_id), None, None) => assessment_finding_key_tx(tx, assessment_id)
            .await?
            .ok_or(PoamError::NotFound)?,
        (None, Some(finding_id), Some(_)) => {
            sqlx::query_as("SELECT system_id,policy_lineage_id FROM poam_findings WHERE id=$1")
                .bind(finding_id)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(PoamError::NotFound)?
        }
        _ => {
            return Err(PoamError::Validation(
                "invalid_finding_observation",
                "Provide either assessment_id or both finding_id and observation".into(),
            ));
        }
    };
    // SECURITY: Only stable finding metadata is read before authorization. The
    // caller cannot distinguish stale, invalid, or missing observation data for
    // a system outside the caller's environment scope.
    if !actor_can_access_systems_tx(tx, actor, &[key.0]).await? {
        return Err(PoamError::NotFound);
    }
    Ok(key)
}

async fn finding_action_context_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    assessment_id: Option<Uuid>,
    finding_id: Option<Uuid>,
    observation: Option<&FindingObservationReference>,
) -> Result<FindingActionContext, PoamError> {
    match (assessment_id, finding_id, observation) {
        (Some(assessment_id), None, None) => {
            let context = assessment_context_tx(tx, assessment_id)
                .await?
                .ok_or(PoamError::NotFound)?;
            if !actor_can_access_systems_tx(tx, actor, &[context.system_id]).await? {
                return Err(PoamError::NotFound);
            }
            validate_current_assessment_tx(tx, &context).await?;
            Ok(FindingActionContext {
                assessment_id: Some(assessment_id),
                finding_id: context.finding_id,
                system_id: context.system_id,
                policy_lineage_id: context.policy_lineage_id,
                policy_version_id: context.policy_version_id,
                overall_outcome: context.overall_outcome,
            })
        }
        (None, Some(finding_id), Some(observation)) => {
            legacy_finding_action_context_tx(tx, actor, finding_id, observation).await
        }
        _ => Err(PoamError::Validation(
            "invalid_finding_observation",
            "Provide either assessment_id or both finding_id and observation".into(),
        )),
    }
}

async fn legacy_finding_action_context_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    finding_id: Uuid,
    requested: &FindingObservationReference,
) -> Result<FindingActionContext, PoamError> {
    let (system_id, policy_lineage_id): (Uuid, Uuid) = sqlx::query_as(
        "SELECT system_id,policy_lineage_id FROM poam_findings WHERE id=$1 FOR SHARE",
    )
    .bind(finding_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PoamError::NotFound)?;
    // SECURITY: Resolve policy and observation details only after the caller's
    // environment scope is known to include the finding's system.
    if !actor_can_access_systems_tx(tx, actor, &[system_id]).await? {
        return Err(PoamError::NotFound);
    }
    let ResolutionOutcome::Resolved(resolved) =
        resolve_system_effective_policies_in_tx(tx, system_id).await?
    else {
        return Err(PoamError::Precondition(
            "policy_conflict",
            "Current effective policy set has conflicts".into(),
            None,
        ));
    };
    let policy = resolved
        .policies
        .iter()
        .find(|policy| policy.policy_lineage_id == policy_lineage_id)
        .ok_or_else(|| {
            PoamError::Precondition(
                "stale_finding",
                "Finding policy is no longer effective for the system".into(),
                None,
            )
        })?;
    let deployed: Option<(i32, String, Value)> = sqlx::query_as(
        r#"SELECT derivation.id,deployed.store_path,derivation.policy_results
           FROM systems system
           JOIN LATERAL (
             SELECT state.store_path FROM system_states state
             WHERE state.hostname=system.hostname AND state.store_path IS NOT NULL
               AND btrim(state.store_path)<>''
             ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
           ) deployed ON true
           JOIN derivations derivation
             ON COALESCE(derivation.store_path,derivation.expected_store_path)=deployed.store_path
            AND derivation.derivation_type='nixos'
           WHERE system.id=$1
           ORDER BY derivation.completed_at DESC NULLS LAST,derivation.id DESC LIMIT 1"#,
    )
    .bind(system_id)
    .fetch_optional(&mut **tx)
    .await?;
    let (derivation_id, target_store_path, policy_results) = deployed.ok_or_else(|| {
        PoamError::Precondition(
            "stale_finding",
            "No current deployed policy observation exists".into(),
            None,
        )
    })?;
    let current = match requested.source {
        FindingObservationSource::NixPolicyResult => {
            if !matches!(
                policy.policy_type.as_str(),
                "require_packages" | "custom_check" | "require_cf_agent"
            ) {
                return Err(PoamError::Validation(
                    "invalid_finding_observation",
                    "Observation source does not match the policy type".into(),
                ));
            }
            let (passed, details) = nix_policy_result(
                &policy_results,
                Some(policy.policy_version_id),
                policy_lineage_id,
            )
            .map_err(|error| {
                PoamError::Precondition("invalid_finding_observation", error.to_string(), None)
            })?
            .ok_or_else(|| {
                PoamError::Precondition(
                    "stale_finding",
                    "The deployed evaluation has no current policy result".into(),
                    None,
                )
            })?;
            (
                nix_policy_observation_reference(
                    system_id,
                    policy_lineage_id,
                    policy.policy_version_id,
                    &resolved.effective_set_digest,
                    &semantic_digest(&policy.effective_config),
                    derivation_id,
                    &target_store_path,
                    passed,
                    details.as_deref(),
                ),
                if passed { "pass" } else { "fail" },
            )
        }
        FindingObservationSource::CveScan => {
            if policy.policy_type != "require_cve_check" {
                return Err(PoamError::Validation(
                    "invalid_finding_observation",
                    "Observation source does not match the policy type".into(),
                ));
            }
            let (scan_id, critical_count, high_count): (Uuid, i32, i32) = sqlx::query_as(
                r#"SELECT id,critical_count,high_count FROM cve_scans
                   WHERE derivation_id=$1 AND status='completed'
                   ORDER BY completed_at DESC NULLS LAST,id DESC LIMIT 1"#,
            )
            .bind(derivation_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| {
                PoamError::Precondition(
                    "stale_finding",
                    "The deployed derivation has no completed CVE scan".into(),
                    None,
                )
            })?;
            let max_critical = policy
                .effective_config
                .get("max_critical")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX);
            let max_high = policy
                .effective_config
                .get("max_high")
                .and_then(Value::as_i64);
            let passed = i64::from(critical_count) <= max_critical
                && max_high.is_none_or(|max| i64::from(high_count) <= max);
            (
                cve_observation_reference(CveObservationValues {
                    system_id,
                    policy_lineage_id,
                    policy_version_id: policy.policy_version_id,
                    effective_set_digest: &resolved.effective_set_digest,
                    effective_config_digest: &semantic_digest(&policy.effective_config),
                    derivation_id,
                    target_store_path: &target_store_path,
                    scan_id,
                    critical_count,
                    high_count,
                    max_critical,
                    max_high,
                }),
                if passed { "pass" } else { "fail" },
            )
        }
    };
    if current.0 != *requested {
        return Err(PoamError::Precondition(
            "stale_finding",
            "Observation was superseded by newer authoritative evidence".into(),
            None,
        ));
    }
    Ok(FindingActionContext {
        assessment_id: None,
        finding_id,
        system_id,
        policy_lineage_id,
        policy_version_id: policy.policy_version_id,
        overall_outcome: current.1.to_string(),
    })
}

async fn assessment_context_tx(
    tx: &mut Transaction<'_, Postgres>,
    assessment_id: Uuid,
) -> Result<Option<AssessmentContext>, PoamError> {
    Ok(sqlx::query_as::<_, AssessmentContext>(
        r#"
        SELECT a.id AS assessment_id, f.id AS finding_id, a.system_id,
               a.policy_lineage_id, a.policy_version_id, a.derivation_id,
               a.overall_outcome,
               a.target_store_path,
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
        || semantic_digest(&policy.effective_config) != context.effective_config_digest
        || policy.effective_config != context.effective_config
    {
        return Err(PoamError::Precondition(
            "stale_finding",
            "Finding does not match the current effective policy context".into(),
            None,
        ));
    }
    let policies = policy_contexts(&resolved)?;
    let assessments = sqlx::query_as::<_, PersistedAssessmentIdentity>(
        r#"SELECT id,policy_lineage_id,policy_version_id,effective_set_digest,
                  effective_config_digest,effective_config
           FROM composite_policy_assessments
           WHERE system_id=$1 AND derivation_id=$2 AND target_store_path=$3
           ORDER BY updated_at DESC,id DESC FOR SHARE"#,
    )
    .bind(context.system_id)
    .bind(context.derivation_id)
    .bind(&context.target_store_path)
    .fetch_all(&mut **tx)
    .await?;
    let assessment_ids = assessments
        .iter()
        .map(|assessment| assessment.id)
        .collect::<Vec<_>>();
    let rules = sqlx::query_as::<_, PersistedRuleIdentity>(
        r#"SELECT assessment_id,rule_id,ordinal,kind,phase,outcome,blocking
           FROM composite_policy_rule_results
           WHERE assessment_id=ANY($1)
           ORDER BY assessment_id,ordinal FOR SHARE"#,
    )
    .bind(&assessment_ids)
    .fetch_all(&mut **tx)
    .await?;
    let compatible = select_compatible_assessment_set(
        &policies,
        &enforce_composite_authorization_digest(&resolved),
        &assessments,
        &rules,
    );
    if !compatible.is_some_and(|set| set.ids().contains(&context.assessment_id)) {
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
    crate::services::composite_enforcement::lock_poam_system_key_tx(tx, key.0).await?;
    // CONCURRENCY: A narrow policy action still acquires the complete policy
    // level for its system before it locks evidence or POA&M rows.
    lock_policy_finding_keys_for_systems_tx(tx, &[key.0]).await?;
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

async fn validate_cve_assignment_compatibility_tx(
    tx: &mut Transaction<'_, Postgres>,
    assignment_version_ids: &[Uuid],
    system_ids: &[Uuid],
) -> Result<(), PoamError> {
    if assignment_version_ids.is_empty() {
        return Ok(());
    }
    let compatible_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(DISTINCT version.id)
           FROM compliance_bundle_assignment_versions version
           JOIN compliance_bundle_assignments assignment
             ON assignment.id=version.assignment_id
           WHERE version.id=ANY($1)
             AND EXISTS (
               SELECT 1 FROM systems system
               WHERE system.id=ANY($2)
                 AND (assignment.system_id=system.id
                   OR assignment.environment_id=system.environment_id)
             )"#,
    )
    .bind(assignment_version_ids)
    .bind(system_ids)
    .fetch_one(&mut **tx)
    .await?;
    if compatible_count != assignment_version_ids.len() as i64 {
        return Err(PoamError::Validation(
            "incompatible_assignment_reference",
            "Assignment references must overlap an exact-CVE finding system".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct CveLinkBaseline {
    system_id: Uuid,
    scan_id: Uuid,
    scan_derivation_id: i32,
    scan_completed_at: DateTime<Utc>,
    generation_snapshot_id: Uuid,
    generation: i32,
    target_store_path: String,
    occurrence_derivation_path: String,
    observed_package_version: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CurrentCveOccurrence {
    system_id: Uuid,
    scan_id: Uuid,
    scan_derivation_id: i32,
    scan_completed_at: DateTime<Utc>,
    generation_snapshot_id: Uuid,
    generation: i32,
    target_store_path: String,
    canonical_cve_id: String,
    canonical_package_name: String,
    observed_package_name: String,
    observed_package_version: String,
    occurrence_derivation_path: String,
    is_whitelisted: bool,
    is_justified: bool,
}

impl CurrentCveOccurrence {
    fn reference(&self) -> CveObservationReference {
        CveObservationReference {
            system_id: self.system_id,
            scan_id: self.scan_id,
            occurrence_derivation_path: self.occurrence_derivation_path.clone(),
            canonical_cve_id: self.canonical_cve_id.clone(),
            canonical_package_name: self.canonical_package_name.clone(),
        }
    }

    fn baseline(&self) -> CveLinkBaseline {
        CveLinkBaseline {
            system_id: self.system_id,
            scan_id: self.scan_id,
            scan_derivation_id: self.scan_derivation_id,
            scan_completed_at: self.scan_completed_at,
            generation_snapshot_id: self.generation_snapshot_id,
            generation: self.generation,
            target_store_path: self.target_store_path.clone(),
            occurrence_derivation_path: self.occurrence_derivation_path.clone(),
            observed_package_version: self.observed_package_version.clone(),
        }
    }
}

async fn current_cve_occurrence_tx(
    tx: &mut Transaction<'_, Postgres>,
    reference: &CveObservationReference,
) -> Result<CurrentCveOccurrence, PoamError> {
    let occurrence = sqlx::query_as::<_, CurrentCveOccurrence>(
        r#"SELECT system.id AS system_id,scan.id AS scan_id,
                   derivation.id AS scan_derivation_id,
                   scan.completed_at AS scan_completed_at,
                   retained.id AS generation_snapshot_id,
                   retained.generation,deployed.store_path AS target_store_path,
                   observation.canonical_cve_id,
                  observation.canonical_package_name,
                  observation.observed_package_name,
                  observation.observed_package_version,
                  observation.observed_derivation_path AS occurrence_derivation_path,
                  observation.is_whitelisted,
                  EXISTS(SELECT 1 FROM system_cve_justifications justification
                    WHERE justification.cve_id=observation.canonical_cve_id
                      AND (justification.system_id IS NULL
                        OR justification.system_id=system.id)) AS is_justified
           FROM systems system
           JOIN LATERAL (
             SELECT state.store_path,state.generation FROM system_states state
             WHERE state.hostname=system.hostname AND state.store_path IS NOT NULL
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
            AND artifact.lifecycle='available' AND artifact.integrity_version=1
           JOIN derivations derivation
             ON derivation.id=retained.derivation_id
            AND derivation.commit_id=retained.commit_id
            AND derivation.derivation_name=retained.configuration_name
            AND derivation.derivation_type='nixos'
            AND COALESCE(derivation.store_path,derivation.expected_store_path)=retained.source_store_path
           JOIN LATERAL (
             SELECT candidate.id,candidate.completed_at
             FROM cve_scans candidate
             WHERE candidate.derivation_id=derivation.id
               AND candidate.status='completed'
               AND candidate.evidence_schema_version=1
             ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
           ) scan ON true
           JOIN cve_scan_vulnerability_observations observation ON observation.scan_id=scan.id
           WHERE system.id=$1
              AND observation.observed_derivation_path=$2
              AND observation.canonical_cve_id=$3
              AND observation.canonical_package_name=$4"#,
    )
    .bind(reference.system_id)
    .bind(&reference.occurrence_derivation_path)
    .bind(&reference.canonical_cve_id)
    .bind(&reference.canonical_package_name)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| {
        PoamError::Precondition(
            "stale_cve_observation",
            "Exact CVE observation is not present in current deployed evidence".into(),
            None,
        )
    })?;
    if occurrence.scan_id != reference.scan_id || occurrence.reference() != *reference {
        return Err(PoamError::Precondition(
            "stale_cve_observation",
            "Exact CVE observation was superseded by newer deployed evidence".into(),
            None,
        ));
    }
    Ok(occurrence)
}

async fn validate_cve_create_context_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    reference: &CveObservationReference,
) -> Result<CurrentCveOccurrence, PoamError> {
    // CONCURRENCY: The single-system convenience path uses the same complete
    // lock levels as fleet triage: CVE, all system sentinels, all policy keys,
    // then all exact keys. It does not interleave keys for this one subject.
    lock_fleet_cve_scope_tx(
        tx,
        &reference.canonical_cve_id,
        &[reference.system_id],
        &reference.canonical_package_name,
    )
    .await?;
    // SECURITY: The system row lock serializes environment moves. An update
    // that wins commits before this read; an update that loses cannot change
    // authorization until this mutation commits.
    let environment_id: Option<Uuid> =
        sqlx::query_scalar("SELECT environment_id FROM systems WHERE id=$1 FOR UPDATE")
            .bind(reference.system_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(PoamError::NotFound)?;
    let is_active: Option<bool> =
        sqlx::query_scalar("SELECT is_active FROM users WHERE id=$1 FOR SHARE")
            .bind(actor.user_id)
            .fetch_optional(&mut **tx)
            .await?;
    if is_active != Some(true) {
        return Err(PoamError::Forbidden);
    }
    let roles = sqlx::query_scalar::<_, AuthRole>(
        "SELECT role FROM user_role_assignments WHERE user_id=$1 ORDER BY role FOR SHARE",
    )
    .bind(actor.user_id)
    .fetch_all(&mut **tx)
    .await?;
    let is_admin = roles.contains(&AuthRole::Admin);
    if !is_admin && !roles.contains(&AuthRole::Operator) {
        return Err(PoamError::Forbidden);
    }
    let environment_ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT environment_id FROM user_environment_memberships WHERE user_id=$1 ORDER BY environment_id FOR SHARE",
    )
    .bind(actor.user_id)
    .fetch_all(&mut **tx)
    .await?;
    if !is_admin && !environment_id.is_some_and(|id| environment_ids.contains(&id)) {
        return Err(PoamError::NotFound);
    }
    let occurrence = current_cve_occurrence_tx(tx, reference).await?;
    if occurrence.is_whitelisted {
        return Err(PoamError::Precondition(
            "cve_occurrence_whitelisted",
            "Scanner-whitelisted CVE evidence cannot start remediation".into(),
            None,
        ));
    }
    if occurrence.is_justified {
        return Err(PoamError::Precondition(
            "cve_occurrence_justified",
            "A justified CVE remains independent from POA&M remediation".into(),
            None,
        ));
    }
    Ok(occurrence)
}

/// Creates a POA&M from one current failing finding.
///
/// The operation validates assignment references, creates optional default
/// milestones, and records creation activity atomically.
///
/// # Errors
///
/// Returns an authorization, validation, conflict, precondition, not-found, or
/// database error when the corresponding creation requirement is not met.
pub async fn create(
    pool: &PgPool,
    actor: &PoamActor,
    request: CreatePoamRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    if request.assignee.is_some() && !request.owner.trim().is_empty() {
        return Err(PoamError::Validation(
            "ambiguous_assignee",
            "Provide either a non-empty owner or a typed assignee, not both".into(),
        ));
    }
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
    let resolved_assignee = match request.assignee.as_ref() {
        Some(assignee) => Some(resolve_assignee_tx(&mut tx, assignee).await?),
        None => None,
    };
    // CONCURRENCY: Assessment, derivation-result, and CVE-scan writers acquire
    // this stable key before commit. Acquire it before resolving evidence so a
    // writer that wins the lock is visible to the following READ COMMITTED
    // statements, and a writer that loses the lock commits after this action.
    let key = finding_action_key_tx(
        &mut tx,
        actor,
        request.assessment_id,
        request.finding_id,
        request.observation.as_ref(),
    )
    .await?;
    lock_assessment_finding_key_tx(&mut tx, key).await?;
    let context = finding_action_context_tx(
        &mut tx,
        actor,
        request.assessment_id,
        request.finding_id,
        request.observation.as_ref(),
    )
    .await?;
    if !actor_can_access_systems_tx(&mut tx, actor, &[context.system_id]).await? {
        return Err(PoamError::NotFound);
    }
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
        INSERT INTO poams(title,plan,owner,owner_kind,owner_user_id,owner_group_name,target_date,risk,created_by)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id"#,
    )
    .bind(title)
    .bind(request.plan.trim())
    .bind(
        resolved_assignee
            .as_ref()
            .map_or_else(|| request.owner.trim(), |assignee| assignee.owner.as_str()),
    )
    .bind(resolved_assignee.as_ref().and_then(|assignee| assignee.kind))
    .bind(resolved_assignee.as_ref().and_then(|assignee| assignee.user_id))
    .bind(
        resolved_assignee
            .as_ref()
            .and_then(|assignee| assignee.group_name.as_deref()),
    )
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
        'owner',owner,'assignee',poam_assignee_view(poams),'target_date',target_date,'risk',risk,'status',status,'revision',revision,
        'created_by',created_by,'created_at',created_at),
       'finding',jsonb_build_object('finding_id',$2::uuid,'assessment_id',$3::uuid,'observation',$4::jsonb),
      'assignments',COALESCE((SELECT jsonb_agg(jsonb_build_object('assignment_id',assignment_id,
        'assignment_version_id',assignment_version_id,'added_by',added_by,'added_at',added_at)
        ORDER BY assignment_version_id) FROM poam_assignment_references WHERE poam_id=$1),'[]'::jsonb),
      'milestones',COALESCE((SELECT jsonb_agg(to_jsonb(milestone) ORDER BY ordinal)
        FROM poam_milestones milestone WHERE poam_id=$1),'[]'::jsonb)) FROM poams WHERE id=$1"#)
      .bind(poam_id).bind(context.finding_id).bind(context.assessment_id)
      .bind(serde_json::to_value(&request.observation).unwrap_or(Value::Null))
      .fetch_one(&mut *tx).await?;
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

/// Creates a POA&M from one current unwhitelisted and unjustified CVE occurrence.
///
/// Stable finding identity excludes package version. The operation re-resolves
/// the server-issued observation and enforces one active remediation while the
/// exact finding advisory key is held.
///
/// # Errors
///
/// Returns an authorization, validation, conflict, precondition, not-found, or
/// database error when creation requirements are not met.
pub async fn create_cve(
    pool: &PgPool,
    actor: &PoamActor,
    request: CreateCvePoamRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    if request.assignee.is_some() && !request.owner.trim().is_empty() {
        return Err(PoamError::Validation(
            "ambiguous_assignee",
            "Provide either a non-empty owner or a typed assignee, not both".into(),
        ));
    }
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
    if request.assignment_version_ids.len() > MAX_POAM_RELATIONSHIPS as usize {
        return Err(PoamError::Validation(
            "too_many_assignment_references",
            "At most 100 assignment references are allowed".into(),
        ));
    }
    let mut assignment_version_ids = request.assignment_version_ids.clone();
    assignment_version_ids.sort_unstable();
    assignment_version_ids.dedup();
    let mut tx = pool.begin().await?;
    let occurrence = validate_cve_create_context_tx(&mut tx, actor, &request.observation).await?;
    let resolved_assignee = match request.assignee.as_ref() {
        Some(assignee) => Some(resolve_assignee_tx(&mut tx, assignee).await?),
        None => None,
    };
    validate_assignment_refs_tx(&mut tx, actor, &assignment_version_ids).await?;
    validate_cve_assignment_compatibility_tx(
        &mut tx,
        &assignment_version_ids,
        &[occurrence.system_id],
    )
    .await?;
    let poam_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO poams(title,plan,owner,owner_kind,owner_user_id,
              owner_group_name,target_date,risk,created_by)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id"#,
    )
    .bind(title)
    .bind(request.plan.trim())
    .bind(
        resolved_assignee
            .as_ref()
            .map_or_else(|| request.owner.trim(), |assignee| assignee.owner.as_str()),
    )
    .bind(
        resolved_assignee
            .as_ref()
            .and_then(|assignee| assignee.kind),
    )
    .bind(
        resolved_assignee
            .as_ref()
            .and_then(|assignee| assignee.user_id),
    )
    .bind(
        resolved_assignee
            .as_ref()
            .and_then(|assignee| assignee.group_name.as_deref()),
    )
    .bind(request.target_date.or_else(|| {
        request
            .default_milestones
            .then(|| clock.today() + Duration::days(56))
    }))
    .bind(request.risk.as_str())
    .bind(actor.user_id)
    .fetch_one(&mut *tx)
    .await?;
    let cve_finding_id = materialize_exact_subjects_tx(
        &mut tx,
        poam_id,
        actor.user_id,
        &[occurrence.baseline()],
        &occurrence.canonical_cve_id,
        &occurrence.canonical_package_name,
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| PoamError::Database(anyhow::anyhow!("exact subject was not materialized")))?;
    for assignment_version_id in &assignment_version_ids {
        sqlx::query("INSERT INTO poam_assignment_references(poam_id,assignment_id,assignment_version_id,added_by) SELECT $1,assignment_id,id,$3 FROM compliance_bundle_assignment_versions WHERE id=$2")
            .bind(poam_id).bind(assignment_version_id).bind(actor.user_id)
            .execute(&mut *tx).await?;
    }
    if request.default_milestones {
        for (ordinal, (offset, milestone_title)) in [14_i64, 28, 35, 49, 56]
            .into_iter()
            .zip([
                "Update NixOS module",
                "Deploy to staging",
                "Validate new configuration",
                "Deploy to production",
                "Verify CVE remediation passes",
            ])
            .enumerate()
        {
            sqlx::query("INSERT INTO poam_milestones(poam_id,ordinal,title,target_date,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$5)")
                .bind(poam_id).bind(ordinal as i32).bind(milestone_title)
                .bind(clock.today() + Duration::days(offset)).bind(actor.user_id)
                .execute(&mut *tx).await?;
        }
    }
    insert_activity_and_audit(
        &mut tx,
        poam_id,
        actor.user_id,
        &actor.identifier,
        "created",
        &json!({
            "poam_id": poam_id,
            "revision": 1,
            "finding": {
                "cve_finding_id": cve_finding_id,
                "observation": request.observation,
                "system_id": occurrence.system_id,
                "canonical_cve_id": occurrence.canonical_cve_id,
                "canonical_package_name": occurrence.canonical_package_name
            }
        }),
        actor.request_origin.as_deref(),
    )
    .await?;
    let detail = cve_poam_detail_tx(&mut tx, actor, poam_id, clock).await?;
    tx.commit().await?;
    Ok(detail)
}

/// Lists POA&Ms visible to an actor using the requested filters and page.
///
/// Filters that require current policy context are resolved against
/// authoritative evidence and have bounded candidate expansion.
///
/// # Errors
///
/// Returns a validation error for invalid or overly broad filters and a
/// database error when visible summaries or policy context cannot be loaded.
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

/// Returns a POA&M with default bounded history pages.
///
/// # Errors
///
/// Returns [`PoamError::NotFound`] when the POA&M is absent or outside the
/// actor's scope. It returns a validation or database error on load failure.
pub async fn detail(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    detail_with_history(pool, actor, id, &PoamDetailQuery::default(), clock).await
}

async fn cve_poam_detail_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    id: Uuid,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    let mut detail = poam::detail(
        tx,
        id,
        clock.today(),
        actor.is_admin,
        &actor.environment_ids,
        100,
        None,
        None,
        100,
        None,
        None,
        10,
        None,
        None,
    )
    .await?
    .ok_or(PoamError::NotFound)?;
    let keys = detail
        .cve_findings
        .iter()
        .filter(|finding| finding.link_active)
        .map(|finding| CveFindingKey {
            id: finding.id,
            system_id: finding.system_id,
            canonical_cve_id: finding.canonical_cve_id.clone(),
            canonical_package_name: finding.canonical_package_name.clone(),
        })
        .collect::<Vec<_>>();
    let items = current_cve_verification_items_tx(tx, &keys, None).await?;
    for finding in &mut detail.cve_findings {
        let Some(item) = items.iter().find(|item| item.cve_finding_id == finding.id) else {
            continue;
        };
        finding.current_derivation_id = item.scan_derivation_id;
        finding.current_target_store_path = item.target_store_path.clone();
        finding.current_scan_id = item.scan_id;
        finding.current_occurrence_derivation_path = item.occurrence_derivation_path.clone();
        finding.current_observed_package_version = item.observed_package_version.clone();
        finding.resolution_state = item.result.clone();
    }
    Ok(detail)
}

/// Returns a POA&M with caller-selected cursor-based history pages.
///
/// Current findings are refreshed from authoritative assessment evidence.
/// Historical verification items retain their captured system and policy
/// identity, and requirement metadata is hydrated for display.
///
/// # Errors
///
/// Returns a not-found error for an inaccessible POA&M, a validation error for
/// invalid cursor or limit combinations, or a database error on query failure.
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
        let cve_keys = detail
            .cve_findings
            .iter()
            .filter(|finding| finding.link_active)
            .map(|finding| CveFindingKey {
                id: finding.id,
                system_id: finding.system_id,
                canonical_cve_id: finding.canonical_cve_id.clone(),
                canonical_package_name: finding.canonical_package_name.clone(),
            })
            .collect::<Vec<_>>();
        let cve_items = current_cve_verification_items_tx(&mut tx, &cve_keys, None).await?;
        for finding in &mut detail.cve_findings {
            let Some(item) = cve_items
                .iter()
                .find(|item| item.cve_finding_id == finding.id)
            else {
                continue;
            };
            finding.current_derivation_id = item.scan_derivation_id;
            finding.current_target_store_path = item.target_store_path.clone();
            finding.current_scan_id = item.scan_id;
            finding.current_occurrence_derivation_path = item.occurrence_derivation_path.clone();
            finding.current_observed_package_version = item.observed_package_version.clone();
            finding.resolution_state = item.result.clone();
        }
    }
    // PERFORMANCE: One metadata query hydrates the current finding page and all
    // bounded verification items in the response.
    let requirement_version_ids = detail
        .findings
        .iter()
        .flat_map(|finding| finding.requirement_version_ids.iter().copied())
        .chain(
            detail
                .verification_attempts
                .iter()
                .flat_map(|attempt| attempt.items.iter())
                .flat_map(|item| item.requirement_version_ids.iter().copied()),
        )
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let requirement_metadata =
        poam::finding_requirement_metadata(&mut tx, &requirement_version_ids).await?;
    for finding in &mut detail.findings {
        finding.requirements = sqlx::types::Json(
            requirement_metadata
                .iter()
                .filter(|requirement| {
                    finding
                        .requirement_version_ids
                        .contains(&requirement.requirement_version_id)
                })
                .cloned()
                .collect(),
        );
    }
    for item in detail
        .verification_attempts
        .iter_mut()
        .flat_map(|attempt| attempt.items.iter_mut())
    {
        item.requirements = sqlx::types::Json(
            requirement_metadata
                .iter()
                .filter(|requirement| {
                    item.requirement_version_ids
                        .contains(&requirement.requirement_version_id)
                })
                .cloned()
                .collect(),
        );
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

/// Updates mutable POA&M fields using optimistic revision control.
///
/// # Errors
///
/// Returns an authorization, validation, not-found, conflict, or database
/// error when the update cannot be applied atomically.
pub async fn update(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    request: UpdatePoamRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    if request.owner.is_some() && request.assignee.is_some() {
        return Err(PoamError::Validation(
            "ambiguous_assignee",
            "Provide either owner or assignee in an update, not both".into(),
        ));
    }
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
    let resolved_assignee = match request.assignee.as_ref() {
        Some(assignee) => Some(resolve_assignee_tx(&mut tx, assignee).await?),
        None => None,
    };
    let old:Value=sqlx::query_scalar("SELECT jsonb_build_object('title',title,'plan',plan,'owner',owner,'assignee',poam_assignee_view(poams),'target_date',target_date,'risk',risk) FROM poams WHERE id=$1")
      .bind(id).fetch_one(&mut *tx).await?;
    let owner = resolved_assignee
        .as_ref()
        .map(|assignee| assignee.owner.as_str())
        .or_else(|| request.owner.as_deref().map(str::trim));
    let assignee_changed = resolved_assignee.is_some() || request.owner.is_some();
    sqlx::query(r#"UPDATE poams SET title=COALESCE($2,title),plan=COALESCE($3,plan),owner=COALESCE($4,owner),
        owner_kind=CASE WHEN $5 THEN $6 ELSE owner_kind END,
        owner_user_id=CASE WHEN $5 THEN $7 ELSE owner_user_id END,
        owner_group_name=CASE WHEN $5 THEN $8 ELSE owner_group_name END,
        target_date=CASE WHEN $9 THEN $10 ELSE target_date END,risk=COALESCE($11,risk) WHERE id=$1"#)
        .bind(id).bind(request.title.as_deref().map(str::trim)).bind(request.plan.as_deref().map(str::trim))
        .bind(owner).bind(assignee_changed)
        .bind(resolved_assignee.as_ref().and_then(|assignee| assignee.kind))
        .bind(resolved_assignee.as_ref().and_then(|assignee| assignee.user_id))
        .bind(resolved_assignee.as_ref().and_then(|assignee| assignee.group_name.as_deref()))
        .bind(request.target_date.is_some()).bind(request.target_date.flatten())
        .bind(request.risk.map(PoamRisk::as_str)).execute(&mut *tx).await?;
    let new:Value=sqlx::query_scalar("SELECT jsonb_build_object('title',title,'plan',plan,'owner',owner,'assignee',poam_assignee_view(poams),'target_date',target_date,'risk',risk) FROM poams WHERE id=$1")
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
        ("open", "in_progress" | "blocked" | "awaiting_verification")
            | ("in_progress", "open" | "blocked" | "awaiting_verification")
            | ("blocked", "open" | "in_progress" | "awaiting_verification")
            | ("awaiting_verification", "in_progress" | "blocked")
    )
}

/// Transitions an active POA&M between non-terminal workflow states.
///
/// Completion is excluded and must use [`close`].
///
/// # Errors
///
/// Returns an authorization, validation, not-found, conflict, or database
/// error when the transition is not valid at the supplied revision.
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

/// Adds an audited note to an active POA&M.
///
/// # Errors
///
/// Returns an authorization, validation, not-found, conflict, or database
/// error when the note cannot be recorded at the supplied revision.
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

/// Adds a milestone to an active POA&M.
///
/// # Errors
///
/// Returns an authorization, validation, not-found, conflict, or database
/// error when the milestone cannot be recorded at the supplied revision.
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

/// Updates one milestone and records the changed state in the audit history.
///
/// # Errors
///
/// Returns an authorization, validation, not-found, conflict, or database
/// error when the milestone cannot be updated at the supplied revision.
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

/// Removes one milestone from an active POA&M.
///
/// # Errors
///
/// Returns an authorization, not-found, conflict, or database error when the
/// milestone cannot be removed at the supplied revision.
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

/// Links a current failing finding to an active POA&M.
///
/// The finding must share the POA&M's policy lineage. Another active POA&M
/// must not manage it. Evidence validation and link creation are atomic.
///
/// # Errors
///
/// Returns an authorization, validation, precondition, not-found, conflict, or
/// database error when the finding cannot be linked.
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
    // CONCURRENCY: Resolve authoritative evidence only after acquiring the
    // finding key shared with assessment, derivation-result, and scan writers.
    let key = finding_action_key_tx(
        &mut tx,
        actor,
        request.assessment_id,
        request.finding_id,
        request.observation.as_ref(),
    )
    .await?;
    lock_assessment_finding_key_tx(&mut tx, key).await?;
    let context = finding_action_context_tx(
        &mut tx,
        actor,
        request.assessment_id,
        request.finding_id,
        request.observation.as_ref(),
    )
    .await?;
    if !actor_can_access_systems_tx(&mut tx, actor, &[context.system_id]).await? {
        return Err(PoamError::NotFound);
    }
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
    bump_and_audit(&mut tx,actor,id,"finding_linked",json!({"finding_id":context.finding_id,"assessment_id":context.assessment_id,"observation":&request.observation,"system_id":context.system_id,"policy_lineage_id":context.policy_lineage_id})).await?;
    tx.commit().await?;
    detail(pool, actor, id, clock).await
}

/// Retires an active finding link while retaining at least one finding.
///
/// # Errors
///
/// Returns an authorization, validation, not-found, conflict, or database
/// error when the link cannot be retired at the supplied revision.
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
    let key: (Uuid, Uuid) = sqlx::query_as(
        r#"SELECT finding.system_id,finding.policy_lineage_id
           FROM poam_finding_links link
           JOIN poam_findings finding ON finding.id=link.finding_id
           WHERE link.poam_id=$1 AND finding.id=$2 AND link.retired_at IS NULL"#,
    )
    .bind(id)
    .bind(finding_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PoamError::NotFound)?;
    let mut tx = pool.begin().await?;
    lock_assessment_finding_key_tx(&mut tx, key).await?;
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

/// Links a current exact CVE occurrence to an active exact-CVE POA&M.
///
/// All exact-CVE links in the POA&M must share canonical CVE and package
/// identity. Policy and exact-CVE link families cannot be mixed.
///
/// # Errors
///
/// Returns an authorization, validation, precondition, not-found, conflict, or
/// database error when the occurrence cannot be linked atomically.
pub async fn link_cve_finding(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    request: AddCveFindingRequest,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    let mut tx = pool.begin().await?;
    let occurrence = validate_cve_create_context_tx(&mut tx, actor, &request.observation).await?;
    lock_mutable_poam(&mut tx, actor, id, request.revision).await?;
    let identity: (Option<String>, Option<String>, i64) = sqlx::query_as(
        r#"SELECT min(canonical_cve_id),min(canonical_package_name),COUNT(*)
           FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL"#,
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let (Some(cve_id), Some(package_name), count) = identity else {
        return Err(PoamError::Validation(
            "incompatible_finding",
            "An exact-CVE link cannot be added to a policy POA&M".into(),
        ));
    };
    if count >= MAX_POAM_RELATIONSHIPS {
        return Err(PoamError::Validation(
            "too_many_findings",
            "A POA&M can contain at most 100 active findings".into(),
        ));
    }
    if cve_id != occurrence.canonical_cve_id || package_name != occurrence.canonical_package_name {
        return Err(PoamError::Validation(
            "incompatible_finding",
            "Exact-CVE findings must share canonical CVE and package identity".into(),
        ));
    }
    let cve_finding_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO poam_cve_findings(system_id,canonical_cve_id,canonical_package_name)
           VALUES($1,$2,$3)
           ON CONFLICT(system_id,canonical_cve_id,canonical_package_name)
           DO UPDATE SET canonical_package_name=EXCLUDED.canonical_package_name RETURNING id"#,
    )
    .bind(occurrence.system_id)
    .bind(&occurrence.canonical_cve_id)
    .bind(&occurrence.canonical_package_name)
    .fetch_one(&mut *tx)
    .await?;
    if let Err(error) = sqlx::query(
        r#"INSERT INTO poam_cve_finding_links(poam_id,cve_finding_id,system_id,
              canonical_cve_id,canonical_package_name,baseline_scan_id,
              baseline_scan_derivation_id,baseline_scan_completed_at,
              baseline_generation_snapshot_id,baseline_generation,
              baseline_target_store_path,baseline_occurrence_derivation_path,
              baseline_observed_package_version,linked_by)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)"#,
    )
    .bind(id)
    .bind(cve_finding_id)
    .bind(occurrence.system_id)
    .bind(&occurrence.canonical_cve_id)
    .bind(&occurrence.canonical_package_name)
    .bind(occurrence.scan_id)
    .bind(occurrence.scan_derivation_id)
    .bind(occurrence.scan_completed_at)
    .bind(occurrence.generation_snapshot_id)
    .bind(occurrence.generation)
    .bind(&occurrence.target_store_path)
    .bind(&occurrence.occurrence_derivation_path)
    .bind(&occurrence.observed_package_version)
    .bind(actor.user_id)
    .execute(&mut *tx)
    .await
    {
        return Err(db_conflict(&error).unwrap_or_else(|| error.into()));
    }
    bump_and_audit(
        &mut tx,
        actor,
        id,
        "cve_finding_linked",
        json!({"cve_finding_id":cve_finding_id,"observation":request.observation,
            "system_id":occurrence.system_id,"canonical_cve_id":occurrence.canonical_cve_id,
            "canonical_package_name":occurrence.canonical_package_name}),
    )
    .await?;
    // Build the authorized response while the system and POA&M rows remain
    // locked. An environment move after commit cannot turn success into a
    // post-commit not-found error.
    let detail = cve_poam_detail_tx(&mut tx, actor, id, clock).await?;
    tx.commit().await?;
    Ok(detail)
}

/// Retires one active exact-CVE link while retaining at least one exact finding.
///
/// The operation reloads authorization after its writer locks. When the link is
/// the environment's final active exact link for this POA&M, the operation also
/// retires that environment's SCHEDULED disposition. The response is built
/// before commit.
///
/// # Errors
///
/// Returns an authorization, validation, not-found, conflict, or database
/// error when the link cannot be retired at the supplied revision.
pub async fn unlink_cve_finding(
    pool: &PgPool,
    actor: &PoamActor,
    id: Uuid,
    cve_finding_id: Uuid,
    revision: i64,
    clock: &dyn PoamClock,
) -> Result<PoamDetail, PoamError> {
    require_mutator(actor)?;
    require_visible(pool, actor, id).await?;
    let key = sqlx::query_as::<_, CveFindingKey>(
        r#"SELECT finding.id,finding.system_id,finding.canonical_cve_id,
                  finding.canonical_package_name
           FROM poam_cve_finding_links link
           JOIN poam_cve_findings finding ON finding.id=link.cve_finding_id
           WHERE link.poam_id=$1 AND finding.id=$2 AND link.retired_at IS NULL"#,
    )
    .bind(id)
    .bind(cve_finding_id)
    .fetch_optional(pool)
    .await?
    .ok_or(PoamError::NotFound)?;
    let mut tx = pool.begin().await?;
    lock_fleet_cve_scope_tx(
        &mut tx,
        &key.canonical_cve_id,
        &[key.system_id],
        &key.canonical_package_name,
    )
    .await?;
    let environment_id: Option<Uuid> =
        sqlx::query_scalar("SELECT environment_id FROM systems WHERE id=$1 FOR UPDATE")
            .bind(key.system_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(PoamError::NotFound)?;
    let actor = current_mutating_actor_tx(&mut tx, actor).await?;
    if !actor_can_access_systems_tx(&mut tx, &actor, &[key.system_id]).await? {
        return Err(PoamError::NotFound);
    }
    lock_mutable_poam(&mut tx, &actor, id, revision).await?;
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
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
    let affected = sqlx::query("UPDATE poam_cve_finding_links SET retired_at=NOW(),retired_by=$3,retirement_reason='unlinked' WHERE poam_id=$1 AND cve_finding_id=$2 AND retired_at IS NULL")
        .bind(id).bind(cve_finding_id).bind(actor.user_id).execute(&mut *tx).await?.rows_affected();
    if affected == 0 {
        return Err(PoamError::NotFound);
    }
    if let Some(environment_id) = environment_id {
        sqlx::query(
            r#"UPDATE cve_environment_dispositions disposition
               SET retired_at=$5,retired_by=$6,retirement_reason='poam_environment_unlinked'
               WHERE disposition.canonical_cve_id=$1
                 AND disposition.canonical_package_name=$2
                 AND disposition.environment_id=$3 AND disposition.poam_id=$4
                 AND disposition.state='scheduled' AND disposition.retired_at IS NULL
                 AND NOT EXISTS(
                   SELECT 1 FROM poam_cve_finding_links link
                   JOIN systems system ON system.id=link.system_id
                   WHERE link.poam_id=$4 AND link.retired_at IS NULL
                     AND link.canonical_cve_id=$1
                     AND link.canonical_package_name=$2
                     AND system.environment_id=$3)"#,
        )
        .bind(&key.canonical_cve_id)
        .bind(&key.canonical_package_name)
        .bind(environment_id)
        .bind(id)
        .bind(clock.now())
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await?;
    }
    bump_and_audit(
        &mut tx,
        &actor,
        id,
        "cve_finding_unlinked",
        json!({"cve_finding_id":cve_finding_id}),
    )
    .await?;
    let detail = cve_poam_detail_tx(&mut tx, &actor, id, clock).await?;
    tx.commit().await?;
    Ok(detail)
}

/// Links an immutable assignment version to an active POA&M.
///
/// The assignment must be visible to the actor. Its scope and policy lineage
/// must overlap an active finding.
///
/// # Errors
///
/// Returns an authorization, validation, not-found, conflict, or database
/// error when the assignment reference cannot be linked.
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
    if finding_contexts.is_empty() {
        let system_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT system_id FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL ORDER BY system_id",
        )
        .bind(id).fetch_all(&mut *tx).await?;
        validate_cve_assignment_compatibility_tx(
            &mut tx,
            &[request.assignment_version_id],
            &system_ids,
        )
        .await?;
    } else {
        validate_assignment_compatibility_tx(
            &mut tx,
            &[request.assignment_version_id],
            &finding_contexts,
        )
        .await?;
    }
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

/// Removes an immutable assignment-version reference from an active POA&M.
///
/// # Errors
///
/// Returns an authorization, not-found, conflict, or database error when the
/// reference cannot be removed at the supplied revision.
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

/// Lists current failing findings that can be linked to a POA&M.
///
/// Candidates are visible to the actor and share the POA&M policy lineage. The
/// service validates current authoritative evidence before returning them.
///
/// # Errors
///
/// Returns a not-found error for an inaccessible POA&M, a validation error for
/// invalid bounds or broad searches, or a database error on query failure.
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

/// Returns visible relationships for current composite assessments.
///
/// When both pagination arguments are absent, the response uses the bounded
/// compatibility page and reports truncation through `historical_has_more` and
/// `historical_next_offset`. Supplying `history_limit` selects an explicit
/// per-finding page; `history_offset` is invalid without a limit.
///
/// # Errors
///
/// Returns [`PoamError::Validation`] for invalid IDs or pagination and
/// [`PoamError::Database`] when a persistence operation fails.
pub async fn finding_relationships(
    pool: &PgPool,
    actor: &PoamActor,
    assessment_ids: &[Uuid],
    history_limit: Option<i64>,
    history_offset: Option<i64>,
    clock: &dyn PoamClock,
) -> Result<Vec<FindingPoamRelationship>, PoamError> {
    if assessment_ids.is_empty() || assessment_ids.len() > MAX_POAM_RELATIONSHIPS as usize {
        return Err(PoamError::Validation(
            "invalid_assessment_ids",
            "Between 1 and 100 assessment IDs are required".into(),
        ));
    }
    let history_page = relationship_page_bounds(history_limit, history_offset)?;
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
        history_page,
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
            let mut historical_poams = summaries
                .iter()
                .filter(|(related_finding, active, summary)| {
                    *related_finding == finding_id
                        && !*active
                        && Some(summary.id) != active_id
                        && seen.insert(summary.id)
                })
                .map(|(_, _, summary)| summary.clone())
                .collect::<Vec<_>>();
            let historical_has_more = history_page
                .is_some_and(|(history_limit, _)| historical_poams.len() as i64 > history_limit);
            if let Some((history_limit, _)) = history_page {
                historical_poams.truncate(history_limit as usize);
            }
            FindingPoamRelationship {
                assessment_id: Some(assessment_id),
                finding_id,
                active_poam,
                historical_poams,
                historical_has_more,
                historical_next_offset: history_page.and_then(|(history_limit, history_offset)| {
                    historical_has_more.then_some(history_offset + history_limit)
                }),
            }
        })
        .collect())
}

/// Returns visible POA&M relationships for stable finding IDs.
///
/// Findings outside the actor's environment scope are omitted. The request
/// must contain between 1 and 100 IDs. When both pagination arguments are
/// absent, the response uses the bounded compatibility page and reports its
/// continuation metadata. `history_offset` requires `history_limit`.
///
/// # Errors
///
/// Returns [`PoamError::Validation`] for an empty or oversized request and
/// [`PoamError::Database`] when persistence queries fail.
pub async fn finding_relationships_by_finding(
    pool: &PgPool,
    actor: &PoamActor,
    finding_ids: &[Uuid],
    history_limit: Option<i64>,
    history_offset: Option<i64>,
    clock: &dyn PoamClock,
) -> Result<Vec<FindingPoamRelationship>, PoamError> {
    if finding_ids.is_empty() || finding_ids.len() > MAX_POAM_RELATIONSHIPS as usize {
        return Err(PoamError::Validation(
            "invalid_finding_ids",
            "Between 1 and 100 finding IDs are required".into(),
        ));
    }
    let history_page = relationship_page_bounds(history_limit, history_offset)?;
    let visible =
        poam::visible_findings(pool, finding_ids, actor.is_admin, &actor.environment_ids).await?;
    let visible_ids = visible.iter().map(|row| row.0).collect::<Vec<_>>();
    let summaries = poam::finding_poam_summaries(
        pool,
        &visible_ids,
        clock.today(),
        actor.is_admin,
        &actor.environment_ids,
        history_page,
    )
    .await?;
    Ok(visible
        .into_iter()
        .map(|(finding_id, _, _)| {
            let active_poam = summaries
                .iter()
                .find(|(related_finding, active, _)| *related_finding == finding_id && *active)
                .map(|(_, _, summary)| summary.clone());
            let active_id = active_poam.as_ref().map(|summary| summary.id);
            let mut seen = BTreeSet::new();
            let mut historical_poams = summaries
                .iter()
                .filter(|(related_finding, active, summary)| {
                    *related_finding == finding_id
                        && !*active
                        && Some(summary.id) != active_id
                        && seen.insert(summary.id)
                })
                .map(|(_, _, summary)| summary.clone())
                .collect::<Vec<_>>();
            let historical_has_more = history_page
                .is_some_and(|(history_limit, _)| historical_poams.len() as i64 > history_limit);
            if let Some((history_limit, _)) = history_page {
                historical_poams.truncate(history_limit as usize);
            }
            FindingPoamRelationship {
                assessment_id: None,
                finding_id,
                active_poam,
                historical_poams,
                historical_has_more,
                historical_next_offset: history_page.and_then(|(history_limit, history_offset)| {
                    historical_has_more.then_some(history_offset + history_limit)
                }),
            }
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct FleetCveSubject {
    system_id: Uuid,
    hostname: String,
    environment_id: Uuid,
    environment_name: String,
    primary_ip_address: Option<String>,
    flake_name: Option<String>,
    flake_id: Option<i32>,
    commit_hash: Option<String>,
    deployment_policy: String,
    observed_package_version: String,
    scan_id: Uuid,
    scan_derivation_id: i32,
    scan_completed_at: DateTime<Utc>,
    generation_snapshot_id: Uuid,
    generation: i32,
    target_store_path: String,
    occurrence_derivation_path: String,
}

impl FleetCveSubject {
    fn baseline(&self) -> CveLinkBaseline {
        CveLinkBaseline {
            system_id: self.system_id,
            scan_id: self.scan_id,
            scan_derivation_id: self.scan_derivation_id,
            scan_completed_at: self.scan_completed_at,
            generation_snapshot_id: self.generation_snapshot_id,
            generation: self.generation,
            target_store_path: self.target_store_path.clone(),
            occurrence_derivation_path: self.occurrence_derivation_path.clone(),
            observed_package_version: self.observed_package_version.clone(),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct EnvironmentDispositionRow {
    environment_id: Uuid,
    state: String,
    justification: Option<String>,
    review_date: Option<NaiveDate>,
    accepted_by: Option<Uuid>,
    accepted_at: Option<DateTime<Utc>>,
    poam_id: Option<Uuid>,
    scheduled_by: Option<Uuid>,
    scheduled_at: Option<DateTime<Utc>>,
    actor_display: String,
    active_poam_id: Option<Uuid>,
    poam_human_id: Option<String>,
    poam_title: Option<String>,
    poam_plan: Option<String>,
    poam_target_date: Option<NaiveDate>,
    poam_risk: Option<String>,
    poam_assignee: Option<sqlx::types::Json<PoamAssigneeView>>,
}

async fn fleet_cve_subjects_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    cve_id: &str,
    package_name: &str,
    environment_ids: Option<&[Uuid]>,
) -> Result<Vec<FleetCveSubject>, PoamError> {
    let requested_environments = environment_ids.unwrap_or(&[]);
    let rows = sqlx::query_as::<_, FleetCveSubject>(
        r#"WITH current_subjects AS (
             SELECT DISTINCT ON (system.id)
                    system.id AS system_id,system.hostname,
                    system.environment_id,environment.name AS environment_name,
                    state.primary_ip_address,flake.name AS flake_name,
                    flake.id AS flake_id,commit.git_commit_hash AS commit_hash,
                    system.deployment_policy,
                    observation.observed_package_version,
                    scan.id AS scan_id,derivation.id AS scan_derivation_id,
                    scan.completed_at AS scan_completed_at,
                    retained.id AS generation_snapshot_id,retained.generation,
                    retained.source_store_path AS target_store_path,
                    observation.observed_derivation_path AS occurrence_derivation_path
             FROM systems system
             JOIN environments environment ON environment.id=system.environment_id
             JOIN LATERAL (
               SELECT current.store_path,current.generation,current.primary_ip_address
               FROM system_states current
               WHERE current.hostname=system.hostname
                 AND current.store_path IS NOT NULL
                 AND current.generation IS NOT NULL
                 AND current.generation_matches_current_store_path IS TRUE
                 AND btrim(current.store_path)<>''
               ORDER BY current.timestamp DESC,current.id DESC LIMIT 1
             ) state ON true
             JOIN evaluation_generation_snapshots retained
               ON retained.system_id=system.id
              AND retained.generation=state.generation
              AND retained.source_store_path=state.store_path
              AND retained.lineage_verified
             JOIN evaluation_snapshots artifact
               ON artifact.id=retained.snapshot_id
              AND artifact.commit_id=retained.commit_id
              AND artifact.configuration_name=retained.configuration_name
              AND artifact.lifecycle='available' AND artifact.integrity_version=1
             JOIN derivations derivation
               ON derivation.id=retained.derivation_id
              AND derivation.commit_id=retained.commit_id
              AND derivation.derivation_name=retained.configuration_name
              AND derivation.derivation_type='nixos'
              AND COALESCE(derivation.store_path,derivation.expected_store_path)
                  =retained.source_store_path
             JOIN commits commit ON commit.id=retained.commit_id
             LEFT JOIN flakes flake ON flake.id=system.flake_id
             JOIN LATERAL (
               SELECT scan.id,scan.completed_at FROM cve_scans scan
               WHERE scan.derivation_id=derivation.id
                 AND scan.status='completed' AND scan.completed_at IS NOT NULL
                 AND scan.evidence_schema_version=1
               ORDER BY scan.completed_at DESC,scan.id DESC LIMIT 1
             ) scan ON true
             JOIN cve_scan_vulnerability_observations observation
               ON observation.scan_id=scan.id
              AND observation.canonical_cve_id=$1
              AND observation.canonical_package_name=$2
              AND NOT observation.is_whitelisted
             WHERE system.is_active
               AND ($3 OR system.environment_id=ANY($4))
               AND ($5 OR system.environment_id=ANY($6))
             ORDER BY system.id,observation.observed_derivation_path
           )
           SELECT * FROM current_subjects ORDER BY environment_name,hostname,system_id
           LIMIT $7"#,
    )
    .bind(cve_id)
    .bind(package_name)
    .bind(actor.is_admin)
    .bind(&actor.environment_ids)
    .bind(environment_ids.is_none())
    .bind(requested_environments)
    .bind((MAX_FLEET_CVE_SUBJECTS + 1) as i64)
    .fetch_all(&mut **tx)
    .await?;
    if rows.len() > MAX_FLEET_CVE_SUBJECTS {
        return Err(PoamError::Validation(
            "cve_scope_too_large",
            "Exact CVE triage is limited to 1000 currently affected systems".into(),
        ));
    }
    Ok(rows)
}

async fn fleet_cve_dispositions_tx(
    tx: &mut Transaction<'_, Postgres>,
    cve_id: &str,
    package_name: &str,
    environment_ids: &[Uuid],
) -> Result<BTreeMap<Uuid, CveEnvironmentDisposition>, PoamError> {
    let rows = sqlx::query_as::<_, EnvironmentDispositionRow>(
        r#"SELECT disposition.environment_id,disposition.state,
                  disposition.justification,disposition.review_date,
                  disposition.accepted_by,disposition.accepted_at,
                  disposition.poam_id,disposition.scheduled_by,
                  disposition.scheduled_at,
                   COALESCE(NULLIF(btrim(concat_ws(' ',actor.first_name,actor.last_name)),''),
                            NULLIF(btrim(actor.username),''),actor.email,
                            COALESCE(disposition.accepted_by,disposition.scheduled_by)::text)
                     AS actor_display,
                   poam.id AS active_poam_id,
                   CASE WHEN poam.id IS NOT NULL
                     THEN 'POAM-' || lpad(poam.human_number::text,4,'0')
                   END AS poam_human_id,
                   poam.title AS poam_title,poam.plan AS poam_plan,
                   poam.target_date AS poam_target_date,poam.risk AS poam_risk,
                   CASE WHEN poam.id IS NOT NULL THEN poam_assignee_view(poam) END
                     AS poam_assignee
           FROM cve_current_environment_dispositions disposition
           LEFT JOIN users actor ON actor.id=COALESCE(
              disposition.accepted_by,disposition.scheduled_by)
           LEFT JOIN poams poam ON poam.id=disposition.poam_id
             AND poam.status<>'completed'
           WHERE disposition.canonical_cve_id=$1
             AND disposition.canonical_package_name=$2
             AND disposition.environment_id=ANY($3)
           ORDER BY disposition.environment_id"#,
    )
    .bind(cve_id)
    .bind(package_name)
    .bind(environment_ids)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let disposition = match row.state.as_str() {
                "accepted" => CveEnvironmentDisposition::Accepted {
                    justification: row.justification.ok_or_else(|| {
                        PoamError::Database(anyhow::anyhow!(
                            "accepted CVE disposition lacks justification"
                        ))
                    })?,
                    review_date: row.review_date,
                    actor: CveDispositionActor {
                        user_id: row.accepted_by.ok_or_else(|| {
                            PoamError::Database(anyhow::anyhow!(
                                "accepted CVE disposition lacks actor"
                            ))
                        })?,
                        display: row.actor_display,
                    },
                    accepted_at: row.accepted_at.ok_or_else(|| {
                        PoamError::Database(anyhow::anyhow!(
                            "accepted CVE disposition lacks timestamp"
                        ))
                    })?,
                },
                "scheduled" => CveEnvironmentDisposition::Scheduled {
                    poam_id: row.poam_id.ok_or_else(|| {
                        PoamError::Database(anyhow::anyhow!(
                            "scheduled CVE disposition lacks POA&M"
                        ))
                    })?,
                    poam: {
                        let id = row.active_poam_id.ok_or_else(|| {
                            PoamError::Database(anyhow::anyhow!(
                                "scheduled CVE disposition lacks active POA&M metadata"
                            ))
                        })?;
                        if row.poam_id != Some(id) {
                            return Err(PoamError::Database(anyhow::anyhow!(
                                "scheduled CVE disposition POA&M metadata does not match"
                            )));
                        }
                        let risk = match row.poam_risk.as_deref() {
                            Some("high") => PoamRisk::High,
                            Some("medium") => PoamRisk::Medium,
                            Some("low") => PoamRisk::Low,
                            _ => {
                                return Err(PoamError::Database(anyhow::anyhow!(
                                    "scheduled CVE disposition has incompatible POA&M risk"
                                )));
                            }
                        };
                        ScheduledPoamMetadata {
                            id,
                            human_id: row.poam_human_id.ok_or_else(|| {
                                PoamError::Database(anyhow::anyhow!(
                                    "scheduled CVE disposition lacks POA&M human ID"
                                ))
                            })?,
                            title: row.poam_title.ok_or_else(|| {
                                PoamError::Database(anyhow::anyhow!(
                                    "scheduled CVE disposition lacks POA&M title"
                                ))
                            })?,
                            plan: row.poam_plan.ok_or_else(|| {
                                PoamError::Database(anyhow::anyhow!(
                                    "scheduled CVE disposition lacks POA&M plan"
                                ))
                            })?,
                            target_date: row.poam_target_date.ok_or_else(|| {
                                PoamError::Database(anyhow::anyhow!(
                                    "scheduled CVE disposition lacks POA&M target date"
                                ))
                            })?,
                            risk,
                            assignee: row
                                .poam_assignee
                                .ok_or_else(|| {
                                    PoamError::Database(anyhow::anyhow!(
                                        "scheduled CVE disposition lacks POA&M assignee"
                                    ))
                                })?
                                .0,
                        }
                    },
                    actor: CveDispositionActor {
                        user_id: row.scheduled_by.ok_or_else(|| {
                            PoamError::Database(anyhow::anyhow!(
                                "scheduled CVE disposition lacks actor"
                            ))
                        })?,
                        display: row.actor_display,
                    },
                    scheduled_at: row.scheduled_at.ok_or_else(|| {
                        PoamError::Database(anyhow::anyhow!(
                            "scheduled CVE disposition lacks timestamp"
                        ))
                    })?,
                },
                _ => {
                    return Err(PoamError::Database(anyhow::anyhow!(
                        "unknown CVE disposition state"
                    )));
                }
            };
            Ok((row.environment_id, disposition))
        })
        .collect()
}

async fn retain_coherent_scheduled_dispositions_tx(
    tx: &mut Transaction<'_, Postgres>,
    dispositions: &mut BTreeMap<Uuid, CveEnvironmentDisposition>,
    subjects: &[FleetCveSubject],
    cve_id: &str,
    package_name: &str,
) -> Result<(), PoamError> {
    let scheduled = dispositions
        .iter()
        .filter_map(|(environment_id, disposition)| match disposition {
            CveEnvironmentDisposition::Scheduled { poam_id, .. } => {
                Some((*environment_id, *poam_id))
            }
            CveEnvironmentDisposition::Accepted { .. } => None,
        })
        .collect::<Vec<_>>();
    if scheduled.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, Vec<Uuid>)>(
        r#"SELECT requested.environment_id,requested.poam_id,poam.status,
                  COALESCE(array_agg(link.system_id ORDER BY link.system_id)
                    FILTER (WHERE link.system_id IS NOT NULL),'{}'::uuid[])
           FROM UNNEST($1::uuid[],$2::uuid[]) requested(environment_id,poam_id)
           JOIN poams poam ON poam.id=requested.poam_id
           LEFT JOIN poam_cve_finding_links link
             ON link.poam_id=requested.poam_id AND link.retired_at IS NULL
            AND link.canonical_cve_id=$3 AND link.canonical_package_name=$4
            AND EXISTS(SELECT 1 FROM systems system WHERE system.id=link.system_id
                       AND system.environment_id=requested.environment_id)
           GROUP BY requested.environment_id,requested.poam_id,poam.status
           ORDER BY requested.environment_id"#,
    )
    .bind(scheduled.iter().map(|row| row.0).collect::<Vec<_>>())
    .bind(scheduled.iter().map(|row| row.1).collect::<Vec<_>>())
    .bind(cve_id)
    .bind(package_name)
    .fetch_all(&mut **tx)
    .await?;
    for (environment_id, poam_id) in scheduled {
        let expected = subjects
            .iter()
            .filter(|subject| subject.environment_id == environment_id)
            .map(|subject| subject.system_id)
            .collect::<BTreeSet<_>>();
        let coherent = rows.iter().any(|row| {
            row.0 == environment_id
                && row.1 == poam_id
                && row.2 != "completed"
                && row.3.iter().copied().collect::<BTreeSet<_>>() == expected
        });
        if !coherent {
            // SECURITY: Corrupt, stale, or completed remediation references
            // fail closed as OPEN instead of claiming current fleet coverage.
            dispositions.remove(&environment_id);
        }
    }
    Ok(())
}

fn fleet_cve_rollup(environments: &[CveAffectedEnvironment]) -> FleetCveTriageRollup {
    let accepted = environments
        .iter()
        .filter(|environment| {
            matches!(
                environment.disposition,
                Some(CveEnvironmentDisposition::Accepted { .. })
            )
        })
        .count();
    let scheduled = environments
        .iter()
        .filter(|environment| {
            matches!(
                environment.disposition,
                Some(CveEnvironmentDisposition::Scheduled { .. })
            )
        })
        .count();
    match (accepted, scheduled, environments.len()) {
        (0, 0, _) => FleetCveTriageRollup::Outstanding,
        (accepted, 0, total) if accepted == total => FleetCveTriageRollup::Accepted,
        (0, scheduled, total) if scheduled == total => FleetCveTriageRollup::Scheduled,
        _ => FleetCveTriageRollup::Partial,
    }
}

/// Returns the visible fleet drawer state for one exact CVE/package identity.
///
/// The read uses only exact retained-generation evidence. It does not enqueue a
/// scan, infer an occurrence from mutable package rows, or disclose hidden
/// environments.
///
/// # Errors
///
/// Returns not found when no current subject is visible, validation for an
/// invalid identity, or a database error when the bounded read fails.
async fn fleet_cve_detail_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    cve_id: &str,
    package_name: &str,
) -> Result<FleetCveDetail, PoamError> {
    let subjects = fleet_cve_subjects_tx(tx, actor, cve_id, package_name, None).await?;
    if subjects.is_empty() {
        return Err(PoamError::NotFound);
    }
    let environment_ids = subjects
        .iter()
        .map(|subject| subject.environment_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut dispositions =
        fleet_cve_dispositions_tx(tx, cve_id, package_name, &environment_ids).await?;
    retain_coherent_scheduled_dispositions_tx(
        tx,
        &mut dispositions,
        &subjects,
        cve_id,
        package_name,
    )
    .await?;
    let read_scope = if actor.is_admin {
        crate::queries::cves::CveReadScope::All
    } else {
        crate::queries::cves::CveReadScope::Environments(actor.environment_ids.clone())
    };
    let detail =
        crate::queries::cves::fetch_cve_detail_tx(tx, &read_scope, cve_id, package_name).await?;
    let mut environments = Vec::with_capacity(environment_ids.len());
    for environment_id in environment_ids {
        let environment_subjects = subjects
            .iter()
            .filter(|subject| subject.environment_id == environment_id)
            .collect::<Vec<_>>();
        let environment_name = environment_subjects[0].environment_name.clone();
        let systems = environment_subjects
            .iter()
            .map(|subject| CveAffectedSystemDetail {
                system_id: subject.system_id,
                hostname: subject.hostname.clone(),
                environment: Some(subject.environment_name.clone()),
                primary_ip_address: subject.primary_ip_address.clone(),
                flake_name: subject.flake_name.clone(),
                flake_id: subject.flake_id,
                commit_hash: subject.commit_hash.clone(),
                deployment_policy: subject.deployment_policy.clone(),
                current_package_version: Some(subject.observed_package_version.clone()),
            })
            .collect::<Vec<_>>();
        environments.push(CveAffectedEnvironment {
            environment_id,
            environment_name,
            affected_system_count: systems.len() as i64,
            systems,
            disposition: dispositions.remove(&environment_id),
        });
    }
    Ok(FleetCveDetail {
        cve: detail,
        canonical_package_name: package_name.to_owned(),
        rollup: fleet_cve_rollup(&environments),
        affected_system_count: subjects.len() as i64,
        environments,
    })
}

/// Returns the visible fleet drawer state for one exact CVE/package identity.
///
/// The read uses only exact retained-generation evidence. It does not enqueue a
/// scan, infer an occurrence from mutable package rows, or disclose hidden
/// environments.
///
/// # Errors
///
/// Returns not found when no current subject is visible, validation for an
/// invalid identity, or a database error when the bounded read fails.
pub async fn fleet_cve_detail(
    pool: &PgPool,
    actor: &PoamActor,
    cve_id: &str,
    package_name: &str,
) -> Result<FleetCveDetail, PoamError> {
    let cve_id = cve_id.trim().to_ascii_uppercase();
    let package_name = package_name.trim();
    if !is_canonical_cve_id(&cve_id) || package_name.is_empty() {
        return Err(PoamError::Validation(
            "invalid_cve_identity",
            "A canonical CVE ID and package name are required".into(),
        ));
    }
    validate_text_length(
        package_name,
        MAX_SHORT_TEXT_BYTES,
        "text_too_long",
        "package",
    )?;
    let mut tx = pool.begin().await?;
    let detail = fleet_cve_detail_tx(&mut tx, actor, &cve_id, package_name).await?;
    tx.commit().await?;
    Ok(detail)
}

async fn lock_fleet_cve_scope_tx(
    tx: &mut Transaction<'_, Postgres>,
    cve_id: &str,
    system_ids: &[Uuid],
    package_name: &str,
) -> Result<(), PoamError> {
    sqlx::query("SELECT lock_poam_cve_key($1)")
        .bind(cve_id)
        .execute(&mut **tx)
        .await?;
    // CONCURRENCY: Acquire every level for the complete request before any
    // evidence or lifecycle row is locked. Never interleave policy and exact
    // keys per system; that order deadlocks with fleet operations.
    for system_id in system_ids.iter().copied().collect::<BTreeSet<_>>() {
        crate::services::composite_enforcement::lock_poam_system_key_tx(tx, system_id).await?;
    }
    sqlx::query(
        r#"SELECT lock_poam_finding_key(key.system_id,key.policy_lineage_id)
           FROM (
             SELECT finding.system_id,finding.policy_lineage_id
             FROM poam_findings finding WHERE finding.system_id=ANY($1)
             ORDER BY finding.system_id,finding.policy_lineage_id
           ) key"#,
    )
    .bind(system_ids)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"SELECT lock_poam_cve_finding_key(
                    key.system_id,key.canonical_cve_id,key.canonical_package_name)
           FROM (
             SELECT DISTINCT system_id,canonical_cve_id,canonical_package_name
             FROM (
               SELECT finding.system_id,finding.canonical_cve_id,
                      finding.canonical_package_name
               FROM poam_cve_findings finding WHERE finding.system_id=ANY($1)
               UNION ALL
               SELECT requested.system_id,$2,$3
               FROM unnest($1::uuid[]) requested(system_id)
             ) all_keys
             ORDER BY system_id,canonical_cve_id,canonical_package_name
           ) key"#,
    )
    .bind(system_ids)
    .bind(cve_id)
    .bind(package_name)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn materialize_exact_subjects_tx(
    tx: &mut Transaction<'_, Postgres>,
    poam_id: Uuid,
    actor_id: Uuid,
    baselines: &[CveLinkBaseline],
    cve_id: &str,
    package_name: &str,
) -> Result<Vec<Uuid>, PoamError> {
    let mut finding_ids = Vec::with_capacity(baselines.len());
    for baseline in baselines {
        let finding_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO poam_cve_findings(
                  system_id,canonical_cve_id,canonical_package_name)
               VALUES($1,$2,$3)
               ON CONFLICT(system_id,canonical_cve_id,canonical_package_name)
               DO UPDATE SET canonical_package_name=EXCLUDED.canonical_package_name
               RETURNING id"#,
        )
        .bind(baseline.system_id)
        .bind(cve_id)
        .bind(package_name)
        .fetch_one(&mut **tx)
        .await?;
        if let Err(error) = sqlx::query(
            r#"INSERT INTO poam_cve_finding_links(
                  poam_id,cve_finding_id,system_id,canonical_cve_id,
                  canonical_package_name,baseline_scan_id,
                  baseline_scan_derivation_id,baseline_scan_completed_at,
                  baseline_generation_snapshot_id,baseline_generation,
                  baseline_target_store_path,baseline_occurrence_derivation_path,
                  baseline_observed_package_version,linked_by)
               VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)"#,
        )
        .bind(poam_id)
        .bind(finding_id)
        .bind(baseline.system_id)
        .bind(cve_id)
        .bind(package_name)
        .bind(baseline.scan_id)
        .bind(baseline.scan_derivation_id)
        .bind(baseline.scan_completed_at)
        .bind(baseline.generation_snapshot_id)
        .bind(baseline.generation)
        .bind(&baseline.target_store_path)
        .bind(&baseline.occurrence_derivation_path)
        .bind(&baseline.observed_package_version)
        .bind(actor_id)
        .execute(&mut **tx)
        .await
        {
            return Err(db_conflict(&error).unwrap_or_else(|| error.into()));
        }
        finding_ids.push(finding_id);
    }
    Ok(finding_ids)
}

async fn insert_fleet_cve_poam_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &PoamActor,
    subjects: &[FleetCveSubject],
    cve_id: &str,
    package_name: &str,
    request: &FleetCvePoamRequest,
    resolved_assignee: &ResolvedAssignee,
    clock: &dyn PoamClock,
) -> Result<Uuid, PoamError> {
    let poam_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO poams(title,plan,owner,owner_kind,owner_user_id,
              owner_group_name,target_date,risk,created_by)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id"#,
    )
    .bind(request.title.trim())
    .bind(request.plan.trim())
    .bind(&resolved_assignee.owner)
    .bind(resolved_assignee.kind)
    .bind(resolved_assignee.user_id)
    .bind(resolved_assignee.group_name.as_deref())
    .bind(request.target_date)
    .bind(request.risk.as_str())
    .bind(actor.user_id)
    .fetch_one(&mut **tx)
    .await?;
    materialize_exact_subjects_tx(
        tx,
        poam_id,
        actor.user_id,
        &subjects
            .iter()
            .map(FleetCveSubject::baseline)
            .collect::<Vec<_>>(),
        cve_id,
        package_name,
    )
    .await?;
    if request.default_milestones {
        for (ordinal, (offset, title)) in [14_i64, 28, 35, 49, 56]
            .into_iter()
            .zip([
                "Update NixOS module",
                "Deploy to staging",
                "Validate new configuration",
                "Deploy to production",
                "Verify CVE remediation passes",
            ])
            .enumerate()
        {
            let target_date = if ordinal == 4 {
                request.target_date
            } else {
                (clock.today() + Duration::days(offset)).min(request.target_date)
            };
            sqlx::query("INSERT INTO poam_milestones(poam_id,ordinal,title,target_date,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$5)")
                .bind(poam_id).bind(ordinal as i32).bind(title).bind(target_date)
                .bind(actor.user_id).execute(&mut **tx).await?;
        }
    }
    insert_activity_and_audit(
        tx,
        poam_id,
        actor.user_id,
        &actor.identifier,
        "created",
        &json!({
            "poam_id":poam_id,"revision":1,"source":"fleet_cve_triage",
            "canonical_cve_id":cve_id,"canonical_package_name":package_name,
            "subject_count":subjects.len()
        }),
        actor.request_origin.as_deref(),
    )
    .await?;
    Ok(poam_id)
}

async fn compatible_existing_fleet_poam_tx(
    tx: &mut Transaction<'_, Postgres>,
    poam_id: Uuid,
    request: &FleetCvePoamRequest,
    assignee: &ResolvedAssignee,
    expected_system_ids: &BTreeSet<Uuid>,
    cve_id: &str,
    package_name: &str,
) -> Result<bool, PoamError> {
    let row: Option<(
        String,
        String,
        String,
        Option<String>,
        Option<Uuid>,
        Option<String>,
        Option<NaiveDate>,
        String,
        String,
    )> = sqlx::query_as(
        r#"SELECT title,plan,owner,owner_kind,owner_user_id,owner_group_name,
                  target_date,risk,status FROM poams WHERE id=$1 FOR SHARE"#,
    )
    .bind(poam_id)
    .fetch_optional(&mut **tx)
    .await?;
    let metadata_matches = row.is_some_and(|row| {
        row.0 == request.title.trim()
            && row.1 == request.plan.trim()
            && row.2 == assignee.owner
            && row.3.as_deref() == assignee.kind
            && row.4 == assignee.user_id
            && row.5 == assignee.group_name
            && row.6 == Some(request.target_date)
            && row.7 == request.risk.as_str()
            && row.8 != "completed"
    });
    if !metadata_matches {
        return Ok(false);
    }
    let active_policy_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM poam_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
    )
    .bind(poam_id)
    .fetch_one(&mut **tx)
    .await?;
    let active_exact_subjects = sqlx::query_as::<_, (Uuid, String, String)>(
        r#"SELECT system_id,canonical_cve_id,canonical_package_name
           FROM poam_cve_finding_links
           WHERE poam_id=$1 AND retired_at IS NULL
           ORDER BY system_id,canonical_cve_id,canonical_package_name"#,
    )
    .bind(poam_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect::<BTreeSet<_>>();
    let expected_subjects = expected_system_ids
        .iter()
        .map(|system_id| (*system_id, cve_id.to_owned(), package_name.to_owned()))
        .collect::<BTreeSet<_>>();
    Ok(active_policy_count == 0 && active_exact_subjects == expected_subjects)
}

/// Applies environment-scoped exact-CVE triage in one transaction.
///
/// The server derives every subject from current retained-generation evidence.
/// The request cannot supply or omit host identities. All scheduled environments
/// use one POA&M, while accepted environments produce no remediation or PASS
/// evidence. The operation reloads authorization after its writer locks and
/// builds the response before commit.
///
/// # Errors
///
/// Returns authorization, validation, bounded typed conflict, not-found, or
/// database errors when the complete action set cannot commit atomically.
pub async fn triage_fleet_cve(
    pool: &PgPool,
    actor: &PoamActor,
    cve_id: &str,
    request: FleetCveTriageRequest,
    clock: &dyn PoamClock,
) -> Result<FleetCveTriageResponse, PoamError> {
    for retry in 0..3 {
        match triage_fleet_cve_once(pool, actor, cve_id, request.clone(), clock).await {
            Err(PoamError::Database(error)) if retry < 2 && is_serialization_failure(&error) => {
                continue;
            }
            result => return result,
        }
    }
    unreachable!()
}

async fn triage_fleet_cve_once(
    pool: &PgPool,
    actor: &PoamActor,
    cve_id: &str,
    request: FleetCveTriageRequest,
    clock: &dyn PoamClock,
) -> Result<FleetCveTriageResponse, PoamError> {
    require_mutator(actor)?;
    let cve_id = cve_id.trim().to_ascii_uppercase();
    let package_name = request.canonical_package_name.trim().to_owned();
    if !is_canonical_cve_id(&cve_id) || package_name.is_empty() {
        return Err(PoamError::Validation(
            "invalid_cve_identity",
            "A canonical CVE ID and package name are required".into(),
        ));
    }
    validate_text_length(
        &package_name,
        MAX_SHORT_TEXT_BYTES,
        "text_too_long",
        "package",
    )?;
    if request.actions.is_empty() || request.actions.len() > MAX_FLEET_CVE_ENVIRONMENTS {
        return Err(PoamError::Validation(
            "invalid_environment_actions",
            "Triage requires between 1 and 100 environment actions".into(),
        ));
    }
    let mut actions = BTreeMap::new();
    let mut duplicate_environment_id = false;
    for action in request.actions {
        if actions.insert(action.environment_id(), action).is_some() {
            duplicate_environment_id = true;
        }
    }
    let scheduled_environment_ids = actions
        .iter()
        .filter_map(|(environment_id, action)| {
            matches!(action, CveEnvironmentTriageAction::SchedulePatch { .. })
                .then_some(*environment_id)
        })
        .collect::<BTreeSet<_>>();
    if scheduled_environment_ids.is_empty() != request.poam.is_none() {
        return Err(PoamError::Validation(
            "invalid_poam_payload",
            "Provide one POA&M payload exactly when an environment schedules patching".into(),
        ));
    }
    for action in actions.values() {
        if let CveEnvironmentTriageAction::AcceptRisk { justification, .. } = action {
            let justification = justification.trim();
            if !(10..=2_000).contains(&justification.len()) {
                return Err(PoamError::Validation(
                    "invalid_acceptance_justification",
                    "Acceptance justification must be between 10 and 2000 bytes".into(),
                ));
            }
        }
    }
    if let Some(poam) = request.poam.as_ref() {
        if poam.title.trim().is_empty() || poam.plan.trim().is_empty() {
            return Err(PoamError::Validation(
                "invalid_poam_payload",
                "Scheduled remediation requires a title and plan".into(),
            ));
        }
        validate_text_length(&poam.title, MAX_SHORT_TEXT_BYTES, "text_too_long", "title")?;
        validate_text_length(&poam.plan, MAX_PLAN_BYTES, "text_too_long", "plan")?;
        if matches!(poam.assignee, PoamAssigneeRequest::Unassigned) {
            return Err(PoamError::Validation(
                "invalid_poam_assignee",
                "Scheduled remediation requires a typed user or group assignee".into(),
            ));
        }
    }

    let requested_environment_ids = actions.keys().copied().collect::<Vec<_>>();
    // READ COMMITTED makes every statement after a lock wait observe the
    // writer that released that lock. Environment and sorted system locks make
    // the resulting subject set stable through commit.
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT lock_poam_cve_key($1)")
        .bind(&cve_id)
        .execute(&mut *tx)
        .await?;
    let before = fleet_cve_subjects_tx(&mut tx, actor, &cve_id, &package_name, None).await?;
    let environment_ids = before
        .iter()
        .map(|subject| subject.environment_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    sqlx::query("SELECT id FROM environments WHERE id=ANY($1) ORDER BY id FOR UPDATE")
        .bind(&environment_ids)
        .execute(&mut *tx)
        .await?;
    let mut system_ids = before
        .iter()
        .map(|subject| subject.system_id)
        .collect::<BTreeSet<_>>();
    let linked_system_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT DISTINCT link.system_id
           FROM poam_cve_finding_links link
           JOIN systems system ON system.id=link.system_id
           WHERE link.retired_at IS NULL
             AND link.canonical_cve_id=$1
             AND link.canonical_package_name=$2
             AND system.environment_id=ANY($3)
           ORDER BY link.system_id"#,
    )
    .bind(&cve_id)
    .bind(&package_name)
    .bind(&environment_ids)
    .fetch_all(&mut *tx)
    .await?;
    system_ids.extend(linked_system_ids);
    let system_ids = system_ids.into_iter().collect::<Vec<_>>();
    lock_fleet_cve_scope_tx(&mut tx, &cve_id, &system_ids, &package_name).await?;
    sqlx::query("SELECT id FROM systems WHERE id=ANY($1) ORDER BY id FOR UPDATE")
        .bind(&system_ids)
        .execute(&mut *tx)
        .await?;
    let actor = current_mutating_actor_tx(&mut tx, actor).await?;
    let subjects = fleet_cve_subjects_tx(&mut tx, &actor, &cve_id, &package_name, None).await?;
    if before
        .iter()
        .map(|row| row.system_id)
        .collect::<BTreeSet<_>>()
        != subjects
            .iter()
            .map(|row| row.system_id)
            .collect::<BTreeSet<_>>()
    {
        return Err(PoamError::Conflict(
            "cve_evidence_changed",
            "Affected systems changed while triage acquired locks; retry the request".into(),
        ));
    }
    let affected_environments = subjects
        .iter()
        .map(|subject| subject.environment_id)
        .collect::<BTreeSet<_>>();
    let requested_environments = requested_environment_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if duplicate_environment_id || requested_environments != affected_environments {
        // SECURITY: The generic conflict does not identify whether an extra ID
        // is unknown, hidden, or merely no longer affected.
        return Err(PoamError::Conflict(
            "cve_evidence_changed",
            "The complete visible affected environment set changed; refresh and retry".into(),
        ));
    }

    let scheduled_subjects = subjects
        .iter()
        .filter(|subject| scheduled_environment_ids.contains(&subject.environment_id))
        .cloned()
        .collect::<Vec<_>>();
    let scheduled_system_ids = scheduled_subjects
        .iter()
        .map(|subject| subject.system_id)
        .collect::<Vec<_>>();
    let active_scheduled_links: Vec<(Uuid, Uuid)> = if scheduled_system_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            r#"SELECT link.system_id,link.poam_id
               FROM poam_cve_finding_links link
               WHERE link.system_id=ANY($1) AND link.canonical_cve_id=$2
                 AND link.canonical_package_name=$3 AND link.retired_at IS NULL
               ORDER BY link.system_id,link.poam_id"#,
        )
        .bind(&scheduled_system_ids)
        .bind(&cve_id)
        .bind(&package_name)
        .fetch_all(&mut *tx)
        .await?
    };
    let retiring_environment_ids = actions
        .iter()
        .filter_map(|(environment_id, action)| {
            (!matches!(action, CveEnvironmentTriageAction::SchedulePatch { .. }))
                .then_some(*environment_id)
        })
        .collect::<Vec<_>>();
    let retiring_links: Vec<(Uuid, Uuid, Uuid)> = if retiring_environment_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            r#"SELECT link.poam_id,link.cve_finding_id,link.system_id
               FROM poam_cve_finding_links link
               JOIN systems system ON system.id=link.system_id
               WHERE link.retired_at IS NULL AND link.canonical_cve_id=$1
                 AND link.canonical_package_name=$2
                 AND system.environment_id=ANY($3)
               ORDER BY link.poam_id,link.system_id,link.cve_finding_id"#,
        )
        .bind(&cve_id)
        .bind(&package_name)
        .bind(&retiring_environment_ids)
        .fetch_all(&mut *tx)
        .await?
    };
    let resolved_assignee = match request.poam.as_ref() {
        Some(poam) => Some(resolve_assignee_tx(&mut tx, &poam.assignee).await?),
        None => None,
    };
    let mut poam_id = None;
    let mut poam_reused = false;
    if let Some(poam_request) = request.poam.as_ref() {
        let resolved_assignee = resolved_assignee.as_ref().ok_or_else(|| {
            PoamError::Database(anyhow::anyhow!(
                "validated scheduled action lacks a resolved assignee"
            ))
        })?;
        let active_poams = active_scheduled_links
            .iter()
            .map(|row| row.1)
            .collect::<BTreeSet<_>>();
        if active_scheduled_links.is_empty() {
            poam_id = Some(
                insert_fleet_cve_poam_tx(
                    &mut tx,
                    &actor,
                    &scheduled_subjects,
                    &cve_id,
                    &package_name,
                    poam_request,
                    resolved_assignee,
                    clock,
                )
                .await?,
            );
        } else if active_scheduled_links.len() == scheduled_subjects.len()
            && active_poams.len() == 1
        {
            let existing_id = *active_poams.first().expect("one active POA&M");
            let expected_existing_system_ids = scheduled_subjects
                .iter()
                .map(|subject| subject.system_id)
                .chain(
                    retiring_links
                        .iter()
                        .filter(|row| row.0 == existing_id)
                        .map(|row| row.2),
                )
                .collect::<BTreeSet<_>>();
            if compatible_existing_fleet_poam_tx(
                &mut tx,
                existing_id,
                poam_request,
                resolved_assignee,
                &expected_existing_system_ids,
                &cve_id,
                &package_name,
            )
            .await?
            {
                poam_id = Some(existing_id);
                poam_reused = true;
            }
        }
        if poam_id.is_none() {
            let conflicts = scheduled_subjects
                .iter()
                .map(|subject| CveTriageConflictSubject {
                    system_id: subject.system_id,
                    hostname: subject.hostname.clone(),
                    environment_id: subject.environment_id,
                    poam_id: active_scheduled_links
                        .iter()
                        .find(|link| link.0 == subject.system_id)
                        .map(|link| link.1),
                })
                .filter(|subject| {
                    subject.poam_id.is_some()
                        || active_scheduled_links.len() != scheduled_subjects.len()
                })
                .take(MAX_FLEET_CVE_CONFLICTS)
                .collect::<Vec<_>>();
            return Err(PoamError::ConflictDetails(
                "cve_subjects_already_managed",
                "Scheduled subjects are partially managed, use another POA&M, or have incompatible POA&M metadata".into(),
                json!({"subjects":conflicts,"truncated":scheduled_subjects.len()>MAX_FLEET_CVE_CONFLICTS}),
            ));
        }
    }

    for retiring_poam_id in retiring_links
        .iter()
        .map(|row| row.0)
        .collect::<BTreeSet<_>>()
    {
        let active_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM poam_cve_finding_links WHERE poam_id=$1 AND retired_at IS NULL",
        )
        .bind(retiring_poam_id)
        .fetch_one(&mut *tx)
        .await?;
        let retiring_count = retiring_links
            .iter()
            .filter(|row| row.0 == retiring_poam_id)
            .count() as i64;
        if active_count <= retiring_count {
            return Err(PoamError::ConflictDetails(
                "poam_final_subject",
                "Changing this disposition would remove the final active subject from a POA&M"
                    .into(),
                json!({"poam_ids":[retiring_poam_id]}),
            ));
        }
    }
    if !retiring_environment_ids.is_empty() {
        sqlx::query(
            r#"UPDATE poam_cve_finding_links link
               SET retired_at=$4,retired_by=$5,retirement_reason='fleet_triage_changed'
               FROM systems system
               WHERE system.id=link.system_id AND link.retired_at IS NULL
                 AND link.canonical_cve_id=$1
                 AND link.canonical_package_name=$2
                 AND system.environment_id=ANY($3)"#,
        )
        .bind(&cve_id)
        .bind(&package_name)
        .bind(&retiring_environment_ids)
        .bind(clock.now())
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await?;
    }

    let current_dispositions =
        fleet_cve_dispositions_tx(&mut tx, &cve_id, &package_name, &environment_ids).await?;
    let now = clock.now();
    for (environment_id, action) in &actions {
        let unchanged_accepted = matches!(
           (action, current_dispositions.get(environment_id)),
           (
               CveEnvironmentTriageAction::AcceptRisk {
                   justification,
                   review_date,
                   ..
               },
                Some(CveEnvironmentDisposition::Accepted {
                    justification: current_justification,
                    review_date: current_review_date,
                    ..
                })
            ) if justification.trim() == current_justification
                && review_date == current_review_date
        );
        if unchanged_accepted {
            continue;
        }
        sqlx::query(
            r#"UPDATE cve_environment_dispositions
               SET retired_at=$4,retired_by=$5,retirement_reason='triage_changed'
               WHERE canonical_cve_id=$1 AND canonical_package_name=$2
                 AND environment_id=$3 AND retired_at IS NULL"#,
        )
        .bind(&cve_id)
        .bind(&package_name)
        .bind(environment_id)
        .bind(now)
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await?;
        match action {
            CveEnvironmentTriageAction::LeaveOpen { .. } => {}
            CveEnvironmentTriageAction::AcceptRisk {
                justification,
                review_date,
                ..
            } => {
                sqlx::query(
                    r#"INSERT INTO cve_environment_dispositions(
                          canonical_cve_id,canonical_package_name,environment_id,
                          state,justification,review_date,accepted_by,accepted_at)
                       VALUES($1,$2,$3,'accepted',$4,$5,$6,$7)"#,
                )
                .bind(&cve_id)
                .bind(&package_name)
                .bind(environment_id)
                .bind(justification.trim())
                .bind(review_date)
                .bind(actor.user_id)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
            CveEnvironmentTriageAction::SchedulePatch { .. } => {
                let scheduled_poam_id = poam_id.ok_or_else(|| {
                    PoamError::Database(anyhow::anyhow!("validated scheduled action lacks a POA&M"))
                })?;
                sqlx::query(
                    r#"INSERT INTO cve_environment_dispositions(
                          canonical_cve_id,canonical_package_name,environment_id,
                          state,poam_id,scheduled_by,scheduled_at)
                       VALUES($1,$2,$3,'scheduled',$4,$5,$6)"#,
                )
                .bind(&cve_id)
                .bind(&package_name)
                .bind(environment_id)
                .bind(scheduled_poam_id)
                .bind(actor.user_id)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    sqlx::query(
        r#"INSERT INTO admin_audit_events(
              actor_user_id,actor_identifier,action,target,request_origin,metadata)
           VALUES($1,$2,'fleet_cve_triaged',$3,$4,$5)"#,
    )
    .bind(actor.user_id)
    .bind(&actor.identifier)
    .bind(format!("cve:{cve_id}:{package_name}"))
    .bind(actor.request_origin.as_deref())
    .bind(json!({
        "canonical_cve_id":cve_id,"canonical_package_name":package_name,
        "environment_ids":environment_ids,"affected_system_count":subjects.len(),
        "poam_id":poam_id,"poam_reused":poam_reused
    }))
    .execute(&mut *tx)
    .await?;
    // Build the authoritative response before commit. A later read race cannot
    // turn a committed triage mutation into an HTTP failure.
    let detail = fleet_cve_detail_tx(&mut tx, &actor, &cve_id, &package_name).await?;
    tx.commit().await?;
    Ok(FleetCveTriageResponse {
        detail,
        poam_id,
        poam_reused,
    })
}

fn is_canonical_cve_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("CVE-") else {
        return false;
    };
    let mut parts = rest.split('-');
    matches!(parts.next(), Some(year) if year.len()==4 && year.bytes().all(|b| b.is_ascii_digit()))
        && matches!(parts.next(), Some(sequence) if sequence.len()>=4 && sequence.bytes().all(|b| b.is_ascii_digit()))
        && parts.next().is_none()
}

/// Returns server-issued exact-CVE occurrence context and POA&M relationships.
///
/// The response is derived only from the latest completed schema-1 scan for
/// the system's exact deployed derivation. Inaccessible systems return
/// [`PoamError::NotFound`] before evidence details are loaded.
///
/// # Errors
///
/// Returns a validation error for invalid history bounds, a not-found error for
/// inaccessible systems, or a database error when evidence cannot be loaded.
pub async fn cve_relationships(
    pool: &PgPool,
    actor: &PoamActor,
    system_id: Uuid,
    history_limit: Option<i64>,
    history_offset: Option<i64>,
    clock: &dyn PoamClock,
) -> Result<Vec<CvePoamRelationship>, PoamError> {
    let history_page = relationship_page_bounds(history_limit, history_offset)?
        .unwrap_or((LEGACY_RELATIONSHIP_HISTORY_LIMIT, 0));
    let mut tx = pool.begin().await?;
    if !actor_can_access_systems_tx(&mut tx, actor, &[system_id]).await? {
        return Err(PoamError::NotFound);
    }
    let occurrences = sqlx::query_as::<_, CurrentCveOccurrence>(
        r#"SELECT system.id AS system_id,scan.id AS scan_id,
                   derivation.id AS scan_derivation_id,
                   scan.completed_at AS scan_completed_at,
                   retained.id AS generation_snapshot_id,
                   retained.generation,deployed.store_path AS target_store_path,
                   observation.canonical_cve_id,
                  observation.canonical_package_name,
                  observation.observed_package_name,
                  observation.observed_package_version,
                  observation.observed_derivation_path AS occurrence_derivation_path,
                  observation.is_whitelisted,
                  EXISTS(SELECT 1 FROM system_cve_justifications justification
                    WHERE justification.cve_id=observation.canonical_cve_id
                      AND (justification.system_id IS NULL
                        OR justification.system_id=system.id)) AS is_justified
           FROM systems system
           JOIN LATERAL (
             SELECT state.store_path,state.generation FROM system_states state
             WHERE state.hostname=system.hostname AND state.store_path IS NOT NULL
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
            AND artifact.lifecycle='available' AND artifact.integrity_version=1
           JOIN derivations derivation
             ON derivation.id=retained.derivation_id
            AND derivation.commit_id=retained.commit_id
            AND derivation.derivation_name=retained.configuration_name
            AND derivation.derivation_type='nixos'
            AND COALESCE(derivation.store_path,derivation.expected_store_path)=retained.source_store_path
           JOIN LATERAL (
             SELECT candidate.id,candidate.completed_at FROM cve_scans candidate
             WHERE candidate.derivation_id=derivation.id
               AND candidate.status='completed'
               AND candidate.evidence_schema_version=1
             ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
           ) scan ON true
           JOIN cve_scan_vulnerability_observations observation ON observation.scan_id=scan.id
           WHERE system.id=$1
           ORDER BY observation.canonical_cve_id,observation.canonical_package_name,
                     observation.observed_derivation_path
           LIMIT $2"#,
    )
    .bind(system_id)
    .bind(MAX_POAM_RELATIONSHIPS)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    hydrate_cve_relationships(pool, actor, system_id, occurrences, history_page, clock).await
}

/// Returns exact-CVE remediation context for the supplied vulnerability rows.
///
/// Each row key is resolved only against the latest completed schema-1 scan for
/// the system's exact retained deployment generation. Missing row keys produce
/// no relationship. Scan and derivation-path fields bind returned version
/// evidence to the row's exact occurrence; they do not extend the stable
/// system, CVE, and canonical-package finding identity. At most 1,000 row keys
/// are accepted in one request.
///
/// # Errors
///
/// Returns a validation error for invalid history bounds or an oversized batch,
/// a not-found error for an inaccessible system, or a database error when
/// authoritative evidence or relationship history cannot be loaded.
pub async fn cve_relationships_for_rows(
    pool: &PgPool,
    actor: &PoamActor,
    system_id: Uuid,
    row_keys: &[CveRelationshipRowKey],
    history_limit: Option<i64>,
    history_offset: Option<i64>,
    clock: &dyn PoamClock,
) -> Result<Vec<CvePoamRelationship>, PoamError> {
    let history_page = relationship_page_bounds(history_limit, history_offset)?
        .unwrap_or((LEGACY_RELATIONSHIP_HISTORY_LIMIT, 0));
    if row_keys.len() > MAX_CVE_RELATIONSHIP_ROWS {
        return Err(PoamError::Validation(
            "invalid_batch_size",
            "At most 1000 CVE vulnerability rows can be hydrated".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    if !actor_can_access_systems_tx(&mut tx, actor, &[system_id]).await? {
        return Err(PoamError::NotFound);
    }
    if row_keys.is_empty() {
        tx.commit().await?;
        return Ok(Vec::new());
    }
    let cve_ids = row_keys
        .iter()
        .map(|key| key.canonical_cve_id.as_str())
        .collect::<Vec<_>>();
    let package_names = row_keys
        .iter()
        .map(|key| key.canonical_package_name.as_str())
        .collect::<Vec<_>>();
    let scan_ids = row_keys.iter().map(|key| key.scan_id).collect::<Vec<_>>();
    let occurrence_derivation_paths = row_keys
        .iter()
        .map(|key| key.occurrence_derivation_path.as_str())
        .collect::<Vec<_>>();
    let occurrences = sqlx::query_as::<_, CurrentCveOccurrence>(
        r#"WITH requested AS (
             SELECT * FROM UNNEST($2::text[],$3::text[],$4::uuid[],$5::text[])
               WITH ORDINALITY AS row(canonical_cve_id,
                                      canonical_package_name,scan_id,
                                      occurrence_derivation_path,ordinal)
           )
            SELECT system.id AS system_id,scan.id AS scan_id,
                   derivation.id AS scan_derivation_id,
                   scan.completed_at AS scan_completed_at,
                   retained.id AS generation_snapshot_id,
                   retained.generation,deployed.store_path AS target_store_path,
                   observation.canonical_cve_id,
                  observation.canonical_package_name,
                  observation.observed_package_name,
                  observation.observed_package_version,
                  observation.observed_derivation_path AS occurrence_derivation_path,
                  observation.is_whitelisted,
                  EXISTS(SELECT 1 FROM system_cve_justifications justification
                    WHERE justification.cve_id=observation.canonical_cve_id
                      AND (justification.system_id IS NULL
                        OR justification.system_id=system.id)) AS is_justified
           FROM requested
           JOIN systems system ON system.id=$1
           JOIN LATERAL (
             SELECT state.store_path,state.generation FROM system_states state
             WHERE state.hostname=system.hostname AND state.store_path IS NOT NULL
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
            AND artifact.lifecycle='available' AND artifact.integrity_version=1
           JOIN derivations derivation
             ON derivation.id=retained.derivation_id
            AND derivation.commit_id=retained.commit_id
            AND derivation.derivation_name=retained.configuration_name
            AND derivation.derivation_type='nixos'
            AND COALESCE(derivation.store_path,derivation.expected_store_path)=retained.source_store_path
           JOIN LATERAL (
              SELECT candidate.id,candidate.completed_at FROM cve_scans candidate
             WHERE candidate.derivation_id=derivation.id
               AND candidate.status='completed'
               AND candidate.evidence_schema_version=1
             ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
            ) scan ON true
            JOIN cve_scan_vulnerability_observations observation
              ON observation.scan_id=scan.id
             AND observation.scan_id=requested.scan_id
             AND observation.canonical_cve_id=requested.canonical_cve_id
             AND observation.canonical_package_name=requested.canonical_package_name
             AND observation.observed_derivation_path
                 =requested.occurrence_derivation_path
             AND NOT observation.is_whitelisted
            ORDER BY requested.ordinal"#,
    )
    .bind(system_id)
    .bind(&cve_ids)
    .bind(&package_names)
    .bind(&scan_ids)
    .bind(&occurrence_derivation_paths)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    hydrate_cve_relationships(pool, actor, system_id, occurrences, history_page, clock).await
}

async fn hydrate_cve_relationships(
    pool: &PgPool,
    actor: &PoamActor,
    system_id: Uuid,
    occurrences: Vec<CurrentCveOccurrence>,
    history_page: (i64, i64),
    clock: &dyn PoamClock,
) -> Result<Vec<CvePoamRelationship>, PoamError> {
    let cve_ids = occurrences
        .iter()
        .map(|occurrence| occurrence.canonical_cve_id.as_str())
        .collect::<Vec<_>>();
    let package_names = occurrences
        .iter()
        .map(|occurrence| occurrence.canonical_package_name.as_str())
        .collect::<Vec<_>>();
    let finding_rows = sqlx::query_as::<_, (Uuid, String, String)>(
        r#"SELECT DISTINCT finding.id,finding.canonical_cve_id,
                           finding.canonical_package_name
           FROM UNNEST($2::text[],$3::text[])
             AS requested(canonical_cve_id,canonical_package_name)
           JOIN poam_cve_findings finding
             ON finding.system_id=$1
            AND finding.canonical_cve_id=requested.canonical_cve_id
            AND finding.canonical_package_name=requested.canonical_package_name"#,
    )
    .bind(system_id)
    .bind(&cve_ids)
    .bind(&package_names)
    .fetch_all(pool)
    .await?;
    let finding_ids = finding_rows.iter().map(|row| row.0).collect::<Vec<_>>();
    let summaries = poam::cve_finding_poam_summaries(
        pool,
        &finding_ids,
        clock.today(),
        actor.is_admin,
        &actor.environment_ids,
        history_page,
    )
    .await?;
    Ok(occurrences
        .into_iter()
        .map(|occurrence| {
            let finding_id = finding_rows
                .iter()
                .find(|row| {
                    row.1 == occurrence.canonical_cve_id
                        && row.2 == occurrence.canonical_package_name
                })
                .map(|row| row.0);
            let active_poam = finding_id.and_then(|finding_id| {
                summaries
                    .iter()
                    .find(|(id, active, _)| *id == finding_id && *active)
                    .map(|(_, _, summary)| summary.clone())
            });
            let active_id = active_poam.as_ref().map(|summary| summary.id);
            let mut seen = BTreeSet::new();
            let mut historical_poams = finding_id
                .into_iter()
                .flat_map(|finding_id| {
                    summaries.iter().filter(move |(id, active, summary)| {
                        *id == finding_id && !*active && Some(summary.id) != active_id
                    })
                })
                .filter(|(_, _, summary)| seen.insert(summary.id))
                .map(|(_, _, summary)| summary.clone())
                .collect::<Vec<_>>();
            let historical_has_more = historical_poams.len() as i64 > history_page.0;
            historical_poams.truncate(history_page.0 as usize);
            CvePoamRelationship {
                cve_finding_id: finding_id,
                observation: occurrence.reference(),
                observed_package_name: occurrence.observed_package_name,
                observed_package_version: occurrence.observed_package_version,
                is_whitelisted: occurrence.is_whitelisted,
                is_justified: occurrence.is_justified,
                active_poam,
                historical_poams,
                historical_has_more,
                historical_next_offset: historical_has_more
                    .then_some(history_page.1 + history_page.0),
            }
        })
        .collect())
}

/// Returns visible active POA&Ms compatible with one authoritative finding.
///
/// The server resolves `observation` against current deployed evidence before
/// it searches by the finding's policy lineage. Inaccessible findings return
/// [`PoamError::NotFound`] without revealing observation state.
///
/// # Errors
///
/// Returns a validation error for invalid bounds or search text, a precondition
/// error for stale or non-failing evidence, and a database error for query
/// failures.
pub async fn compatible_for_finding(
    pool: &PgPool,
    actor: &PoamActor,
    finding_id: Uuid,
    observation: &FindingObservationReference,
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
    let context =
        finding_action_context_tx(&mut tx, actor, None, Some(finding_id), Some(observation))
            .await?;
    if !actor_can_access_systems_tx(&mut tx, actor, &[context.system_id]).await? {
        return Err(PoamError::NotFound);
    }
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

/// Returns visible active POA&Ms compatible with one current assessment.
///
/// The assessment must be current, visible, and failing. Results share its
/// policy lineage and exclude POA&Ms already linked to the finding.
///
/// # Errors
///
/// Returns a validation error for invalid bounds, a not-found error for hidden
/// assessments, a precondition error for stale evidence, or a database error.
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

/// Returns visible POA&M relationships for immutable assignment versions.
///
/// When both pagination arguments are absent, the response uses the bounded
/// compatibility page and reports truncation through `poams_has_more` and
/// `poams_next_offset`. Supplying `history_limit` selects an explicit
/// per-assignment page; `history_offset` is invalid without a limit.
///
/// # Errors
///
/// Returns [`PoamError::Validation`] for invalid IDs or pagination and
/// [`PoamError::Database`] when a persistence operation fails.
pub async fn assignment_relationships(
    pool: &PgPool,
    actor: &PoamActor,
    assignment_version_ids: &[Uuid],
    history_limit: Option<i64>,
    history_offset: Option<i64>,
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
    let history_page = relationship_page_bounds(history_limit, history_offset)?;
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
        history_page,
    )
    .await?;
    Ok(visible_ids
        .into_iter()
        .map(|assignment_version_id| {
            let mut related = summaries
                .iter()
                .filter(|(related_id, _)| *related_id == assignment_version_id)
                .map(|(_, summary)| summary.clone())
                .collect::<Vec<_>>();
            let poams_has_more =
                history_page.is_some_and(|(history_limit, _)| related.len() as i64 > history_limit);
            if let Some((history_limit, _)) = history_page {
                related.truncate(history_limit as usize);
            }
            AssignmentPoamRelationship {
                assignment_version_id,
                poams: related,
                poams_has_more,
                poams_next_offset: history_page.and_then(|(history_limit, history_offset)| {
                    poams_has_more.then_some(history_offset + history_limit)
                }),
            }
        })
        .collect())
}

/// Creates a pending waiver request for one current failing finding.
///
/// The stored observation snapshot and token bind later decisions to the exact
/// evidence validated during creation.
///
/// # Errors
///
/// Returns an authorization, validation, precondition, not-found, conflict, or
/// database error when the waiver request cannot be created.
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
    let mut tx = pool.begin().await?;
    let legacy_finding_id = request
        .assessment_id
        .is_none()
        .then_some(request.finding_id);
    let key = finding_action_key_tx(
        &mut tx,
        actor,
        request.assessment_id,
        legacy_finding_id,
        request.observation.as_ref(),
    )
    .await?;
    lock_assessment_finding_key_tx(&mut tx, key).await?;
    let context = finding_action_context_tx(
        &mut tx,
        actor,
        request.assessment_id,
        legacy_finding_id,
        request.observation.as_ref(),
    )
    .await?;
    if context.finding_id != request.finding_id {
        return Err(PoamError::NotFound);
    }
    if !actor_can_access_systems_tx(&mut tx, actor, &[context.system_id]).await? {
        return Err(PoamError::NotFound);
    }
    if context.overall_outcome != "fail" {
        return Err(PoamError::Precondition(
            "finding_not_failed",
            "A waiver can only be requested for a current Fail finding".into(),
            None,
        ));
    }
    let (policy_version_id, observation_snapshot, observation_token) = if let Some(assessment_id) =
        request.assessment_id
    {
        let observation_snapshot = observation_snapshot_tx(&mut tx, assessment_id)
            .await?
            .ok_or(PoamError::NotFound)?;
        let observation_token = semantic_digest(&observation_snapshot);
        (
            context.policy_version_id,
            observation_snapshot,
            observation_token,
        )
    } else {
        let items = current_verification_items_tx(
            &mut tx,
            &[(
                context.finding_id,
                context.system_id,
                context.policy_lineage_id,
            )],
            Utc::now(),
        )
        .await?;
        let item = items.into_iter().next().ok_or(PoamError::NotFound)?;
        if item.observed_outcome.as_deref() != Some("fail")
            || item.assessment_id.is_some()
            || item.policy_version_id != Some(context.policy_version_id)
        {
            return Err(PoamError::Precondition(
                "finding_not_failed",
                "A waiver can only be requested for an exact current legacy Fail finding".into(),
                None,
            ));
        }
        (
            context.policy_version_id,
            item.observation_snapshot.ok_or_else(|| {
                PoamError::Precondition(
                    "stale_finding",
                    "Legacy finding evidence is incomplete".into(),
                    None,
                )
            })?,
            item.observation_token.ok_or_else(|| {
                PoamError::Precondition(
                    "stale_finding",
                    "Legacy finding evidence is incomplete".into(),
                    None,
                )
            })?,
        )
    };
    let waiver_id:Uuid=sqlx::query_scalar("INSERT INTO finding_waivers(finding_id,justification,policy_version_id,assessment_id,observation_token,observation_snapshot,created_by) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
      .bind(request.finding_id).bind(request.justification.trim()).bind(policy_version_id).bind(request.assessment_id).bind(&observation_token).bind(&observation_snapshot).bind(actor.user_id).fetch_one(&mut *tx).await?;
    let payload = json!({"waiver_id":waiver_id,"finding_id":request.finding_id,"assessment_id":request.assessment_id,"status":"pending"});
    sqlx::query("INSERT INTO finding_waiver_events(waiver_id,actor_user_id,to_status,payload) VALUES($1,$2,'pending',$3)")
      .bind(waiver_id).bind(actor.user_id).bind(&payload).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO admin_audit_events(actor_user_id,actor_identifier,action,target,request_origin,metadata) VALUES($1,$2,'finding_waiver_created',$3,$4,$5)")
      .bind(actor.user_id).bind(&actor.identifier).bind(format!("finding:{}",request.finding_id)).bind(actor.request_origin.as_deref()).bind(&payload).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(payload)
}

/// Lists waiver records for an administrator.
///
/// # Errors
///
/// Returns [`PoamError::Forbidden`] for non-admin actors. It returns a
/// validation error for invalid filters or a database error on load failure.
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

/// Returns one waiver record to an administrator.
///
/// # Errors
///
/// Returns [`PoamError::Forbidden`] for non-admin actors,
/// [`PoamError::NotFound`] for an absent waiver, or a database error.
pub async fn waiver(pool: &PgPool, actor: &PoamActor, id: Uuid) -> Result<WaiverView, PoamError> {
    if !actor.is_admin {
        return Err(PoamError::Forbidden);
    }
    poam::waiver(pool, id).await?.ok_or(PoamError::NotFound)
}

/// Applies an administrator decision to a waiver.
///
/// The transition is validated against the waiver lifecycle and current
/// evidence while holding the finding lock.
///
/// # Errors
///
/// Returns an authorization, validation, precondition, not-found, conflict, or
/// database error when the decision cannot be applied.
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
    lock_assessment_finding_key_tx(&mut tx, key).await?;
    let row=sqlx::query_as::<_,(Uuid,String,Uuid,Option<Uuid>,Uuid,String,Value)>("SELECT w.finding_id,w.status,f.system_id,w.assessment_id,w.policy_version_id,w.observation_token,w.observation_snapshot FROM finding_waivers w JOIN poam_findings f ON f.id=w.finding_id WHERE w.id=$1 FOR UPDATE OF w")
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
        if let Some(assessment_id) = row.3 {
            let context = assessment_context_tx(&mut tx, assessment_id)
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
            if observation_snapshot_tx(&mut tx, assessment_id)
                .await?
                .map(|snapshot| semantic_digest(&snapshot))
                .as_deref()
                != Some(row.5.as_str())
            {
                return Err(PoamError::Precondition(
                    "waiver_observation_changed",
                    "The exact Fail observation changed after waiver submission".into(),
                    None,
                ));
            }
        } else {
            let finding = sqlx::query_as::<_, (Uuid, Uuid)>(
                "SELECT system_id,policy_lineage_id FROM poam_findings WHERE id=$1",
            )
            .bind(row.0)
            .fetch_one(&mut *tx)
            .await?;
            let item = current_verification_items_tx(
                &mut tx,
                &[(row.0, finding.0, finding.1)],
                clock.now(),
            )
            .await?
            .into_iter()
            .next()
            .ok_or(PoamError::NotFound)?;
            if item.assessment_id.is_some()
                || item.observed_outcome.as_deref() != Some("fail")
                || item.policy_version_id != Some(row.4)
                || item.observation_token.as_deref() != Some(row.5.as_str())
                || item.observation_snapshot.as_ref() != Some(&row.6)
            {
                return Err(PoamError::Precondition(
                    "waiver_observation_changed",
                    "The exact legacy Fail observation changed after waiver submission".into(),
                    None,
                ));
            }
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

#[derive(Debug, Clone)]
struct CveVerificationItem {
    cve_finding_id: Uuid,
    system_id: Uuid,
    canonical_cve_id: String,
    canonical_package_name: String,
    baseline: Option<CveLinkBaseline>,
    result: String,
    scan_id: Option<Uuid>,
    scan_derivation_id: Option<i32>,
    scan_completed_at: Option<DateTime<Utc>>,
    generation_snapshot_id: Option<Uuid>,
    generation: Option<i32>,
    target_store_path: Option<String>,
    occurrence_present: bool,
    occurrence_derivation_path: Option<String>,
    observed_package_version: Option<String>,
    detail: String,
}

async fn current_cve_verification_items_tx(
    tx: &mut Transaction<'_, Postgres>,
    findings: &[CveFindingKey],
    poam_id: Option<Uuid>,
) -> Result<Vec<CveVerificationItem>, PoamError> {
    let mut items = Vec::with_capacity(findings.len());
    for finding in findings {
        let baseline = if let Some(poam_id) = poam_id {
            let resolved =
                sqlx::query_as::<_, (Uuid, i32, DateTime<Utc>, Uuid, i32, String, String, String)>(
                    r#"SELECT baseline_scan_id,baseline_scan_derivation_id,
                          baseline_scan_completed_at,baseline_generation_snapshot_id,
                          baseline_generation,baseline_target_store_path,
                          baseline_occurrence_derivation_path,
                          baseline_observed_package_version
                   FROM poam_cve_finding_links
                   WHERE poam_id=$1 AND cve_finding_id=$2 AND retired_at IS NULL"#,
                )
                .bind(poam_id)
                .bind(finding.id)
                .fetch_optional(&mut **tx)
                .await?
                .map(|row| CveLinkBaseline {
                    system_id: finding.system_id,
                    scan_id: row.0,
                    scan_derivation_id: row.1,
                    scan_completed_at: row.2,
                    generation_snapshot_id: row.3,
                    generation: row.4,
                    target_store_path: row.5,
                    occurrence_derivation_path: row.6,
                    observed_package_version: row.7,
                });
            if resolved.is_none() {
                return Err(PoamError::Conflict(
                    "concurrent_finding_change",
                    "The active exact-CVE link changed during verification".into(),
                ));
            }
            resolved
        } else {
            None
        };
        let deployed: Option<(i32, String, Uuid, i32)> = sqlx::query_as(
            r#"SELECT derivation.id,deployed.store_path,retained.id,retained.generation
               FROM systems system
               JOIN LATERAL (
                 SELECT state.store_path,state.generation FROM system_states state
                 WHERE state.hostname=system.hostname AND state.store_path IS NOT NULL
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
                AND artifact.lifecycle='available' AND artifact.integrity_version=1
               JOIN derivations derivation
                 ON derivation.id=retained.derivation_id
                AND derivation.commit_id=retained.commit_id
                AND derivation.derivation_name=retained.configuration_name
                AND derivation.derivation_type='nixos'
                AND COALESCE(derivation.store_path,derivation.expected_store_path)=retained.source_store_path
               WHERE system.id=$1"#,
        )
        .bind(finding.system_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some((derivation_id, target_store_path, generation_snapshot_id, generation)) = deployed
        else {
            items.push(CveVerificationItem {
                cve_finding_id: finding.id,
                system_id: finding.system_id,
                canonical_cve_id: finding.canonical_cve_id.clone(),
                canonical_package_name: finding.canonical_package_name.clone(),
                baseline,
                result: "missing".into(),
                scan_id: None,
                scan_derivation_id: None,
                scan_completed_at: None,
                generation_snapshot_id: None,
                generation: None,
                target_store_path: None,
                occurrence_present: false,
                occurrence_derivation_path: None,
                observed_package_version: None,
                detail: "No exact deployed derivation can be resolved".into(),
            });
            continue;
        };
        if let Some(linked) = baseline.as_ref()
            && (linked.scan_derivation_id != derivation_id
                || linked.generation_snapshot_id != generation_snapshot_id
                || linked.generation != generation
                || linked.target_store_path != target_store_path)
        {
            items.push(CveVerificationItem {
                cve_finding_id: finding.id,
                system_id: finding.system_id,
                canonical_cve_id: finding.canonical_cve_id.clone(),
                canonical_package_name: finding.canonical_package_name.clone(),
                baseline,
                result: "missing".into(),
                scan_id: None,
                scan_derivation_id: None,
                scan_completed_at: None,
                generation_snapshot_id: None,
                generation: None,
                target_store_path: None,
                occurrence_present: false,
                occurrence_derivation_path: None,
                observed_package_version: None,
                detail: "The deployed generation no longer matches the immutable link baseline"
                    .into(),
            });
            continue;
        }
        let scan: Option<(Uuid, DateTime<Utc>)> = sqlx::query_as(
            r#"SELECT id,completed_at FROM cve_scans
               WHERE derivation_id=$1 AND status='completed'
                  AND evidence_schema_version=1
                  AND ($2::timestamptz IS NULL OR completed_at>$2)
               ORDER BY completed_at DESC,id DESC LIMIT 1"#,
        )
        .bind(derivation_id)
        .bind(baseline.as_ref().map(|value| value.scan_completed_at))
        .fetch_optional(&mut **tx)
        .await?;
        let Some((scan_id, scan_completed_at)) = scan else {
            items.push(CveVerificationItem {
                cve_finding_id: finding.id,
                system_id: finding.system_id,
                canonical_cve_id: finding.canonical_cve_id.clone(),
                canonical_package_name: finding.canonical_package_name.clone(),
                baseline,
                result: "missing".into(),
                scan_id: None,
                scan_derivation_id: None,
                scan_completed_at: None,
                generation_snapshot_id: None,
                generation: None,
                target_store_path: None,
                occurrence_present: false,
                occurrence_derivation_path: None,
                observed_package_version: None,
                detail: if poam_id.is_some() {
                    "No completed schema-1 scan is strictly newer than the immutable link baseline"
                        .into()
                } else {
                    "No completed schema-1 scan exists for the exact deployed derivation".into()
                },
            });
            continue;
        };
        let occurrence: Option<(String, String, bool)> = sqlx::query_as(
            r#"SELECT observed_derivation_path,observed_package_version,is_whitelisted
               FROM cve_scan_vulnerability_observations
               WHERE scan_id=$1 AND canonical_cve_id=$2
                  AND canonical_package_name=$3
               ORDER BY observed_derivation_path LIMIT 1"#,
        )
        .bind(scan_id)
        .bind(&finding.canonical_cve_id)
        .bind(&finding.canonical_package_name)
        .fetch_optional(&mut **tx)
        .await?;
        let justified: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM system_cve_justifications
               WHERE cve_id=$1 AND (system_id IS NULL OR system_id=$2))"#,
        )
        .bind(&finding.canonical_cve_id)
        .bind(finding.system_id)
        .fetch_one(&mut **tx)
        .await?;
        let (result, detail) = match (&occurrence, justified) {
            (None, _) => (
                "pass",
                "Exact occurrence is absent from current sealed evidence",
            ),
            (Some((_, _, true)), _) => (
                "whitelisted",
                "Exact occurrence remains present but is scanner-whitelisted",
            ),
            (Some(_), true) => (
                "justified",
                "Exact occurrence remains present with an applicable justification",
            ),
            (Some(_), false) => ("fail", "Exact unwhitelisted occurrence remains present"),
        };
        items.push(CveVerificationItem {
            cve_finding_id: finding.id,
            system_id: finding.system_id,
            canonical_cve_id: finding.canonical_cve_id.clone(),
            canonical_package_name: finding.canonical_package_name.clone(),
            baseline,
            result: result.into(),
            scan_id: Some(scan_id),
            scan_derivation_id: Some(derivation_id),
            scan_completed_at: Some(scan_completed_at),
            generation_snapshot_id: Some(generation_snapshot_id),
            generation: Some(generation),
            target_store_path: Some(target_store_path),
            occurrence_present: occurrence.is_some(),
            occurrence_derivation_path: occurrence.as_ref().map(|row| row.0.clone()),
            observed_package_version: occurrence.as_ref().map(|row| row.1.clone()),
            detail: detail.into(),
        });
    }
    Ok(items)
}

async fn insert_cve_verification_items(
    tx: &mut Transaction<'_, Postgres>,
    attempt_id: Uuid,
    items: &[CveVerificationItem],
    now: DateTime<Utc>,
) -> Result<(), PoamError> {
    if items.is_empty() {
        return Ok(());
    }
    let mut builder = sqlx::QueryBuilder::<Postgres>::new(
        "INSERT INTO poam_cve_verification_items(attempt_id,cve_finding_id,system_id,canonical_cve_id,canonical_package_name,baseline_scan_id,baseline_scan_derivation_id,baseline_scan_completed_at,baseline_generation_snapshot_id,baseline_generation,baseline_target_store_path,baseline_occurrence_derivation_path,baseline_observed_package_version,result,scan_id,scan_derivation_id,scan_completed_at,generation_snapshot_id,generation,target_store_path,occurrence_present,occurrence_derivation_path,observed_package_version,detail,observed_at) ",
    );
    builder.push_values(items, |mut row, item| {
        row.push_bind(attempt_id)
            .push_bind(item.cve_finding_id)
            .push_bind(item.system_id)
            .push_bind(&item.canonical_cve_id)
            .push_bind(&item.canonical_package_name)
            .push_bind(item.baseline.as_ref().map(|value| value.scan_id))
            .push_bind(item.baseline.as_ref().map(|value| value.scan_derivation_id))
            .push_bind(item.baseline.as_ref().map(|value| value.scan_completed_at))
            .push_bind(
                item.baseline
                    .as_ref()
                    .map(|value| value.generation_snapshot_id),
            )
            .push_bind(item.baseline.as_ref().map(|value| value.generation))
            .push_bind(item.baseline.as_ref().map(|value| &value.target_store_path))
            .push_bind(
                item.baseline
                    .as_ref()
                    .map(|value| &value.occurrence_derivation_path),
            )
            .push_bind(
                item.baseline
                    .as_ref()
                    .map(|value| &value.observed_package_version),
            )
            .push_bind(&item.result)
            .push_bind(item.scan_id)
            .push_bind(item.scan_derivation_id)
            .push_bind(item.scan_completed_at)
            .push_bind(item.generation_snapshot_id)
            .push_bind(item.generation)
            .push_bind(&item.target_store_path)
            .push_bind(item.occurrence_present)
            .push_bind(&item.occurrence_derivation_path)
            .push_bind(&item.observed_package_version)
            .push_bind(&item.detail)
            .push_bind(now);
    });
    if !items.is_empty() {
        builder.build().execute(&mut **tx).await?;
    }
    Ok(())
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

impl AssessmentObservation {
    fn identity(&self) -> PersistedAssessmentIdentity {
        PersistedAssessmentIdentity {
            id: self.assessment_id,
            policy_lineage_id: self.policy_lineage_id,
            policy_version_id: self.policy_version_id,
            effective_set_digest: self.effective_set_digest.clone(),
            effective_config_digest: self.effective_config_digest.clone(),
            effective_config: self.effective_config.clone(),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct LegacyDeployedEvidence {
    system_id: Uuid,
    derivation_id: i32,
    target_store_path: String,
    policy_results: Value,
    observed_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct LegacyCveEvidence {
    derivation_id: i32,
    scan_id: Uuid,
    critical_count: i32,
    high_count: i32,
}

#[derive(Debug)]
struct LegacyObservation {
    derivation_id: i32,
    target_store_path: String,
    effective_set_digest: String,
    effective_config_digest: String,
    effective_config: Value,
    observed_outcome: String,
    observation_token: String,
    observation_snapshot: Value,
    observed_at: DateTime<Utc>,
}

fn policy_bundle_context(policy: &EffectivePolicy) -> (Vec<Uuid>, Vec<Uuid>) {
    let bundle_ids = policy
        .provenance
        .iter()
        .filter(|entry| entry.authoritative)
        .filter_map(|entry| entry.bundle_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let bundle_version_ids = policy
        .provenance
        .iter()
        .filter(|entry| entry.authoritative)
        .filter_map(|entry| entry.bundle_version_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    (bundle_ids, bundle_version_ids)
}

async fn current_legacy_observations_tx(
    tx: &mut Transaction<'_, Postgres>,
    findings: &[(Uuid, Uuid, Uuid)],
    resolved_by_system: &HashMap<Uuid, ResolutionOutcome>,
) -> Result<HashMap<(Uuid, Uuid), LegacyObservation>, PoamError> {
    let system_ids = findings.iter().map(|row| row.1).collect::<Vec<_>>();
    let deployed = sqlx::query_as::<_, LegacyDeployedEvidence>(
        r#"SELECT requested.system_id,derivation.id AS derivation_id,
             state.store_path AS target_store_path,derivation.policy_results,
             state.timestamp AS observed_at
           FROM (SELECT DISTINCT unnest($1::uuid[]) AS system_id) requested
           JOIN systems system ON system.id=requested.system_id
           JOIN LATERAL (
             SELECT state.store_path,state.timestamp FROM system_states state
             WHERE state.hostname=system.hostname AND state.store_path IS NOT NULL
               AND btrim(state.store_path)<>''
             ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
           ) state ON true
           JOIN LATERAL (
             SELECT derivation.id,derivation.policy_results
             FROM derivations derivation
             WHERE COALESCE(derivation.store_path,derivation.expected_store_path)=state.store_path
               AND derivation.derivation_type='nixos'
             ORDER BY derivation.completed_at DESC NULLS LAST,derivation.id DESC LIMIT 1
           ) derivation ON true"#,
    )
    .bind(&system_ids)
    .fetch_all(&mut **tx)
    .await?;
    let derivation_ids = deployed
        .iter()
        .map(|evidence| evidence.derivation_id)
        .collect::<Vec<_>>();
    let scans = sqlx::query_as::<_, LegacyCveEvidence>(
        r#"SELECT DISTINCT ON (derivation_id) derivation_id,id AS scan_id,
             critical_count,high_count
           FROM cve_scans WHERE derivation_id=ANY($1) AND status='completed'
           ORDER BY derivation_id,completed_at DESC NULLS LAST,id DESC"#,
    )
    .bind(&derivation_ids)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|scan| (scan.derivation_id, scan))
    .collect::<HashMap<_, _>>();
    let deployed = deployed
        .into_iter()
        .map(|evidence| (evidence.system_id, evidence))
        .collect::<HashMap<_, _>>();

    let mut observations = HashMap::new();
    for (_, system_id, lineage_id) in findings {
        let Some(ResolutionOutcome::Resolved(resolved)) = resolved_by_system.get(system_id) else {
            continue;
        };
        let Some(policy) = resolved
            .policies
            .iter()
            .find(|policy| policy.policy_lineage_id == *lineage_id)
        else {
            continue;
        };
        let Some(evidence) = deployed.get(system_id) else {
            continue;
        };
        let effective_config_digest = semantic_digest(&policy.effective_config);
        let (reference, snapshot, passed) = match policy.policy_type.as_str() {
            "require_packages" | "custom_check" | "require_cf_agent" => {
                let Some((passed, details)) = nix_policy_result(
                    &evidence.policy_results,
                    Some(policy.policy_version_id),
                    *lineage_id,
                )
                .ok()
                .flatten() else {
                    continue;
                };
                let snapshot = json!({
                    "source": "nix_policy_result",
                    "system_id": system_id,
                    "policy_lineage_id": lineage_id,
                    "policy_version_id": policy.policy_version_id,
                    "effective_set_digest": resolved.effective_set_digest,
                    "effective_config_digest": effective_config_digest,
                    "derivation_id": evidence.derivation_id,
                    "target_store_path": evidence.target_store_path,
                    "passed": passed,
                    "details": details,
                });
                (
                    nix_policy_observation_reference(
                        *system_id,
                        *lineage_id,
                        policy.policy_version_id,
                        &resolved.effective_set_digest,
                        &effective_config_digest,
                        evidence.derivation_id,
                        &evidence.target_store_path,
                        passed,
                        details.as_deref(),
                    ),
                    snapshot,
                    passed,
                )
            }
            "require_cve_check" => {
                let Some(scan) = scans.get(&evidence.derivation_id) else {
                    continue;
                };
                let max_critical = policy
                    .effective_config
                    .get("max_critical")
                    .and_then(Value::as_i64)
                    .unwrap_or(i64::MAX);
                let max_high = policy
                    .effective_config
                    .get("max_high")
                    .and_then(Value::as_i64);
                let passed = i64::from(scan.critical_count) <= max_critical
                    && max_high.is_none_or(|max| i64::from(scan.high_count) <= max);
                let values = CveObservationValues {
                    system_id: *system_id,
                    policy_lineage_id: *lineage_id,
                    policy_version_id: policy.policy_version_id,
                    effective_set_digest: &resolved.effective_set_digest,
                    effective_config_digest: &effective_config_digest,
                    derivation_id: evidence.derivation_id,
                    target_store_path: &evidence.target_store_path,
                    scan_id: scan.scan_id,
                    critical_count: scan.critical_count,
                    high_count: scan.high_count,
                    max_critical,
                    max_high,
                };
                let snapshot = json!({
                    "source": "cve_scan",
                    "system_id": system_id,
                    "policy_lineage_id": lineage_id,
                    "policy_version_id": policy.policy_version_id,
                    "effective_set_digest": resolved.effective_set_digest,
                    "effective_config_digest": effective_config_digest,
                    "derivation_id": evidence.derivation_id,
                    "target_store_path": evidence.target_store_path,
                    "scan_id": scan.scan_id,
                    "critical_count": scan.critical_count,
                    "high_count": scan.high_count,
                    "max_critical": max_critical,
                    "max_high": max_high,
                });
                (cve_observation_reference(values), snapshot, passed)
            }
            _ => continue,
        };
        debug_assert_eq!(reference.token, semantic_digest(&snapshot));
        observations.insert(
            (*system_id, *lineage_id),
            LegacyObservation {
                derivation_id: evidence.derivation_id,
                target_store_path: evidence.target_store_path.clone(),
                effective_set_digest: resolved.effective_set_digest.clone(),
                effective_config_digest,
                effective_config: policy.effective_config.clone(),
                observed_outcome: if passed { "pass" } else { "fail" }.into(),
                observation_token: reference.token,
                observation_snapshot: snapshot,
                observed_at: evidence.observed_at,
            },
        );
    }
    Ok(observations)
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
    let legacy_observations =
        current_legacy_observations_tx(tx, findings, &resolved_by_system).await?;
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
           JOIN (SELECT DISTINCT unnest($1::uuid[]) AS system_id) requested
             ON requested.system_id=a.system_id
           JOIN systems s ON s.id=a.system_id
           JOIN LATERAL (
             SELECT ss.store_path FROM system_states ss
             WHERE ss.hostname=s.hostname AND ss.store_path IS NOT NULL AND btrim(ss.store_path)<>''
             ORDER BY ss.timestamp DESC,ss.id DESC LIMIT 1
           ) deployed ON deployed.store_path=a.target_store_path
           ORDER BY a.system_id,a.policy_lineage_id,a.updated_at DESC,a.id DESC"#,
    )
    .bind(&system_ids)
    .fetch_all(&mut **tx)
    .await?;
    let observation_ids = observations
        .iter()
        .map(|observation| observation.assessment_id)
        .collect::<Vec<_>>();
    let assessment_rules = sqlx::query_as::<_, PersistedRuleIdentity>(
        r#"SELECT assessment_id,rule_id,ordinal,kind,phase,outcome,blocking
           FROM composite_policy_rule_results
           WHERE assessment_id=ANY($1)
           ORDER BY assessment_id,ordinal"#,
    )
    .bind(&observation_ids)
    .fetch_all(&mut **tx)
    .await?;
    let finding_ids = findings.iter().map(|row| row.0).collect::<Vec<_>>();
    let waiver_rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, Uuid, Uuid, String)>(
            r#"SELECT finding_id,assessment_id,policy_version_id,id,observation_token FROM finding_waivers
               WHERE finding_id=ANY($1) AND status='accepted'
                  AND (expires_at IS NULL OR expires_at>$2)
               ORDER BY finding_id,assessment_id NULLS FIRST,accepted_at DESC FOR SHARE"#,
        )
        .bind(&finding_ids)
        .bind(now)
        .fetch_all(&mut **tx)
        .await?;
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
        let composite_authorization_digest = enforce_composite_authorization_digest(resolved);
        let compatible_ids = if policy.policy_type == "composite" {
            let policies = policy_contexts(resolved)?;
            let mut seen_targets = BTreeSet::new();
            let mut compatible = None;
            for candidate in observations
                .iter()
                .filter(|observation| observation.system_id == *system_id)
            {
                let target = (
                    candidate.derivation_id,
                    candidate.target_store_path.as_str(),
                );
                if !seen_targets.insert(target) {
                    continue;
                }
                let identities = observations
                    .iter()
                    .filter(|observation| {
                        observation.system_id == *system_id
                            && observation.derivation_id == candidate.derivation_id
                            && observation.target_store_path == candidate.target_store_path
                    })
                    .map(AssessmentObservation::identity)
                    .collect::<Vec<_>>();
                if let Some(selected) = select_compatible_assessment_set(
                    &policies,
                    &composite_authorization_digest,
                    &identities,
                    &assessment_rules,
                ) {
                    compatible = Some(selected.ids().to_vec());
                    break;
                }
            }
            compatible
        } else {
            None
        };
        let exact_observation = current_observations.clone().find(|observation| {
            compatible_ids
                .as_ref()
                .is_some_and(|ids| ids.contains(&observation.assessment_id))
        });
        let Some(observation) = exact_observation.or_else(|| current_observations.clone().next())
        else {
            if let Some(observation) = legacy_observations.get(&(*system_id, *lineage_id)) {
                let (bundle_ids, bundle_version_ids) = policy_bundle_context(policy);
                let waiver_id = waivers
                    .get(&(
                        *finding_id,
                        None,
                        policy.policy_version_id,
                        observation.observation_token.clone(),
                    ))
                    .copied();
                items.push(VerificationItem {
                    finding_id: *finding_id,
                    system_id: *system_id,
                    policy_lineage_id: *lineage_id,
                    result: waiver_id
                        .map(|_| "waiver".to_string())
                        .unwrap_or_else(|| observation.observed_outcome.clone()),
                    policy_version_id: Some(policy.policy_version_id),
                    assessment_id: None,
                    derivation_id: Some(observation.derivation_id),
                    target_store_path: Some(observation.target_store_path.clone()),
                    effective_set_digest: Some(observation.effective_set_digest.clone()),
                    effective_config_digest: Some(observation.effective_config_digest.clone()),
                    effective_config: Some(observation.effective_config.clone()),
                    observed_outcome: Some(observation.observed_outcome.clone()),
                    observation_token: Some(observation.observation_token.clone()),
                    observation_snapshot: Some(observation.observation_snapshot.clone()),
                    assessment_updated_at: Some(observation.observed_at),
                    bundle_ids,
                    bundle_version_ids,
                    requirement_version_ids: requirements_by_policy
                        .get(&policy.policy_version_id)
                        .cloned()
                        .unwrap_or_default(),
                    waiver_id,
                    detail: if waiver_id.is_some() {
                        "Exact current legacy finding has an accepted waiver".into()
                    } else {
                        format!(
                            "Exact current legacy observation {}",
                            observation.observed_outcome
                        )
                    },
                });
                continue;
            }
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
        let exact = compatible_ids
            .as_ref()
            .is_some_and(|ids| ids.contains(&observation.assessment_id));
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
                    Some(observation.assessment_id),
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
    if items.is_empty() {
        return Ok(());
    }
    // INVARIANT: Verification already holds each finding advisory lock. System
    // hostname updates take the same locks, so this read and the item insert
    // capture one transactional identity before the attempt is sealed.
    let system_ids = items
        .iter()
        .map(|item| item.system_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let system_hostnames =
        sqlx::query_as::<_, (Uuid, String)>("SELECT id,hostname FROM systems WHERE id=ANY($1)")
            .bind(&system_ids)
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .collect::<HashMap<_, _>>();
    let item_hostnames = items
        .iter()
        .map(|item| {
            system_hostnames.get(&item.system_id).ok_or_else(|| {
                PoamError::Database(anyhow::anyhow!(
                    "verification system {} disappeared while its finding was locked",
                    item.system_id
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // SECURITY: The server is the sole supported persistence writer. SQL cannot
    // safely duplicate resolver precedence, so the server persists resolver
    // output outside the POA&M DML surface and the trigger requires an exact
    // attempt/finding-bound match. This rejects malformed or accidental writes
    // in the server transaction. The database owner and superusers are trusted;
    // they can disable the trigger and are outside this boundary.
    let mut attestations = HashMap::new();
    for item in items.iter().filter(|item| {
        item.assessment_id.is_none() && matches!(item.result.as_str(), "pass" | "waiver")
    }) {
        sqlx::query(
            r#"INSERT INTO compliance_resolved_effective_contexts(
                 attempt_id,finding_id,system_id,policy_lineage_id,policy_version_id,
                 derivation_id,target_store_path,effective_set_digest,
                 effective_config_digest,effective_config,observed_outcome,
                 observation_token,observation_snapshot
               ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
        )
        .bind(attempt_id)
        .bind(item.finding_id)
        .bind(item.system_id)
        .bind(item.policy_lineage_id)
        .bind(item.policy_version_id)
        .bind(item.derivation_id)
        .bind(&item.target_store_path)
        .bind(&item.effective_set_digest)
        .bind(&item.effective_config_digest)
        .bind(&item.effective_config)
        .bind(&item.observed_outcome)
        .bind(&item.observation_token)
        .bind(&item.observation_snapshot)
        .execute(&mut **tx)
        .await?;
        let attestation_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO poam_effective_context_attestations(
                 attempt_id,finding_id,system_id,policy_lineage_id,policy_version_id,
                 derivation_id,target_store_path,effective_set_digest,
                 effective_config_digest,effective_config,observed_outcome,
                 observation_token,observation_snapshot
               ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
               RETURNING id"#,
        )
        .bind(attempt_id)
        .bind(item.finding_id)
        .bind(item.system_id)
        .bind(item.policy_lineage_id)
        .bind(item.policy_version_id)
        .bind(item.derivation_id)
        .bind(&item.target_store_path)
        .bind(&item.effective_set_digest)
        .bind(&item.effective_config_digest)
        .bind(&item.effective_config)
        .bind(&item.observed_outcome)
        .bind(&item.observation_token)
        .bind(&item.observation_snapshot)
        .fetch_one(&mut **tx)
        .await?;
        attestations.insert(item.finding_id, attestation_id);
    }
    let mut builder = sqlx::QueryBuilder::<Postgres>::new(
        "INSERT INTO poam_verification_items(attempt_id,finding_id,system_id,system_hostname,policy_lineage_id,result,policy_version_id,assessment_id,derivation_id,target_store_path,effective_set_digest,effective_config_digest,effective_config,observed_outcome,observation_token,observation_snapshot,assessment_updated_at,bundle_ids,bundle_version_ids,requirement_version_ids,waiver_id,observed_at,detail,effective_context_attestation_id) ",
    );
    builder.push_values(
        items.iter().zip(item_hostnames),
        |mut row, (item, hostname)| {
            row.push_bind(attempt_id)
                .push_bind(item.finding_id)
                .push_bind(item.system_id)
                .push_bind(hostname)
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
                .push_bind(&item.detail)
                .push_bind(attestations.get(&item.finding_id).copied());
        },
    );
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

/// Records a sealed verification attempt for an awaiting-verification POA&M.
///
/// Serialization and deadlock failures are retried twice. The response records
/// whether every current finding has acceptable exact evidence.
///
/// # Errors
///
/// Returns an authorization, validation, precondition, not-found, conflict, or
/// database error when verification cannot produce a sealed attempt.
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
    let expected_cve_findings = poam_cve_finding_keys(pool, id).await?;
    // Acquire the shared writer locks before reading authoritative state. Under
    // READ COMMITTED, statements after a wait see the writer that just committed.
    let mut tx = pool.begin().await?;
    lock_cve_keys_tx(&mut tx, &expected_cve_findings).await?;
    for system_id in expected_findings
        .iter()
        .map(|row| row.1)
        .chain(expected_cve_findings.iter().map(|row| row.system_id))
        .collect::<BTreeSet<_>>()
    {
        crate::services::composite_enforcement::lock_poam_system_key_tx(&mut tx, system_id).await?;
    }
    let affected_system_ids = expected_findings
        .iter()
        .map(|row| row.1)
        .chain(expected_cve_findings.iter().map(|row| row.system_id))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    lock_policy_finding_keys_for_systems_tx(&mut tx, &affected_system_ids).await?;
    lock_cve_finding_keys_tx(&mut tx, &expected_cve_findings).await?;
    let actor = current_mutating_actor_tx(&mut tx, actor).await?;
    let status = lock_mutable_poam(&mut tx, &actor, id, revision).await?;
    if status != "awaiting_verification" {
        return Err(PoamError::Conflict(
            "invalid_transition",
            "POA&M must be awaiting verification before verification".into(),
        ));
    }
    let findings=sqlx::query_as::<_,(Uuid,Uuid,Uuid)>(r#"SELECT f.id,f.system_id,f.policy_lineage_id FROM poam_finding_links l JOIN poam_findings f ON f.id=l.finding_id
      WHERE l.poam_id=$1 AND l.retired_at IS NULL ORDER BY f.system_id,f.policy_lineage_id FOR UPDATE OF l,f"#).bind(id).fetch_all(&mut *tx).await?;
    let cve_findings = sqlx::query_as::<_, CveFindingKey>(
        r#"SELECT f.id,f.system_id,f.canonical_cve_id,f.canonical_package_name
      FROM poam_cve_finding_links l JOIN poam_cve_findings f ON f.id=l.cve_finding_id
      WHERE l.poam_id=$1 AND l.retired_at IS NULL
      ORDER BY f.system_id,f.canonical_cve_id,f.canonical_package_name,f.id FOR UPDATE OF l,f"#,
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    if findings != expected_findings || cve_findings != expected_cve_findings {
        return Err(PoamError::Conflict(
            "concurrent_finding_change",
            "The active finding set changed; retry the request".into(),
        ));
    }
    if findings.is_empty() == cve_findings.is_empty() {
        return Err(PoamError::Validation(
            "finding_required",
            "POA&M must have exactly one active finding family".into(),
        ));
    }
    if !actor_can_access_systems_tx(
        &mut tx,
        &actor,
        &findings
            .iter()
            .map(|r| r.1)
            .chain(cve_findings.iter().map(|r| r.system_id))
            .collect::<Vec<_>>(),
    )
    .await?
    {
        return Err(PoamError::NotFound);
    }
    let now = clock.now();
    let items = current_verification_items_tx(&mut tx, &findings, now).await?;
    let cve_items = current_cve_verification_items_tx(&mut tx, &cve_findings, Some(id)).await?;
    let accepted = items
        .iter()
        .all(|item| closure_result_is_accepted(&item.result))
        && cve_items.iter().all(|item| item.result == "pass");
    let attempt_id:Uuid=sqlx::query_scalar("INSERT INTO poam_verification_attempts(poam_id,attempted_by,outcome,poam_revision,attempted_at) VALUES($1,$2,$3,$4,$5) RETURNING id")
      .bind(id).bind(actor.user_id).bind(if accepted{"accepted"}else{"rejected"}).bind(revision).bind(now).fetch_one(&mut *tx).await?;
    insert_verification_items(&mut tx, attempt_id, &items, now).await?;
    insert_cve_verification_items(&mut tx, attempt_id, &cve_items, now).await?;
    sqlx::query("UPDATE poam_verification_attempts SET sealed_at=$2 WHERE id=$1")
        .bind(attempt_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let results=items.iter().map(|item|json!({"finding_id":item.finding_id,"result":item.result,"assessment_id":item.assessment_id,"waiver_id":item.waiver_id})).collect::<Vec<_>>();
    let cve_results=cve_items.iter().map(|item|json!({"cve_finding_id":item.cve_finding_id,"result":item.result,"scan_id":item.scan_id})).collect::<Vec<_>>();
    let new_revision=bump_and_audit(&mut tx,&actor,id,"verification_attempted",json!({"attempt_id":attempt_id,"outcome":if accepted{"accepted"}else{"rejected"},"items":results,"cve_items":cve_results})).await?;
    tx.commit().await?;
    Ok(
        json!({"attempt_id":attempt_id,"outcome":if accepted{"accepted"}else{"rejected"},"revision":new_revision,"items":results,"cve_items":cve_results}),
    )
}

/// Verifies and closes an awaiting-verification POA&M atomically.
///
/// Closure succeeds only when every current finding has an exact Pass or an
/// accepted applicable waiver. A rejected attempt is retained as evidence.
/// Successful closure retires active SCHEDULED dispositions with the exact
/// links and builds the completed detail before commit. Serialization and
/// deadlock failures are retried twice.
///
/// # Errors
///
/// Returns an authorization, validation, precondition, not-found, conflict, or
/// database error when the POA&M cannot be verified and closed.
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
    let expected_cve_findings = poam_cve_finding_keys(pool, id).await?;
    let result = async {
        // Keep a fresh post-lock snapshot; serializable/repeatable-read would
        // retain the snapshot from before an advisory-lock wait.
        let mut tx = pool.begin().await?;
        lock_cve_keys_tx(&mut tx, &expected_cve_findings).await?;
        for system_id in expected_findings
            .iter()
            .map(|row| row.1)
            .chain(expected_cve_findings.iter().map(|row| row.system_id))
            .collect::<BTreeSet<_>>()
        {
            crate::services::composite_enforcement::lock_poam_system_key_tx(&mut tx, system_id)
                .await?;
        }
        let affected_system_ids = expected_findings
            .iter()
            .map(|row| row.1)
            .chain(expected_cve_findings.iter().map(|row| row.system_id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        lock_policy_finding_keys_for_systems_tx(&mut tx, &affected_system_ids).await?;
        lock_cve_finding_keys_tx(&mut tx, &expected_cve_findings).await?;
        let actor = current_mutating_actor_tx(&mut tx, actor).await?;
        let status = lock_mutable_poam(&mut tx, &actor, id, revision).await?;
        if status != "awaiting_verification" {
            return Err(PoamError::Conflict(
                "invalid_transition",
                "POA&M must be awaiting verification before close".into(),
            ));
        }
        let findings=sqlx::query_as::<_,(Uuid,Uuid,Uuid)>(r#"SELECT f.id,f.system_id,f.policy_lineage_id FROM poam_finding_links l JOIN poam_findings f ON f.id=l.finding_id
          WHERE l.poam_id=$1 AND l.retired_at IS NULL ORDER BY f.system_id,f.policy_lineage_id FOR UPDATE OF l,f"#).bind(id).fetch_all(&mut *tx).await?;
        let cve_findings=sqlx::query_as::<_,CveFindingKey>(r#"SELECT f.id,f.system_id,f.canonical_cve_id,f.canonical_package_name
          FROM poam_cve_finding_links l JOIN poam_cve_findings f ON f.id=l.cve_finding_id
          WHERE l.poam_id=$1 AND l.retired_at IS NULL
          ORDER BY f.system_id,f.canonical_cve_id,f.canonical_package_name,f.id FOR UPDATE OF l,f"#)
          .bind(id).fetch_all(&mut *tx).await?;
        if findings != expected_findings || cve_findings != expected_cve_findings {
            return Err(PoamError::Conflict(
                "concurrent_finding_change",
                "The active finding set changed; retry the request".into(),
            ));
        }
        if findings.is_empty() == cve_findings.is_empty() {
            return Err(PoamError::Validation(
                "finding_required",
                "POA&M must have exactly one active finding family".into(),
            ));
        }
        if !actor_can_access_systems_tx(
            &mut tx,
            &actor,
            &findings.iter().map(|r| r.1).chain(cve_findings.iter().map(|r| r.system_id)).collect::<Vec<_>>(),
        )
        .await?
        {
            return Err(PoamError::NotFound);
        }
        let now = clock.now();
        let items = current_verification_items_tx(&mut tx, &findings, now).await?;
        let cve_items =
            current_cve_verification_items_tx(&mut tx, &cve_findings, Some(id)).await?;
        let accepted = items.iter().all(|item| closure_result_is_accepted(&item.result))
            && cve_items.iter().all(|item| item.result == "pass");
        let attempt_id:Uuid=sqlx::query_scalar("INSERT INTO poam_verification_attempts(poam_id,attempted_by,outcome,poam_revision,attempted_at) VALUES($1,$2,$3,$4,$5) RETURNING id")
          .bind(id).bind(actor.user_id).bind(if accepted{"accepted"}else{"rejected"}).bind(revision).bind(now).fetch_one(&mut *tx).await?;
        insert_verification_items(&mut tx, attempt_id, &items, now).await?;
        insert_cve_verification_items(&mut tx, attempt_id, &cve_items, now).await?;
        sqlx::query("UPDATE poam_verification_attempts SET sealed_at=$2 WHERE id=$1")
            .bind(attempt_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        let results=items.iter().map(|i|json!({"finding_id":i.finding_id,"result":i.result,"assessment_id":i.assessment_id,"waiver_id":i.waiver_id})).collect::<Vec<_>>();
        let cve_results=cve_items.iter().map(|i|json!({"cve_finding_id":i.cve_finding_id,"result":i.result,"scan_id":i.scan_id})).collect::<Vec<_>>();
        let verification_revision=bump_and_audit(&mut tx,&actor,id,"verification_attempted",json!({"attempt_id":attempt_id,"outcome":if accepted{"accepted"}else{"rejected"},"items":results,"cve_items":cve_results})).await?;
        if !accepted {
            tx.commit().await?;
            return Err(PoamError::Precondition(
                "closure_not_ready",
                "Every policy finding requires an accepted result and every exact CVE requires Pass".into(),
                Some(json!({"attempt_id":attempt_id,"revision":verification_revision,"items":results,"cve_items":cve_results})),
            ));
        }
        sqlx::query("UPDATE poam_finding_links SET retired_at=$2,retired_by=$3,retirement_reason=$4 WHERE poam_id=$1 AND retired_at IS NULL")
          .bind(id).bind(now).bind(actor.user_id).bind(format!("closed:{attempt_id}")).execute(&mut *tx).await?;
        sqlx::query("UPDATE poam_cve_finding_links SET retired_at=$2,retired_by=$3,retirement_reason=$4 WHERE poam_id=$1 AND retired_at IS NULL")
          .bind(id).bind(now).bind(actor.user_id).bind(format!("closed:{attempt_id}")).execute(&mut *tx).await?;
        sqlx::query(
            r#"UPDATE cve_environment_dispositions
               SET retired_at=$2,retired_by=$3,retirement_reason='poam_closed'
               WHERE poam_id=$1 AND state='scheduled' AND retired_at IS NULL"#,
        )
        .bind(id)
        .bind(now)
        .bind(actor.user_id)
        .execute(&mut *tx)
        .await?;
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
            &actor,
            id,
            "closed",
            json!({"attempt_id":attempt_id,"closed_at":now}),
        )
        .await?;
        let detail = cve_poam_detail_tx(&mut tx, &actor, id, clock).await?;
        tx.commit().await?;
        Ok(detail)
    }
    .await;
    result
}

/// Reopens a completed POA&M and restores its closure finding set.
///
/// Reopening fails if another active POA&M has claimed a closure finding. For
/// environment-backed exact findings, current subjects must equal the closure
/// set and no active disposition can conflict. Findings without an environment
/// restore their links without creating an environment disposition. The
/// operation builds the reopened detail before commit.
///
/// # Errors
///
/// Returns an authorization, not-found, conflict, or database error when the
/// closure state cannot be restored at the supplied revision.
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
    let expected_cve = sqlx::query_as::<_, CveFindingKey>(
        r#"SELECT finding.id,finding.system_id,finding.canonical_cve_id,
                  finding.canonical_package_name
           FROM poams poam
           JOIN poam_cve_finding_links link ON link.poam_id=poam.id
             AND link.retirement_reason='closed:'||poam.closure_attempt_id::text
           JOIN poam_cve_findings finding ON finding.id=link.cve_finding_id
           WHERE poam.id=$1
           ORDER BY finding.system_id,finding.canonical_cve_id,
                    finding.canonical_package_name,finding.id"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let mut tx = pool.begin().await?;
    lock_cve_keys_tx(&mut tx, &expected_cve).await?;
    for system_id in expected
        .iter()
        .map(|row| row.1)
        .chain(expected_cve.iter().map(|row| row.system_id))
        .collect::<BTreeSet<_>>()
    {
        crate::services::composite_enforcement::lock_poam_system_key_tx(&mut tx, system_id).await?;
    }
    let affected_system_ids = expected
        .iter()
        .map(|row| row.1)
        .chain(expected_cve.iter().map(|row| row.system_id))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    lock_policy_finding_keys_for_systems_tx(&mut tx, &affected_system_ids).await?;
    lock_cve_finding_keys_tx(&mut tx, &expected_cve).await?;
    sqlx::query("SELECT id FROM systems WHERE id=ANY($1) ORDER BY id FOR UPDATE")
        .bind(&affected_system_ids)
        .execute(&mut *tx)
        .await?;
    let actor = current_mutating_actor_tx(&mut tx, actor).await?;
    let row = sqlx::query_as::<_, (i64, String, Option<Uuid>)>(
        "SELECT revision,status,closure_attempt_id FROM poams WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(PoamError::NotFound)?;
    require_poam_contexts_tx(&mut tx, &actor, id).await?;
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
    let cve_findings=sqlx::query_as::<_,CveFindingKey>("SELECT f.id,f.system_id,f.canonical_cve_id,f.canonical_package_name FROM poam_cve_finding_links l JOIN poam_cve_findings f ON f.id=l.cve_finding_id WHERE l.poam_id=$1 AND l.retirement_reason=$2 ORDER BY f.system_id,f.canonical_cve_id,f.canonical_package_name,f.id FOR UPDATE OF l,f")
      .bind(id).bind(format!("closed:{attempt_id}")).fetch_all(&mut *tx).await?;
    let actual_ids = findings.iter().map(|row| row.0).collect::<BTreeSet<_>>();
    let expected_ids = expected.iter().map(|row| row.0).collect::<BTreeSet<_>>();
    if actual_ids != expected_ids || cve_findings != expected_cve {
        return Err(PoamError::Conflict(
            "concurrent_finding_change",
            "The closure finding set changed; retry the request".into(),
        ));
    }
    if !actor_can_access_systems_tx(
        &mut tx,
        &actor,
        &findings
            .iter()
            .map(|r| r.1)
            .chain(cve_findings.iter().map(|r| r.system_id))
            .collect::<Vec<_>>(),
    )
    .await?
    {
        return Err(PoamError::NotFound);
    }
    let claimed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM poam_finding_links WHERE finding_id=ANY($1) AND retired_at IS NULL)")
      .bind(&findings.iter().map(|r|r.0).collect::<Vec<_>>()).fetch_one(&mut *tx).await?;
    let cve_claimed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM poam_cve_finding_links WHERE cve_finding_id=ANY($1) AND retired_at IS NULL)")
      .bind(&cve_findings.iter().map(|r|r.id).collect::<Vec<_>>()).fetch_one(&mut *tx).await?;
    if claimed || cve_claimed {
        return Err(PoamError::Conflict(
            "finding_already_managed",
            "A finding is now managed by another active POA&M".into(),
        ));
    }
    let cve_environments = sqlx::query_as::<_, (Uuid, String, String, Vec<Uuid>)>(
        r#"SELECT system.environment_id,min(finding.canonical_cve_id),
                  min(finding.canonical_package_name),
                  array_agg(finding.system_id ORDER BY finding.system_id)
           FROM poam_cve_finding_links link
           JOIN poam_cve_findings finding ON finding.id=link.cve_finding_id
           JOIN systems system ON system.id=finding.system_id
           WHERE link.poam_id=$1 AND link.retirement_reason=$2
             AND system.environment_id IS NOT NULL
           GROUP BY system.environment_id ORDER BY system.environment_id"#,
    )
    .bind(id)
    .bind(format!("closed:{attempt_id}"))
    .fetch_all(&mut *tx)
    .await?;
    // COMPATIBILITY: Exact-CVE POA&Ms can manage systems without an
    // environment. Reopen restores their links but cannot create an
    // environment-scoped fleet disposition for them.
    for (environment_id, cve_id, package_name, expected_system_ids) in &cve_environments {
        let current_subjects = fleet_cve_subjects_tx(
            &mut tx,
            &actor,
            cve_id,
            package_name,
            Some(&[*environment_id]),
        )
        .await?
        .into_iter()
        .map(|subject| subject.system_id)
        .collect::<BTreeSet<_>>();
        if current_subjects != expected_system_ids.iter().copied().collect::<BTreeSet<_>>() {
            return Err(PoamError::Conflict(
                "cve_disposition_conflict",
                "Current exact subjects no longer match the closure subject set".into(),
            ));
        }
        let disposition_exists: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM cve_environment_dispositions
               WHERE canonical_cve_id=$1 AND canonical_package_name=$2
                 AND environment_id=$3 AND retired_at IS NULL)"#,
        )
        .bind(cve_id)
        .bind(package_name)
        .bind(environment_id)
        .fetch_one(&mut *tx)
        .await?;
        if disposition_exists {
            return Err(PoamError::Conflict(
                "cve_disposition_conflict",
                "A current accepted or scheduled disposition prevents restoration".into(),
            ));
        }
    }
    sqlx::query(
        "UPDATE poams SET status='in_progress',closed_at=NULL,closure_attempt_id=NULL WHERE id=$1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if let Err(error) = sqlx::query("INSERT INTO poam_finding_links(poam_id,finding_id,linked_by) SELECT $1,finding_id,$3 FROM poam_finding_links WHERE poam_id=$1 AND retirement_reason=$2")
        .bind(id).bind(format!("closed:{attempt_id}")).bind(actor.user_id)
        .execute(&mut *tx)
        .await
    {
        return Err(db_conflict(&error).unwrap_or_else(|| error.into()));
    }
    if let Err(error) = sqlx::query("INSERT INTO poam_cve_finding_links(poam_id,cve_finding_id,system_id,canonical_cve_id,canonical_package_name,baseline_scan_id,baseline_scan_derivation_id,baseline_scan_completed_at,baseline_generation_snapshot_id,baseline_generation,baseline_target_store_path,baseline_occurrence_derivation_path,baseline_observed_package_version,linked_by) SELECT $1,cve_finding_id,system_id,canonical_cve_id,canonical_package_name,baseline_scan_id,baseline_scan_derivation_id,baseline_scan_completed_at,baseline_generation_snapshot_id,baseline_generation,baseline_target_store_path,baseline_occurrence_derivation_path,baseline_observed_package_version,$3 FROM poam_cve_finding_links WHERE poam_id=$1 AND retirement_reason=$2")
        .bind(id).bind(format!("closed:{attempt_id}")).bind(actor.user_id)
        .execute(&mut *tx).await
    {
        return Err(db_conflict(&error).unwrap_or_else(|| error.into()));
    }
    for (environment_id, cve_id, package_name, _) in &cve_environments {
        sqlx::query(
            r#"INSERT INTO cve_environment_dispositions(
                  canonical_cve_id,canonical_package_name,environment_id,state,
                  poam_id,scheduled_by,scheduled_at)
               VALUES($1,$2,$3,'scheduled',$4,$5,$6)"#,
        )
        .bind(cve_id)
        .bind(package_name)
        .bind(environment_id)
        .bind(id)
        .bind(actor.user_id)
        .bind(clock.now())
        .execute(&mut *tx)
        .await?;
    }
    bump_and_audit(
        &mut tx,
        &actor,
        id,
        "reopened",
        json!({"previous_closure_attempt_id":attempt_id,"status":"in_progress"}),
    )
    .await?;
    let detail = cve_poam_detail_tx(&mut tx, &actor, id, clock).await?;
    tx.commit().await?;
    Ok(detail)
}

/// Returns aggregate POA&M dashboard counts visible to an actor.
///
/// # Errors
///
/// Returns a database error when visible aggregate counts cannot be loaded.
pub async fn dashboard(
    pool: &PgPool,
    actor: &PoamActor,
    clock: &dyn PoamClock,
) -> Result<DashboardSummary, PoamError> {
    Ok(poam::dashboard(pool, clock.today(), actor.is_admin, &actor.environment_ids).await?)
}
/// Returns the actor's paginated overdue and awaiting-verification watchlist.
///
/// # Errors
///
/// Returns a validation error for invalid page bounds or a database error when
/// the watchlist cannot be loaded.
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
/// Returns POA&M and current-finding rollups for visible systems.
///
/// # Errors
///
/// Returns a validation error for invalid or excessively broad batches and a
/// database error when authoritative rollup evidence cannot be loaded.
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
/// Returns POA&M and current-finding rollups for visible compliance bundles.
///
/// # Errors
///
/// Returns a validation error for invalid or excessively broad batches and a
/// database error when assignment or finding evidence cannot be loaded.
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
            ) ORDER BY poam.id LIMIT $5"#,
    )
    .bind(actor.is_admin)
    .bind(&actor.environment_ids)
    .bind(&relevant_finding_ids)
    .bind(ids)
    .bind(MAX_ROLLUP_POAMS + 1)
    .fetch_all(pool)
    .await?;
    if visible_poams.len() as i64 > MAX_ROLLUP_POAMS {
        return Err(PoamError::Validation(
            "rollup_scope_too_large",
            "The requested rollup expands to too many POA&Ms".into(),
        ));
    }
    let visible_ids = visible_poams.iter().map(|row| row.0).collect::<Vec<_>>();
    let active_links = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT poam_id,finding_id FROM poam_finding_links WHERE poam_id=ANY($1) AND retired_at IS NULL",
    )
    .bind(&visible_ids)
    .fetch_all(pool)
    .await?;
    let closure_bundles = sqlx::query_as::<_, (Uuid, Vec<Uuid>)>(
        r#"SELECT poam.id,ARRAY(
             SELECT DISTINCT unnest(item.bundle_ids)
             FROM poam_verification_items item
             WHERE item.attempt_id=poam.closure_attempt_id
             ORDER BY 1
           ) FROM poams poam WHERE poam.id=ANY($1)"#,
    )
    .bind(&visible_ids)
    .fetch_all(pool)
    .await?;
    let assignment_bundles = sqlx::query_as::<_, (Uuid, Vec<Uuid>)>(
        r#"SELECT reference.poam_id,array_agg(DISTINCT bundle_version.bundle_id ORDER BY bundle_version.bundle_id)
           FROM poam_assignment_references reference
           JOIN compliance_bundle_assignment_versions assignment_version
             ON assignment_version.id=reference.assignment_version_id
           JOIN compliance_bundle_versions bundle_version
             ON bundle_version.id=assignment_version.bundle_version_id
            WHERE reference.poam_id=ANY($1) GROUP BY reference.poam_id"#,
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
    for (poam_id, bundle_ids) in assignment_bundles {
        bundles_by_poam
            .entry(poam_id)
            .or_default()
            .extend(bundle_ids);
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
            ("open", "awaiting_verification"),
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
