//! Typed API adapter for POA&M workflows.
//!
//! This module intentionally has no view or component dependencies. It mirrors
//! the server contract and preserves structured error responses for callers.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

use crate::api::client::{ApiClientError, base_url, send_request_with_csrf};
pub use crate::api::models::{FindingObservationReference, FindingObservationSource};

const PAGE_SIZE: i64 = 100;
const MAX_BATCH_IDS: usize = 100;
// The server accepts at most 100 relationship-history rows per request.
const RELATIONSHIP_PAGE_SIZE: i64 = 100;
// The server accepts offsets through 10,000. Stop at that boundary instead of
// returning a partial relationship list or issuing an invalid next request.
const MAX_RELATIONSHIP_PAGES: usize = 100;

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

/// Identifies a descending history-page boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryCursor {
    /// Contains the last returned row's timestamp.
    pub at: DateTime<Utc>,
    /// Contains the last returned row's stable tie-breaker ID.
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

/// Contains authoritative fleet-visible POA&M dashboard counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoamDashboardSummary {
    /// Contains all visible POA&Ms in every lifecycle state.
    pub total: i64,
    /// Contains visible POA&Ms that are not completed.
    pub active: i64,
    /// Contains active visible POA&Ms past their target date.
    pub overdue: i64,
    /// Contains visible POA&Ms waiting for verification.
    pub awaiting_verification: i64,
    /// Contains completed visible POA&Ms.
    pub completed: i64,
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
    /// Contains the server-resolved username or email for a known actor.
    pub actor_display: Option<String>,
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
    pub assessment_id: Option<Uuid>,
    pub finding_id: Uuid,
    #[serde(rename = "active_poam", alias = "active")]
    pub active: Option<PoamSummary>,
    #[serde(rename = "historical_poams", alias = "history")]
    pub history: Vec<PoamSummary>,
    /// Indicates that another historical page is available.
    #[serde(default)]
    pub historical_has_more: bool,
    /// Provides the offset for the next historical page.
    #[serde(default)]
    pub historical_next_offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentRelationshipEntry {
    pub assignment_version_id: Uuid,
    pub poams: Vec<PoamSummary>,
    /// Indicates that another related-POA&M page is available.
    #[serde(default)]
    pub poams_has_more: bool,
    /// Provides the offset for the next related-POA&M page.
    #[serde(default)]
    pub poams_next_offset: Option<i64>,
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

/// Selects independent keyset pages in a POA&M detail request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoamDetailQuery {
    /// Limits linked findings returned in this response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_limit: Option<i64>,
    /// Selects findings linked before this timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_before_at: Option<DateTime<Utc>>,
    /// Breaks timestamp ties for `finding_before_at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_before_id: Option<Uuid>,
    /// Limits durable activity events returned in this response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_limit: Option<i64>,
    /// Selects activity created before this timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_before_at: Option<DateTime<Utc>>,
    /// Breaks timestamp ties for `activity_before_at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_before_id: Option<Uuid>,
    /// Limits verification attempts returned in this response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_limit: Option<i64>,
    /// Selects verification attempts created before this timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_before_at: Option<DateTime<Utc>>,
    /// Breaks timestamp ties for `verification_before_at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_before_id: Option<Uuid>,
}

/// Serializes one authoritative failing observation into a create request.
///
/// Composite callers set `assessment_id`. Source-neutral callers set both
/// `finding_id` and `observation`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePoamRequest {
    /// Identifies a current composite assessment for the compatibility API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assessment_id: Option<Uuid>,
    /// Identifies the stable finding for a source-neutral request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_id: Option<Uuid>,
    /// Binds the stable finding to the observation displayed by the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation: Option<FindingObservationReference>,
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

/// Serializes one authoritative failing observation into a link request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFindingRequest {
    pub revision: i64,
    /// Identifies a current composite assessment for the compatibility API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assessment_id: Option<Uuid>,
    /// Identifies the stable finding for a source-neutral request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_id: Option<Uuid>,
    /// Binds the stable finding to the observation displayed by the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation: Option<FindingObservationReference>,
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
    pub fn is_unauthenticated(&self) -> bool {
        matches!(self, Self::Server(error) if error.status == 401)
    }

    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Server(error) if error.status == 403)
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

/// Fetches the server-computed POA&M dashboard summary.
///
/// # Errors
///
/// Returns [`PoamApiError`] when the request fails or the response does not
/// match the dashboard summary contract.
pub async fn dashboard_summary() -> Result<PoamDashboardSummary, PoamApiError> {
    request(
        "GET",
        &format!("{}/poams/dashboard", base_url()),
        None::<&()>,
    )
    .await
}

/// Fetches the first 13 server-ordered POA&M attention rows.
///
/// The fixed bound is the maximum row count supported by the dashboard
/// widget. The response must echo the requested page and a coherent cursor.
///
/// # Errors
///
/// Returns [`PoamApiError`] when the request fails or pagination is
/// incoherent.
pub async fn dashboard_watchlist() -> Result<Page<PoamSummary>, PoamApiError> {
    const LIMIT: i64 = 13;
    const OFFSET: i64 = 0;
    let page: Page<PoamSummary> = request(
        "GET",
        &format!(
            "{}/poams/dashboard/watchlist?limit={LIMIT}&offset={OFFSET}",
            base_url()
        ),
        None::<&()>,
    )
    .await?;
    validate_dashboard_watchlist_page(&page)?;
    Ok(page)
}

fn validate_dashboard_watchlist_page<T>(page: &Page<T>) -> Result<(), PoamApiError> {
    page.validate(0)?;
    let expected_next = page.has_more.then_some(13);
    if page.limit == 13 && page.next_offset == expected_next {
        Ok(())
    } else {
        Err(PoamApiError::Deserialize(
            "incoherent POA&M watchlist page size".to_string(),
        ))
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    assessment_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finding_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observation_source: Option<FindingObservationSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observation_source_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observation_policy_version_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observation_token: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    q: Option<&'a str>,
    limit: i64,
    offset: i64,
}

pub async fn compatible_poams(
    assessment_id: Option<Uuid>,
    finding_id: Option<Uuid>,
    observation: Option<&FindingObservationReference>,
    q: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Page<PoamSummary>, PoamApiError> {
    let query = CompatiblePoamsQuery {
        assessment_id,
        finding_id,
        observation_source: observation.map(|value| value.source),
        observation_source_id: observation.map(|value| value.source_id.as_str()),
        observation_policy_version_id: observation.map(|value| value.policy_version_id),
        observation_token: observation.map(|value| value.token.as_str()),
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

fn relationship_next_offset(
    requested_offset: i64,
    cursors: impl IntoIterator<Item = (bool, Option<i64>)>,
) -> Result<Option<i64>, PoamApiError> {
    let mut next_offset = None;
    for (has_more, next) in cursors {
        match (has_more, next) {
            (false, None) => {}
            (true, Some(next)) if next > requested_offset => {
                if next_offset.is_some_and(|expected| expected != next) {
                    return Err(PoamApiError::Deserialize(
                        "incoherent POA&M relationship pagination cursors".to_string(),
                    ));
                }
                next_offset = Some(next);
            }
            _ => {
                return Err(PoamApiError::Deserialize(
                    "incoherent POA&M relationship pagination response".to_string(),
                ));
            }
        }
    }
    Ok(next_offset)
}

fn merge_finding_relationship_page(
    merged: &mut Vec<FindingRelationshipEntry>,
    page: Vec<FindingRelationshipEntry>,
    requested_offset: i64,
) -> Result<Option<i64>, PoamApiError> {
    let next_offset = relationship_next_offset(
        requested_offset,
        page.iter()
            .map(|entry| (entry.historical_has_more, entry.historical_next_offset)),
    )?;
    for mut entry in page {
        if let Some(existing) = merged.iter_mut().find(|existing| {
            existing.finding_id == entry.finding_id && existing.assessment_id == entry.assessment_id
        }) {
            if existing.active.as_ref().map(|poam| poam.id)
                != entry.active.as_ref().map(|poam| poam.id)
            {
                return Err(PoamApiError::Deserialize(
                    "POA&M finding relationship changed while loading history".to_string(),
                ));
            }
            for poam in entry.history.drain(..) {
                if !existing.history.iter().any(|current| current.id == poam.id) {
                    existing.history.push(poam);
                }
            }
            existing.historical_has_more = entry.historical_has_more;
            existing.historical_next_offset = entry.historical_next_offset;
        } else {
            merged.push(entry);
        }
    }
    Ok(next_offset)
}

fn merge_assignment_relationship_page(
    merged: &mut Vec<AssignmentRelationshipEntry>,
    page: Vec<AssignmentRelationshipEntry>,
    requested_offset: i64,
) -> Result<Option<i64>, PoamApiError> {
    let next_offset = relationship_next_offset(
        requested_offset,
        page.iter()
            .map(|entry| (entry.poams_has_more, entry.poams_next_offset)),
    )?;
    for mut entry in page {
        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| existing.assignment_version_id == entry.assignment_version_id)
        {
            for poam in entry.poams.drain(..) {
                if !existing.poams.iter().any(|current| current.id == poam.id) {
                    existing.poams.push(poam);
                }
            }
            existing.poams_has_more = entry.poams_has_more;
            existing.poams_next_offset = entry.poams_next_offset;
        } else {
            merged.push(entry);
        }
    }
    Ok(next_offset)
}

async fn fetch_relationship_batches<T: DeserializeOwned>(
    path: &str,
    parameter: &str,
    ids: &[Uuid],
    merge_page: fn(&mut Vec<T>, Vec<T>, i64) -> Result<Option<i64>, PoamApiError>,
) -> Result<Vec<T>, PoamApiError> {
    let mut merged = Vec::new();
    for chunk in ids.chunks(MAX_BATCH_IDS) {
        let ids = chunk
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let mut offset = 0;
        for page_number in 0..MAX_RELATIONSHIP_PAGES {
            let url = format!(
                "{}{path}?{parameter}={ids}&history_limit={RELATIONSHIP_PAGE_SIZE}&history_offset={offset}",
                base_url()
            );
            let page: Vec<T> = request("GET", &url, None::<&()>).await?;
            let Some(next_offset) = merge_page(&mut merged, page, offset)? else {
                break;
            };
            if page_number + 1 == MAX_RELATIONSHIP_PAGES {
                return Err(PoamApiError::Deserialize(format!(
                    "POA&M relationship history exceeds the safe client limit of {} records per relationship",
                    RELATIONSHIP_PAGE_SIZE * MAX_RELATIONSHIP_PAGES as i64
                )));
            }
            offset = next_offset;
        }
    }
    Ok(merged)
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
    fetch_relationship_batches(
        "/poams/relationships/findings",
        "assessment_ids",
        assessment_ids,
        merge_finding_relationship_page,
    )
    .await
}

pub async fn finding_relationships_by_finding(
    finding_ids: &[Uuid],
) -> Result<Vec<FindingRelationshipEntry>, PoamApiError> {
    fetch_relationship_batches(
        "/poams/relationships/findings",
        "finding_ids",
        finding_ids,
        merge_finding_relationship_page,
    )
    .await
}

pub async fn assignment_relationships(
    assignment_version_ids: &[Uuid],
) -> Result<Vec<AssignmentRelationshipEntry>, PoamApiError> {
    fetch_relationship_batches(
        "/poams/relationships/assignments",
        "ids",
        assignment_version_ids,
        merge_assignment_relationship_page,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: u128) -> PoamSummary {
        PoamSummary {
            id: Uuid::from_u128(id),
            human_id: format!("POAM-{id:04}"),
            title: format!("POA&M {id}"),
            plan: "Plan".to_string(),
            owner: "Owner".to_string(),
            target_date: None,
            risk: PoamRisk::Medium,
            status: PoamStatus::Completed,
            revision: 1,
            overdue: false,
            finding_count: 1,
            created_at: DateTime::from_timestamp(1, 0).unwrap(),
            updated_at: DateTime::from_timestamp(id as i64, 0).unwrap(),
            closed_at: None,
            closure_attempt_id: None,
        }
    }

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
    fn dashboard_summary_uses_authoritative_wire_fields() {
        let summary = parse_response::<PoamDashboardSummary>(
            200,
            r#"{"total":9,"active":5,"overdue":2,"awaiting_verification":1,"completed":4}"#,
        )
        .unwrap();
        assert_eq!(summary.active, 5);
        assert_eq!(summary.overdue, 2);
        assert_eq!(summary.awaiting_verification, 1);
        assert_eq!(summary.completed, 4);
    }

    #[test]
    fn dashboard_watchlist_pagination_requires_exact_bound() {
        let mut page = Page::<()> {
            items: vec![],
            limit: 13,
            offset: 0,
            has_more: false,
            next_offset: None,
        };
        assert!(validate_dashboard_watchlist_page(&page).is_ok());
        page.limit = 12;
        assert!(validate_dashboard_watchlist_page(&page).is_err());
        page.limit = 13;
        page.offset = 1;
        assert!(validate_dashboard_watchlist_page(&page).is_err());
        page.offset = 0;
        page.has_more = true;
        page.next_offset = Some(14);
        assert!(validate_dashboard_watchlist_page(&page).is_err());
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
    fn relationship_pagination_metadata_defaults_for_rolling_compatibility() {
        let finding: FindingRelationshipEntry = serde_json::from_str(
            r#"{"assessment_id":null,"finding_id":"00000000-0000-0000-0000-000000000001","active_poam":null,"historical_poams":[]}"#,
        )
        .unwrap();
        assert!(!finding.historical_has_more);
        assert_eq!(finding.historical_next_offset, None);

        let assignment: AssignmentRelationshipEntry = serde_json::from_str(
            r#"{"assignment_version_id":"00000000-0000-0000-0000-000000000002","poams":[]}"#,
        )
        .unwrap();
        assert!(!assignment.poams_has_more);
        assert_eq!(assignment.poams_next_offset, None);
    }

    #[test]
    fn finding_relationship_pages_merge_history_by_stable_identity() {
        let finding_id = Uuid::from_u128(10);
        let mut merged = Vec::new();
        let next = merge_finding_relationship_page(
            &mut merged,
            vec![FindingRelationshipEntry {
                assessment_id: None,
                finding_id,
                active: None,
                history: vec![summary(1)],
                historical_has_more: true,
                historical_next_offset: Some(100),
            }],
            0,
        )
        .unwrap();
        assert_eq!(next, Some(100));

        let next = merge_finding_relationship_page(
            &mut merged,
            vec![FindingRelationshipEntry {
                assessment_id: None,
                finding_id,
                active: None,
                history: vec![summary(1), summary(2)],
                historical_has_more: false,
                historical_next_offset: None,
            }],
            100,
        )
        .unwrap();
        assert_eq!(next, None);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0]
                .history
                .iter()
                .map(|poam| poam.id)
                .collect::<Vec<_>>(),
            vec![Uuid::from_u128(1), Uuid::from_u128(2)]
        );
        assert!(!merged[0].historical_has_more);
    }

    #[test]
    fn assignment_relationship_pages_merge_poams_and_reject_bad_cursors() {
        let assignment_version_id = Uuid::from_u128(20);
        let mut merged = Vec::new();
        assert_eq!(
            merge_assignment_relationship_page(
                &mut merged,
                vec![AssignmentRelationshipEntry {
                    assignment_version_id,
                    poams: vec![summary(3)],
                    poams_has_more: true,
                    poams_next_offset: Some(100),
                }],
                0,
            )
            .unwrap(),
            Some(100)
        );
        assert_eq!(
            merge_assignment_relationship_page(
                &mut merged,
                vec![AssignmentRelationshipEntry {
                    assignment_version_id,
                    poams: vec![summary(4)],
                    poams_has_more: false,
                    poams_next_offset: None,
                }],
                100,
            )
            .unwrap(),
            None
        );
        assert_eq!(merged[0].poams.len(), 2);
        assert!(relationship_next_offset(100, [(true, Some(100))]).is_err());
        assert!(relationship_next_offset(100, [(false, Some(200))]).is_err());
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
