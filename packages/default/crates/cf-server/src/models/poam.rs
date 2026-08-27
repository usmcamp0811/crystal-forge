use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::AwaitingVerification => "awaiting_verification",
            Self::Completed => "completed",
        }
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreatePoamRequest {
    pub assessment_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub plan: String,
    #[serde(default)]
    pub owner: String,
    pub target_date: Option<NaiveDate>,
    pub risk: PoamRisk,
    #[serde(default = "default_true")]
    pub default_milestones: bool,
    #[serde(default)]
    pub assignment_version_ids: Vec<Uuid>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdatePoamRequest {
    pub revision: i64,
    pub title: Option<String>,
    pub plan: Option<String>,
    pub owner: Option<String>,
    pub target_date: Option<Option<NaiveDate>>,
    pub risk: Option<PoamRisk>,
}

#[derive(Debug, Deserialize)]
pub struct TransitionPoamRequest {
    pub revision: i64,
    pub status: PoamStatus,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RevisionRequest {
    pub revision: i64,
}

#[derive(Debug, Deserialize)]
pub struct AddFindingRequest {
    pub revision: i64,
    pub assessment_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct AddNoteRequest {
    pub revision: i64,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct AddMilestoneRequest {
    pub revision: i64,
    pub title: String,
    pub target_date: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMilestoneRequest {
    pub revision: i64,
    pub title: Option<String>,
    pub target_date: Option<NaiveDate>,
    pub completed: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct AssignmentReferenceRequest {
    pub revision: i64,
    pub assignment_version_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct CreateWaiverRequest {
    pub finding_id: Uuid,
    pub assessment_id: Uuid,
    pub justification: String,
}

#[derive(Debug, Deserialize)]
pub struct WaiverDecisionRequest {
    pub status: WaiverDecision,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaiverDecision {
    Accepted,
    Rejected,
    Revoked,
    Expired,
}

impl WaiverDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct WaiverListQuery {
    pub status: Option<String>,
    pub finding_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct WaiverView {
    pub id: Uuid,
    pub finding_id: Uuid,
    pub system_id: Uuid,
    pub policy_lineage_id: Uuid,
    pub status: String,
    pub justification: String,
    pub policy_version_id: Uuid,
    pub assessment_id: Uuid,
    pub observation_token: String,
    pub observation_snapshot: serde_json::Value,
    pub accepted_by: Option<Uuid>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PoamListQuery {
    pub status: Option<String>,
    pub risk: Option<String>,
    pub owner: Option<String>,
    pub system_id: Option<Uuid>,
    pub policy_lineage_id: Option<Uuid>,
    pub bundle_id: Option<Uuid>,
    pub requirement: Option<String>,
    pub overdue: Option<bool>,
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PoamDetailQuery {
    pub finding_limit: Option<i64>,
    pub finding_before_at: Option<DateTime<Utc>>,
    pub finding_before_id: Option<Uuid>,
    pub activity_limit: Option<i64>,
    pub activity_before_at: Option<DateTime<Utc>>,
    pub activity_before_id: Option<Uuid>,
    pub verification_limit: Option<i64>,
    pub verification_before_at: Option<DateTime<Utc>>,
    pub verification_before_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryCursor {
    pub at: DateTime<Utc>,
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PoamSummary {
    pub id: Uuid,
    pub human_id: String,
    pub title: String,
    pub plan: String,
    pub owner: String,
    pub target_date: Option<NaiveDate>,
    pub risk: String,
    pub status: String,
    pub revision: i64,
    pub overdue: bool,
    pub finding_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub closure_attempt_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
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
    pub current_outcome: Option<String>,
    pub current_policy_version_id: Option<Uuid>,
    pub current_target_store_path: Option<String>,
    pub assessment_updated_at: Option<DateTime<Utc>>,
    pub resolution_state: String,
    pub effective_set_digest: Option<String>,
    pub effective_config_digest: Option<String>,
    pub bundle_ids: Vec<Uuid>,
    pub bundle_version_ids: Vec<Uuid>,
    pub requirement_version_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
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

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
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

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct VerificationAttemptView {
    pub id: Uuid,
    pub outcome: String,
    pub poam_revision: i64,
    pub attempted_by: Uuid,
    pub attempted_at: DateTime<Utc>,
    pub items: Vec<VerificationItemView>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct VerificationItemView {
    pub attempt_id: Uuid,
    pub finding_id: Uuid,
    pub system_id: Uuid,
    pub policy_lineage_id: Uuid,
    pub result: String,
    pub policy_version_id: Option<Uuid>,
    pub assessment_id: Option<Uuid>,
    pub derivation_id: Option<i32>,
    pub target_store_path: Option<String>,
    pub effective_set_digest: Option<String>,
    pub effective_config_digest: Option<String>,
    pub effective_config: Option<serde_json::Value>,
    pub observed_outcome: Option<String>,
    pub observation_token: Option<String>,
    pub observation_snapshot: Option<serde_json::Value>,
    pub assessment_updated_at: Option<DateTime<Utc>>,
    pub bundle_ids: Vec<Uuid>,
    pub bundle_version_ids: Vec<Uuid>,
    pub requirement_version_ids: Vec<Uuid>,
    pub waiver_id: Option<Uuid>,
    pub observed_at: DateTime<Utc>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ActivityView {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CompatibleFinding {
    pub finding_id: Uuid,
    pub system_id: Uuid,
    pub hostname: String,
    pub environment_id: Option<Uuid>,
    pub policy_lineage_id: Uuid,
    pub policy_name: String,
    pub assessment_id: Option<Uuid>,
    pub outcome: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
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

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DashboardSummary {
    pub total: i64,
    pub active: i64,
    pub overdue: i64,
    pub awaiting_verification: i64,
    pub completed: i64,
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
    pub next_offset: Option<i64>,
}
