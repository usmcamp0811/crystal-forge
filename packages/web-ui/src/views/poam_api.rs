//! Typed API adapter for POA&M workflows.
//!
//! This module intentionally has no view or component dependencies. It mirrors
//! the server contract and preserves structured error responses for callers.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

use crate::api::client::{ApiClientError, base_url, send_request_with_csrf};

const PAGE_SIZE: i64 = 100;
const MAX_BATCH_IDS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoamStatus {
    Open,
    InProgress,
    Blocked,
    AwaitingVerification,
    Completed,
}

impl PoamStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::InProgress => "In Progress",
            Self::Blocked => "Blocked",
            Self::AwaitingVerification => "Awaiting Verification",
            Self::Completed => "Completed",
        }
    }

    pub const fn is_active(self) -> bool {
        !matches!(self, Self::Completed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoamRisk {
    High,
    Medium,
    Low,
}

impl PoamRisk {
    pub const fn label(self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }

    pub const fn category_label(self) -> &'static str {
        match self {
            Self::High => "CAT I",
            Self::Medium => "CAT II",
            Self::Low => "CAT III",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOutcome {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentOutcome {
    Pass,
    Fail,
    Error,
    NotChecked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationResult {
    Pass,
    Waiver,
    Fail,
    Error,
    NotChecked,
    Missing,
    Stale,
    Unknown,
    Warn,
    NotApplicable,
}

impl VerificationResult {
    pub const fn is_accepted(self) -> bool {
        matches!(self, Self::Pass | Self::Waiver)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
    pub next_offset: Option<i64>,
}

impl<T> Page<T> {
    fn validate(&self, requested_offset: i64) -> Result<(), PoamApiError> {
        let coherent_cursor = match (self.has_more, self.next_offset) {
            (true, Some(next)) => next > self.offset,
            (false, None) => true,
            _ => false,
        };
        if self.offset != requested_offset
            || self.limit <= 0
            || self.items.len() as i64 > self.limit
            || !coherent_cursor
        {
            return Err(PoamApiError::Deserialize(
                "incoherent POA&M pagination response".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryCursor {
    pub at: DateTime<Utc>,
    pub id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoamSummary {
    pub id: Uuid,
    pub human_id: String,
    pub title: String,
    pub plan: String,
    pub owner: String,
    pub target_date: Option<NaiveDate>,
    pub risk: PoamRisk,
    pub status: PoamStatus,
    pub revision: i64,
    pub overdue: bool,
    pub finding_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub closure_attempt_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingView {
    pub id: Uuid,
    pub system_id: Uuid,
    pub hostname: String,
    pub environment_id: Option<Uuid>,
    pub policy_lineage_id: Uuid,
    pub policy_name: String,
    pub link_id: Uuid,
    pub linked_at: DateTime<Utc>,
    pub linked_by: Uuid,
    pub retired_at: Option<DateTime<Utc>>,
    pub retired_by: Option<Uuid>,
    pub retirement_reason: Option<String>,
    pub link_active: bool,
    pub current_assessment_id: Option<Uuid>,
    pub current_outcome: Option<AssessmentOutcome>,
    pub current_policy_version_id: Option<Uuid>,
    pub current_target_store_path: Option<String>,
    pub assessment_updated_at: Option<DateTime<Utc>>,
    pub resolution_state: VerificationResult,
    pub effective_set_digest: Option<String>,
    pub effective_config_digest: Option<String>,
    pub bundle_ids: Vec<Uuid>,
    pub bundle_version_ids: Vec<Uuid>,
    pub requirement_version_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneView {
    pub id: Uuid,
    pub ordinal: i32,
    pub title: String,
    pub target_date: NaiveDate,
    pub completed_at: Option<DateTime<Utc>>,
    pub completed_by: Option<Uuid>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentReferenceView {
    pub assignment_id: Uuid,
    pub assignment_version_id: Uuid,
    pub added_by: Uuid,
    pub added_at: DateTime<Utc>,
    pub bundle_id: Uuid,
    pub bundle_version_id: Uuid,
    pub bundle_name: String,
    pub bundle_version: String,
    pub system_id: Option<Uuid>,
    pub system_hostname: Option<String>,
    pub environment_id: Option<Uuid>,
    pub environment_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationItemView {
    pub attempt_id: Uuid,
    pub finding_id: Uuid,
    pub system_id: Uuid,
    pub policy_lineage_id: Uuid,
    pub result: VerificationResult,
    pub policy_version_id: Option<Uuid>,
    pub assessment_id: Option<Uuid>,
    pub derivation_id: Option<i32>,
    pub target_store_path: Option<String>,
    pub effective_set_digest: Option<String>,
    pub effective_config_digest: Option<String>,
    pub effective_config: Option<Value>,
    pub observed_outcome: Option<AssessmentOutcome>,
    pub observation_token: Option<String>,
    pub observation_snapshot: Option<Value>,
    pub assessment_updated_at: Option<DateTime<Utc>>,
    pub bundle_ids: Vec<Uuid>,
    pub bundle_version_ids: Vec<Uuid>,
    pub requirement_version_ids: Vec<Uuid>,
    pub waiver_id: Option<Uuid>,
    pub observed_at: DateTime<Utc>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationAttemptView {
    pub id: Uuid,
    pub outcome: VerificationOutcome,
    pub poam_revision: i64,
    pub attempted_by: Uuid,
    pub attempted_at: DateTime<Utc>,
    pub items: Vec<VerificationItemView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityView {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub kind: String,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoamDetail {
    #[serde(flatten)]
    pub poam: PoamSummary,
    pub findings: Vec<FindingView>,
    pub findings_has_more: bool,
    pub findings_next_cursor: Option<HistoryCursor>,
    pub milestones: Vec<MilestoneView>,
    pub assignment_references: Vec<AssignmentReferenceView>,
    pub verification_attempts: Vec<VerificationAttemptView>,
    pub verification_has_more: bool,
    pub verification_next_cursor: Option<HistoryCursor>,
    pub activity: Vec<ActivityView>,
    pub activity_has_more: bool,
    pub activity_next_cursor: Option<HistoryCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibleFinding {
    pub finding_id: Uuid,
    pub system_id: Uuid,
    pub hostname: String,
    pub environment_id: Option<Uuid>,
    pub policy_lineage_id: Uuid,
    pub policy_name: String,
    pub assessment_id: Option<Uuid>,
    pub outcome: Option<AssessmentOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rollup {
    pub scope_id: Uuid,
    pub total: i64,
    pub active: i64,
    pub overdue: i64,
    pub awaiting_verification: i64,
    pub completed: i64,
    pub open_findings: i64,
    pub on_poam_findings: i64,
    pub no_poam_findings: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationResultSummary {
    pub finding_id: Uuid,
    pub result: VerificationResult,
    pub assessment_id: Option<Uuid>,
    pub waiver_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyPoamResponse {
    pub attempt_id: Uuid,
    pub outcome: VerificationOutcome,
    pub revision: i64,
    pub items: Vec<VerificationResultSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingRelationshipEntry {
    pub assessment_id: Uuid,
    pub finding_id: Uuid,
    #[serde(rename = "active_poam", alias = "active")]
    pub active: Option<PoamSummary>,
    #[serde(rename = "historical_poams", alias = "history")]
    pub history: Vec<PoamSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentRelationshipEntry {
    pub assignment_version_id: Uuid,
    pub poams: Vec<PoamSummary>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoamListQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PoamStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<PoamRisk>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_lineage_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoamDetailQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_before_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_before_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_before_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_before_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_before_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_before_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePoamRequest {
    pub assessment_id: Uuid,
    pub title: String,
    pub plan: String,
    pub owner: String,
    pub target_date: Option<NaiveDate>,
    pub risk: PoamRisk,
    pub default_milestones: bool,
    pub assignment_version_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePoamRequest {
    pub revision: i64,
    pub title: Option<String>,
    pub plan: Option<String>,
    pub owner: Option<String>,
    pub target_date: Option<Option<NaiveDate>>,
    pub risk: Option<PoamRisk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionPoamRequest {
    pub revision: i64,
    pub status: PoamStatus,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionRequest {
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddNoteRequest {
    pub revision: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddMilestoneRequest {
    pub revision: i64,
    pub title: String,
    pub target_date: NaiveDate,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateMilestoneRequest {
    pub revision: i64,
    pub title: Option<String>,
    pub target_date: Option<NaiveDate>,
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFindingRequest {
    pub revision: i64,
    pub assessment_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentReferenceRequest {
    pub revision: i64,
    pub assignment_version_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoamServerError {
    #[serde(skip)]
    pub status: u16,
    #[serde(rename = "error")]
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PoamApiError {
    Network(String),
    Deserialize(String),
    Server(PoamServerError),
}

impl PoamApiError {
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Server(error) if matches!(error.status, 401 | 403))
    }

    pub fn is_not_visible(&self) -> bool {
        matches!(self, Self::Server(error) if error.status == 404)
    }

    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Server(error) if error.code == "stale_revision")
    }

    pub fn is_active_remediation(&self) -> bool {
        matches!(self, Self::Server(error) if error.code == "finding_already_managed")
    }

    pub fn is_precondition(&self) -> bool {
        matches!(self, Self::Server(error) if error.status == 412)
    }

    pub fn is_internal(&self) -> bool {
        matches!(self, Self::Server(error) if error.status >= 500 || error.code == "internal_error")
    }

    pub fn close_precondition_details(&self) -> Option<ClosePreconditionDetails> {
        let Self::Server(error) = self else {
            return None;
        };
        error
            .details
            .clone()
            .and_then(|details| serde_json::from_value(details).ok())
    }
}

impl std::fmt::Display for PoamApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(message) => write!(formatter, "Network error: {message}"),
            Self::Deserialize(message) => write!(formatter, "Deserialization error: {message}"),
            Self::Server(error) => write!(
                formatter,
                "HTTP {} ({}): {}",
                error.status, error.code, error.message
            ),
        }
    }
}

impl From<ApiClientError> for PoamApiError {
    fn from(error: ApiClientError) -> Self {
        match error {
            ApiClientError::Network(message) => Self::Network(message),
            ApiClientError::Deserialize(message) => Self::Deserialize(message),
            ApiClientError::Status { code, body } => Self::Server(PoamServerError {
                status: code,
                code: "http_error".to_string(),
                message: body,
                details: None,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosePreconditionDetails {
    pub attempt_id: Uuid,
    #[serde(alias = "revision")]
    pub committed_revision: i64,
    pub items: Vec<VerificationResultSummary>,
}

async fn request<T: DeserializeOwned, B: Serialize + ?Sized>(
    method: &str,
    url: &str,
    body: Option<&B>,
) -> Result<T, PoamApiError> {
    let payload = body
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| PoamApiError::Deserialize(error.to_string()))?;
    let (status, text) = send_request_with_csrf(method, url, payload.as_deref())
        .await
        .map_err(PoamApiError::from)?;
    parse_response(status, &text)
}

fn parse_response<T: DeserializeOwned>(status: u16, body: &str) -> Result<T, PoamApiError> {
    if (200..300).contains(&status) {
        return serde_json::from_str(body)
            .map_err(|error| PoamApiError::Deserialize(error.to_string()));
    }
    let mut error = serde_json::from_str::<PoamServerError>(body).unwrap_or(PoamServerError {
        status,
        code: "http_error".to_string(),
        message: if body.trim().is_empty() {
            "Request failed".to_string()
        } else {
            body.to_string()
        },
        details: None,
    });
    error.status = status;
    Err(PoamApiError::Server(error))
}

fn with_query<T: Serialize>(path: &str, query: &T) -> Result<String, PoamApiError> {
    let query = serde_urlencoded::to_string(query)
        .map_err(|error| PoamApiError::Deserialize(error.to_string()))?;
    Ok(if query.is_empty() {
        format!("{}{}", base_url(), path)
    } else {
        format!("{}{}?{}", base_url(), path, query)
    })
}

pub async fn list_poams(query: &PoamListQuery) -> Result<Page<PoamSummary>, PoamApiError> {
    let requested_offset = query.offset.unwrap_or(0);
    let page: Page<PoamSummary> =
        request("GET", &with_query("/poams", query)?, None::<&()>).await?;
    page.validate(requested_offset)?;
    Ok(page)
}

pub async fn fetch_all_poams(query: &PoamListQuery) -> Result<Vec<PoamSummary>, PoamApiError> {
    let mut query = query.clone();
    query.limit = Some(query.limit.unwrap_or(PAGE_SIZE).clamp(1, PAGE_SIZE));
    query.offset = Some(query.offset.unwrap_or(0).max(0));
    let mut poams = Vec::new();
    loop {
        let page = list_poams(&query).await?;
        poams.extend(page.items);
        let Some(next_offset) = page.next_offset else {
            return Ok(poams);
        };
        query.offset = Some(next_offset);
    }
}

pub async fn fetch_poam(id: Uuid, query: &PoamDetailQuery) -> Result<PoamDetail, PoamApiError> {
    request(
        "GET",
        &with_query(&format!("/poams/{id}"), query)?,
        None::<&()>,
    )
    .await
}

macro_rules! poam_body_mutation {
    ($name:ident, $method:literal, $suffix:literal, $request:ty, $response:ty) => {
        pub async fn $name(id: Uuid, body: &$request) -> Result<$response, PoamApiError> {
            request(
                $method,
                &format!("{}/poams/{}{}", base_url(), id, $suffix),
                Some(body),
            )
            .await
        }
    };
}

pub async fn create_poam(body: &CreatePoamRequest) -> Result<PoamDetail, PoamApiError> {
    request("POST", &format!("{}/poams", base_url()), Some(body)).await
}

poam_body_mutation!(update_poam, "PATCH", "", UpdatePoamRequest, PoamDetail);
poam_body_mutation!(
    transition_poam,
    "POST",
    "/transition",
    TransitionPoamRequest,
    PoamDetail
);
poam_body_mutation!(add_poam_note, "POST", "/notes", AddNoteRequest, PoamDetail);
poam_body_mutation!(
    add_poam_milestone,
    "POST",
    "/milestones",
    AddMilestoneRequest,
    PoamDetail
);
poam_body_mutation!(
    link_poam_finding,
    "POST",
    "/findings",
    AddFindingRequest,
    PoamDetail
);
poam_body_mutation!(
    link_poam_assignment,
    "POST",
    "/assignments",
    AssignmentReferenceRequest,
    PoamDetail
);
poam_body_mutation!(
    verify_poam,
    "POST",
    "/verify",
    RevisionRequest,
    VerifyPoamResponse
);
poam_body_mutation!(close_poam, "POST", "/close", RevisionRequest, PoamDetail);
poam_body_mutation!(reopen_poam, "POST", "/reopen", RevisionRequest, PoamDetail);

pub async fn update_poam_milestone(
    id: Uuid,
    milestone_id: Uuid,
    body: &UpdateMilestoneRequest,
) -> Result<PoamDetail, PoamApiError> {
    request(
        "PATCH",
        &format!("{}/poams/{id}/milestones/{milestone_id}", base_url()),
        Some(body),
    )
    .await
}

async fn revision_delete(path: String, revision: i64) -> Result<PoamDetail, PoamApiError> {
    request(
        "DELETE",
        &format!("{}{}?revision={revision}", base_url(), path),
        None::<&()>,
    )
    .await
}

pub async fn remove_poam_milestone(
    id: Uuid,
    milestone_id: Uuid,
    revision: i64,
) -> Result<PoamDetail, PoamApiError> {
    revision_delete(format!("/poams/{id}/milestones/{milestone_id}"), revision).await
}

pub async fn unlink_poam_finding(
    id: Uuid,
    finding_id: Uuid,
    revision: i64,
) -> Result<PoamDetail, PoamApiError> {
    revision_delete(format!("/poams/{id}/findings/{finding_id}"), revision).await
}

pub async fn unlink_poam_assignment(
    id: Uuid,
    assignment_version_id: Uuid,
    revision: i64,
) -> Result<PoamDetail, PoamApiError> {
    revision_delete(
        format!("/poams/{id}/assignments/{assignment_version_id}"),
        revision,
    )
    .await
}

#[derive(Serialize)]
struct SearchQuery<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    q: Option<&'a str>,
    limit: i64,
    offset: i64,
}

pub async fn compatible_findings(
    id: Uuid,
    q: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Page<CompatibleFinding>, PoamApiError> {
    let query = SearchQuery {
        q,
        limit: limit.clamp(1, PAGE_SIZE),
        offset: offset.max(0),
    };
    let page: Page<CompatibleFinding> = request(
        "GET",
        &with_query(&format!("/poams/{id}/compatible"), &query)?,
        None::<&()>,
    )
    .await?;
    page.validate(query.offset)?;
    Ok(page)
}

#[derive(Serialize)]
struct CompatiblePoamsQuery<'a> {
    assessment_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    q: Option<&'a str>,
    limit: i64,
    offset: i64,
}

pub async fn compatible_poams(
    assessment_id: Uuid,
    q: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Page<PoamSummary>, PoamApiError> {
    let query = CompatiblePoamsQuery {
        assessment_id,
        q,
        limit: limit.clamp(1, PAGE_SIZE),
        offset: offset.max(0),
    };
    let page: Page<PoamSummary> = request(
        "GET",
        &with_query("/poams/compatible", &query)?,
        None::<&()>,
    )
    .await?;
    page.validate(query.offset)?;
    Ok(page)
}

fn batch_paths(path: &str, parameter: &str, ids: &[Uuid]) -> Vec<String> {
    ids.chunks(MAX_BATCH_IDS)
        .map(|chunk| {
            let ids = chunk
                .iter()
                .map(Uuid::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!("{path}?{parameter}={ids}")
        })
        .collect()
}

async fn fetch_batches<T: DeserializeOwned>(
    path: &str,
    parameter: &str,
    ids: &[Uuid],
) -> Result<Vec<T>, PoamApiError> {
    let mut entries = Vec::new();
    for path in batch_paths(path, parameter, ids) {
        let mut batch: Vec<T> =
            request("GET", &format!("{}{}", base_url(), path), None::<&()>).await?;
        entries.append(&mut batch);
    }
    Ok(entries)
}

pub async fn system_rollups(ids: &[Uuid]) -> Result<Vec<Rollup>, PoamApiError> {
    fetch_batches("/poams/rollups/systems", "ids", ids).await
}

pub async fn bundle_rollups(ids: &[Uuid]) -> Result<Vec<Rollup>, PoamApiError> {
    fetch_batches("/poams/rollups/bundles", "ids", ids).await
}

pub async fn finding_relationships(
    assessment_ids: &[Uuid],
) -> Result<Vec<FindingRelationshipEntry>, PoamApiError> {
    fetch_batches(
        "/poams/relationships/findings",
        "assessment_ids",
        assessment_ids,
    )
    .await
}

pub async fn assignment_relationships(
    assignment_version_ids: &[Uuid],
) -> Result<Vec<AssignmentRelationshipEntry>, PoamApiError> {
    fetch_batches(
        "/poams/relationships/assignments",
        "ids",
        assignment_version_ids,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_and_risk_labels_match_product_vocabulary() {
        assert_eq!(
            PoamStatus::AwaitingVerification.label(),
            "Awaiting Verification"
        );
        assert!(PoamStatus::Blocked.is_active());
        assert!(!PoamStatus::Completed.is_active());
        assert_eq!(PoamRisk::High.label(), "High");
        assert_eq!(PoamRisk::High.category_label(), "CAT I");
        assert_eq!(PoamRisk::Medium.category_label(), "CAT II");
        assert_eq!(PoamRisk::Low.category_label(), "CAT III");
    }

    #[test]
    fn status_and_risk_use_server_wire_values() {
        assert_eq!(
            serde_json::to_string(&PoamStatus::InProgress).unwrap(),
            "\"in_progress\""
        );
        assert_eq!(
            serde_json::to_string(&PoamRisk::Medium).unwrap(),
            "\"medium\""
        );
        assert!(VerificationResult::Pass.is_accepted());
        assert!(!VerificationResult::Stale.is_accepted());
    }

    #[test]
    fn pagination_rejects_incoherent_offsets_and_cursors() {
        let coherent = Page::<()> {
            items: vec![()],
            limit: 25,
            offset: 0,
            has_more: true,
            next_offset: Some(25),
        };
        assert!(coherent.validate(0).is_ok());
        assert!(coherent.validate(25).is_err());

        let mut incoherent = coherent;
        incoherent.next_offset = None;
        assert!(incoherent.validate(0).is_err());
    }

    #[test]
    fn structured_errors_are_preserved_and_classified() {
        let body = r#"{"error":"closure_not_ready","message":"still failing","details":{"attempt_id":"00000000-0000-0000-0000-000000000001","revision":9,"items":[{"finding_id":"00000000-0000-0000-0000-000000000002","result":"fail","assessment_id":null,"waiver_id":null}]}}"#;
        let error = parse_response::<PoamDetail>(412, body).unwrap_err();
        assert!(error.is_precondition());
        let details = error.close_precondition_details().unwrap();
        assert_eq!(details.committed_revision, 9);
        assert_eq!(details.items[0].result, VerificationResult::Fail);

        let stale = parse_response::<PoamDetail>(
            409,
            r#"{"error":"stale_revision","message":"stale","details":null}"#,
        )
        .unwrap_err();
        assert!(stale.is_stale());
        let active = parse_response::<PoamDetail>(
            409,
            r#"{"error":"finding_already_managed","message":"managed","details":null}"#,
        )
        .unwrap_err();
        assert!(active.is_active_remediation());
        assert!(
            parse_response::<PoamDetail>(
                404,
                r#"{"error":"not_found","message":"hidden","details":null}"#
            )
            .unwrap_err()
            .is_not_visible()
        );
        assert!(
            parse_response::<PoamDetail>(
                403,
                r#"{"error":"forbidden","message":"denied","details":null}"#
            )
            .unwrap_err()
            .is_unauthorized()
        );
        assert!(
            parse_response::<PoamDetail>(
                500,
                r#"{"error":"internal_error","message":"failed","details":null}"#
            )
            .unwrap_err()
            .is_internal()
        );
    }

    #[test]
    fn batch_paths_chunk_at_server_limit_and_use_expected_contracts() {
        let ids = (0..201).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
        let paths = batch_paths("/poams/rollups/systems", "ids", &ids);
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0].matches(',').count(), 99);
        assert_eq!(paths[1].matches(',').count(), 99);
        assert_eq!(paths[2].matches(',').count(), 0);
        assert!(paths[0].starts_with("/poams/rollups/systems?ids="));

        let relationships =
            batch_paths("/poams/relationships/findings", "assessment_ids", &ids[..1]);
        assert!(relationships[0].starts_with("/poams/relationships/findings?assessment_ids="));
        assert!(batch_paths("/poams/rollups/bundles", "ids", &[]).is_empty());
    }

    #[test]
    fn mutation_requests_serialize_revision_and_nullable_target_date() {
        let request = UpdatePoamRequest {
            revision: 7,
            target_date: Some(None),
            ..Default::default()
        };
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["revision"], 7);
        assert!(value["target_date"].is_null());
    }
}
