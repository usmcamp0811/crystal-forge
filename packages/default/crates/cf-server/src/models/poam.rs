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

/// Requests creation of a POA&M from one authoritative failing observation.
///
/// Callers provide either `assessment_id` for composite-assessment compatibility,
/// or provide `finding_id` and `observation` together for source-neutral evidence.
/// Mixing or omitting these identity forms is invalid.
#[derive(Debug, Deserialize)]
pub struct CreatePoamRequest {
    /// Identifies a current composite assessment when using the compatibility API.
    #[serde(default)]
    pub assessment_id: Option<Uuid>,
    /// Identifies the stable finding when using source-neutral evidence.
    #[serde(default)]
    pub finding_id: Option<Uuid>,
    /// Binds a stable finding to the exact authoritative observation shown to the user.
    #[serde(default)]
    pub observation: Option<FindingObservationReference>,
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

/// Requests linking one authoritative failing observation to an existing POA&M.
///
/// The identity rules match [`CreatePoamRequest`].
#[derive(Debug, Deserialize)]
pub struct AddFindingRequest {
    pub revision: i64,
    /// Identifies a current composite assessment when using the compatibility API.
    #[serde(default)]
    pub assessment_id: Option<Uuid>,
    /// Identifies the stable finding when using source-neutral evidence.
    #[serde(default)]
    pub finding_id: Option<Uuid>,
    /// Binds a stable finding to the exact authoritative observation shown to the user.
    #[serde(default)]
    pub observation: Option<FindingObservationReference>,
}

/// Identifies the server-owned evidence source behind a source-neutral finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingObservationSource {
    /// Uses the deployed derivation's persisted Nix policy result.
    NixPolicyResult,
    /// Uses the latest completed CVE scan for the deployed derivation.
    CveScan,
}

/// Binds a stable finding to one exact, server-recomputable observation.
///
/// The server rechecks all fields and the semantic token against current
/// effective policy and deployed evidence before it creates or links a POA&M.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingObservationReference {
    /// Selects the authoritative evidence resolver.
    pub source: FindingObservationSource,
    /// Identifies the source record, such as a derivation or CVE scan.
    pub source_id: String,
    /// Identifies the effective immutable policy version.
    pub policy_version_id: Uuid,
    /// Binds source values, policy semantics, and deployment identity.
    pub token: String,
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

/// Selects bounded independent history pages for a POA&M detail response.
///
/// Each history feed uses a `(created_at, id)` keyset cursor. A timestamp and
/// ID for one feed must be provided together and must not be mixed with another
/// feed's cursor.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PoamDetailQuery {
    /// Limits linked findings returned in this response.
    pub finding_limit: Option<i64>,
    /// Selects findings linked before this timestamp.
    pub finding_before_at: Option<DateTime<Utc>>,
    /// Breaks timestamp ties for `finding_before_at`.
    pub finding_before_id: Option<Uuid>,
    /// Limits durable activity events returned in this response.
    pub activity_limit: Option<i64>,
    /// Selects activity created before this timestamp.
    pub activity_before_at: Option<DateTime<Utc>>,
    /// Breaks timestamp ties for `activity_before_at`.
    pub activity_before_id: Option<Uuid>,
    /// Limits verification attempts returned in this response.
    pub verification_limit: Option<i64>,
    /// Selects verification attempts created before this timestamp.
    pub verification_before_at: Option<DateTime<Utc>>,
    /// Breaks timestamp ties for `verification_before_at`.
    pub verification_before_id: Option<Uuid>,
}

/// Identifies the next item boundary in a descending history feed.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryCursor {
    /// Contains the last returned row's authoritative timestamp.
    pub at: DateTime<Utc>,
    /// Contains the last returned row's stable ID for deterministic tie-breaking.
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

#[derive(Debug, Clone, Serialize)]
pub struct FindingPoamRelationship {
    pub assessment_id: Option<Uuid>,
    pub finding_id: Uuid,
    pub active_poam: Option<PoamSummary>,
    pub historical_poams: Vec<PoamSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssignmentPoamRelationship {
    pub assignment_version_id: Uuid,
    pub poams: Vec<PoamSummary>,
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
    /// Contains the current username or email for a known actor.
    pub actor_display: Option<String>,
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
