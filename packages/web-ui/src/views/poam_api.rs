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

/// Represents the persisted POA&M lifecycle state.
///
/// Open, in-progress, and blocked plans can move among those active states or
/// await verification. Awaiting plans can return to in-progress or blocked.
/// Only the server's authoritative close operation can complete a plan, and a
/// completed plan can leave that state only through the reopen operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoamStatus {
    /// Accepts planning work but has not started remediation.
    Open,
    /// Indicates that remediation work is in progress.
    InProgress,
    /// Indicates that remediation cannot currently proceed.
    Blocked,
    /// Indicates that the plan is waiting for authoritative verification.
    AwaitingVerification,
    /// Indicates that authoritative verification accepted and closed the plan.
    Completed,
}

impl PoamStatus {
    /// Returns the product label for this lifecycle state.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::InProgress => "In Progress",
            Self::Blocked => "Blocked",
            Self::AwaitingVerification => "Awaiting Verification",
            Self::Completed => "Completed",
        }
    }

    /// Returns whether this state permits an active remediation relationship.
    pub const fn is_active(self) -> bool {
        !matches!(self, Self::Completed)
    }
}

/// Represents the persisted POA&M risk category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoamRisk {
    /// Represents CAT I high risk.
    High,
    /// Represents CAT II medium risk.
    Medium,
    /// Represents CAT III low risk.
    Low,
}

impl PoamRisk {
    /// Returns the severity label without the category prefix.
    pub const fn label(self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }

    /// Returns the CAT category label for this risk.
    pub const fn category_label(self) -> &'static str {
        match self {
            Self::High => "CAT I",
            Self::Medium => "CAT II",
            Self::Low => "CAT III",
        }
    }
}

/// Represents the server decision for one verification attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOutcome {
    /// Indicates that every required finding result permits closure.
    Accepted,
    /// Indicates that at least one result prevents closure.
    Rejected,
}

/// Represents the source assessment outcome recorded for a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentOutcome {
    /// Indicates that the assessed policy passed.
    Pass,
    /// Indicates a deficiency eligible for remediation.
    Fail,
    /// Indicates that evaluation failed to produce a policy decision.
    Error,
    /// Indicates that the policy was not evaluated.
    NotChecked,
}

/// Represents the normalized result used by POA&M verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationResult {
    /// Indicates that authoritative evidence passed.
    Pass,
    /// Indicates that an authoritative waiver accepts the finding.
    Waiver,
    /// Indicates that authoritative evidence still fails.
    Fail,
    /// Indicates that verification encountered an evaluation error.
    Error,
    /// Indicates that verification did not evaluate the finding.
    NotChecked,
    /// Indicates that required authoritative evidence is missing.
    Missing,
    /// Indicates that the available evidence is no longer current.
    Stale,
    /// Indicates that the server cannot classify the result.
    Unknown,
    /// Indicates a non-passing warning result.
    Warn,
    /// Indicates that the control does not apply to the target.
    NotApplicable,
}

impl VerificationResult {
    /// Returns whether this result permits POA&M closure.
    pub const fn is_accepted(self) -> bool {
        matches!(self, Self::Pass | Self::Waiver)
    }
}

/// Contains one validated offset-paginated server response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    /// Contains the rows in server-defined order.
    pub items: Vec<T>,
    /// Contains the maximum number of rows requested for this page.
    pub limit: i64,
    /// Contains the zero-based offset represented by this page.
    pub offset: i64,
    /// Indicates that another page is available.
    pub has_more: bool,
    /// Provides the next offset exactly when `has_more` is true.
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

/// Summarizes one server-visible POA&M and its current lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoamSummary {
    /// Identifies the POA&M.
    pub id: Uuid,
    /// Contains the stable human-readable POA&M identifier.
    pub human_id: String,
    /// Contains the remediation title.
    pub title: String,
    /// Contains the remediation plan text.
    pub plan: String,
    /// Contains the responsible owner label.
    pub owner: String,
    /// Contains the planned completion date when one is set.
    pub target_date: Option<NaiveDate>,
    /// Contains the persisted risk category.
    pub risk: PoamRisk,
    /// Contains the persisted lifecycle state.
    pub status: PoamStatus,
    /// Contains the optimistic concurrency revision.
    pub revision: i64,
    /// Indicates that an active plan is past its target date.
    pub overdue: bool,
    /// Counts active findings, or the closure finding set for a completed plan.
    pub finding_count: i64,
    /// Records when the plan was created.
    pub created_at: DateTime<Utc>,
    /// Records when the plan last changed.
    pub updated_at: DateTime<Utc>,
    /// Records when the plan closed, if completed.
    pub closed_at: Option<DateTime<Utc>>,
    /// Identifies the verification attempt that authorized closure.
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

/// Describes authoritative requirement and framework labels for a linked finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingRequirementView {
    /// Identifies the immutable requirement version.
    pub requirement_version_id: Uuid,
    /// Contains the framework-published requirement or control identifier.
    pub external_id: String,
    /// Contains the optional requirement or control title.
    pub title: Option<String>,
    /// Identifies the framework lineage.
    pub framework_id: Uuid,
    /// Contains the human-readable framework name.
    pub framework_name: String,
    /// Identifies the immutable framework release.
    pub framework_version_id: Uuid,
    /// Contains the human-readable framework release version.
    pub framework_version: String,
    /// Contains the optional framework release title.
    pub framework_title: Option<String>,
}

/// Describes one authoritative finding linked to a POA&M.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingView {
    /// Identifies the stable finding.
    pub id: Uuid,
    /// Identifies the affected system.
    pub system_id: Uuid,
    /// Contains the affected system's display hostname.
    pub hostname: String,
    /// Identifies the system environment when assigned.
    pub environment_id: Option<Uuid>,
    /// Identifies the policy lineage.
    pub policy_lineage_id: Uuid,
    /// Contains the policy display name.
    pub policy_name: String,
    /// Identifies this durable finding-to-POA&M link.
    pub link_id: Uuid,
    /// Records when the finding was linked.
    pub linked_at: DateTime<Utc>,
    /// Identifies the user who linked the finding.
    pub linked_by: Uuid,
    /// Records when this link was retired.
    pub retired_at: Option<DateTime<Utc>>,
    /// Identifies the user who retired this link.
    pub retired_by: Option<Uuid>,
    /// Contains the durable retirement explanation.
    pub retirement_reason: Option<String>,
    /// Indicates whether this link participates in current verification.
    pub link_active: bool,
    /// Identifies the current composite assessment when one exists.
    pub current_assessment_id: Option<Uuid>,
    /// Contains the current source assessment outcome.
    pub current_outcome: Option<AssessmentOutcome>,
    /// Identifies the effective policy version for the current observation.
    pub current_policy_version_id: Option<Uuid>,
    /// Identifies the evaluated derivation for the current observation.
    pub current_target_store_path: Option<String>,
    /// Records when the current assessment last changed.
    pub assessment_updated_at: Option<DateTime<Utc>>,
    /// Contains the server-normalized current verification result.
    pub resolution_state: VerificationResult,
    /// Contains the current effective policy-set digest.
    pub effective_set_digest: Option<String>,
    /// Contains the current effective configuration digest.
    pub effective_config_digest: Option<String>,
    /// Contains related bundle lineage identities.
    pub bundle_ids: Vec<Uuid>,
    /// Contains related immutable bundle version identities.
    pub bundle_version_ids: Vec<Uuid>,
    /// Contains related immutable requirement version identities.
    pub requirement_version_ids: Vec<Uuid>,
    /// Contains additive display metadata when the server can resolve UUIDs.
    #[serde(default)]
    pub requirements: Vec<FindingRequirementView>,
}

/// Describes one durable POA&M milestone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneView {
    /// Identifies the milestone.
    pub id: Uuid,
    /// Determines the milestone's stable display order.
    pub ordinal: i32,
    /// Contains the milestone title.
    pub title: String,
    /// Contains the milestone target date.
    pub target_date: NaiveDate,
    /// Records when the milestone was completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Identifies the user who completed the milestone.
    pub completed_by: Option<Uuid>,
    /// Identifies the user who created the milestone.
    pub created_by: Uuid,
    /// Identifies the user who last updated the milestone.
    pub updated_by: Uuid,
    /// Records when the milestone was created.
    pub created_at: DateTime<Utc>,
    /// Records when the milestone last changed.
    pub updated_at: DateTime<Utc>,
}

/// Describes one immutable assignment version referenced by a POA&M.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentReferenceView {
    /// Identifies the assignment lineage.
    pub assignment_id: Uuid,
    /// Identifies the exact assignment version.
    pub assignment_version_id: Uuid,
    /// Identifies the user who added the reference.
    pub added_by: Uuid,
    /// Records when the reference was added.
    pub added_at: DateTime<Utc>,
    /// Identifies the assigned bundle lineage.
    pub bundle_id: Uuid,
    /// Identifies the exact assigned bundle version.
    pub bundle_version_id: Uuid,
    /// Contains the bundle display name.
    pub bundle_name: String,
    /// Contains the bundle version label.
    pub bundle_version: String,
    /// Identifies a system scope when this is a system assignment.
    pub system_id: Option<Uuid>,
    /// Contains the system hostname when this is a system assignment.
    pub system_hostname: Option<String>,
    /// Identifies an environment scope when this is an environment assignment.
    pub environment_id: Option<Uuid>,
    /// Contains the environment name when this is an environment assignment.
    pub environment_name: Option<String>,
}

/// Describes one finding result committed during a verification attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationItemView {
    /// Identifies the containing verification attempt.
    pub attempt_id: Uuid,
    /// Identifies the verified stable finding.
    pub finding_id: Uuid,
    /// Identifies the affected system.
    pub system_id: Uuid,
    /// Contains the human-readable hostname when supplied by the server.
    #[serde(default)]
    pub hostname: Option<String>,
    /// Identifies the policy lineage.
    pub policy_lineage_id: Uuid,
    /// Contains the human-readable policy name when supplied by the server.
    #[serde(default)]
    pub policy_name: Option<String>,
    /// Contains the immutable policy version label when supplied by the server.
    #[serde(default)]
    pub policy_version: Option<String>,
    /// Contains the normalized result committed by the server.
    pub result: VerificationResult,
    /// Identifies the effective policy version when one was resolved.
    pub policy_version_id: Option<Uuid>,
    /// Identifies the source assessment when one was resolved.
    pub assessment_id: Option<Uuid>,
    /// Identifies the assessed derivation record when one was resolved.
    pub derivation_id: Option<i32>,
    /// Contains the assessed derivation store path.
    pub target_store_path: Option<String>,
    /// Contains the effective policy-set digest used for verification.
    pub effective_set_digest: Option<String>,
    /// Contains the effective configuration digest used for verification.
    pub effective_config_digest: Option<String>,
    /// Contains the effective policy configuration captured for verification.
    pub effective_config: Option<Value>,
    /// Contains the source assessment outcome observed by verification.
    pub observed_outcome: Option<AssessmentOutcome>,
    /// Binds source-neutral verification to the exact source observation.
    pub observation_token: Option<String>,
    /// Contains the immutable source observation captured by verification.
    pub observation_snapshot: Option<Value>,
    /// Records when the source assessment last changed.
    pub assessment_updated_at: Option<DateTime<Utc>>,
    /// Contains related bundle lineage identities.
    pub bundle_ids: Vec<Uuid>,
    /// Contains related immutable bundle version identities.
    pub bundle_version_ids: Vec<Uuid>,
    /// Contains related immutable requirement version identities.
    pub requirement_version_ids: Vec<Uuid>,
    /// Contains additive requirement and framework labels for current servers.
    #[serde(default)]
    pub requirements: Vec<FindingRequirementView>,
    /// Identifies the waiver accepted for this result, if any.
    pub waiver_id: Option<Uuid>,
    /// Records when the server observed the verification result.
    pub observed_at: DateTime<Utc>,
    /// Contains the server's human-readable result explanation.
    pub detail: String,
}

/// Describes one durable POA&M verification attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationAttemptView {
    /// Identifies the attempt.
    pub id: Uuid,
    /// Contains the aggregate server decision.
    pub outcome: VerificationOutcome,
    /// Contains the POA&M revision verified by this attempt.
    pub poam_revision: i64,
    /// Identifies the user who requested verification.
    pub attempted_by: Uuid,
    /// Records when verification was committed.
    pub attempted_at: DateTime<Utc>,
    /// Contains the immutable finding results from this attempt.
    pub items: Vec<VerificationItemView>,
}

/// Describes one durable POA&M activity event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityView {
    /// Identifies the activity event.
    pub id: Uuid,
    /// Identifies the actor when the event was user-initiated.
    pub actor_user_id: Option<Uuid>,
    /// Contains the server-resolved username or email for a known actor.
    pub actor_display: Option<String>,
    /// Contains the stable machine-readable activity kind.
    pub kind: String,
    /// Contains kind-specific durable activity metadata.
    pub payload: Value,
    /// Records when the event occurred.
    pub created_at: DateTime<Utc>,
}

/// Contains one POA&M and independently paginated related history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoamDetail {
    /// Contains the current POA&M summary and revision.
    #[serde(flatten)]
    pub poam: PoamSummary,
    /// Contains the selected page of linked findings.
    pub findings: Vec<FindingView>,
    /// Indicates that an older findings page is available.
    pub findings_has_more: bool,
    /// Selects the next older findings page.
    pub findings_next_cursor: Option<HistoryCursor>,
    /// Contains all current milestones in display order.
    pub milestones: Vec<MilestoneView>,
    /// Contains all current immutable assignment references.
    pub assignment_references: Vec<AssignmentReferenceView>,
    /// Contains the selected page of verification attempts.
    pub verification_attempts: Vec<VerificationAttemptView>,
    /// Indicates that an older verification page is available.
    pub verification_has_more: bool,
    /// Selects the next older verification page.
    pub verification_next_cursor: Option<HistoryCursor>,
    /// Contains the selected page of durable activity events.
    pub activity: Vec<ActivityView>,
    /// Indicates that an older activity page is available.
    pub activity_has_more: bool,
    /// Selects the next older activity page.
    pub activity_next_cursor: Option<HistoryCursor>,
}

/// Describes a server-confirmed finding that can be linked to a POA&M.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibleFinding {
    /// Identifies the stable finding.
    pub finding_id: Uuid,
    /// Identifies the affected system.
    pub system_id: Uuid,
    /// Contains the system display hostname.
    pub hostname: String,
    /// Identifies the system environment when assigned.
    pub environment_id: Option<Uuid>,
    /// Identifies the policy lineage.
    pub policy_lineage_id: Uuid,
    /// Contains the policy display name.
    pub policy_name: String,
    /// Identifies the current composite assessment when one exists.
    pub assessment_id: Option<Uuid>,
    /// Contains the current source assessment outcome.
    pub outcome: Option<AssessmentOutcome>,
}

/// Contains server-computed finding coverage and lifecycle counts for one scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rollup {
    /// Identifies the system or bundle scope.
    pub scope_id: Uuid,
    /// Counts visible POA&Ms in all lifecycle states.
    pub total: i64,
    /// Counts visible non-completed POA&Ms.
    pub active: i64,
    /// Counts active POA&Ms past their target date.
    pub overdue: i64,
    /// Counts POA&Ms waiting for verification.
    pub awaiting_verification: i64,
    /// Counts completed POA&Ms.
    pub completed: i64,
    /// Counts currently failing findings in the scope.
    pub open_findings: i64,
    /// Counts open findings assigned to an active POA&M.
    pub on_poam_findings: i64,
    /// Counts open findings without an active POA&M.
    pub no_poam_findings: i64,
}

/// Summarizes one finding result in a verification response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationResultSummary {
    /// Identifies the verified stable finding.
    pub finding_id: Uuid,
    /// Contains the normalized verification result.
    pub result: VerificationResult,
    /// Identifies the source assessment when one exists.
    pub assessment_id: Option<Uuid>,
    /// Identifies the accepted waiver when one exists.
    pub waiver_id: Option<Uuid>,
}

/// Contains the result of requesting POA&M verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyPoamResponse {
    /// Identifies the committed verification attempt.
    pub attempt_id: Uuid,
    /// Contains the aggregate server decision.
    pub outcome: VerificationOutcome,
    /// Contains the POA&M revision after verification was committed.
    pub revision: i64,
    /// Contains the committed result for each active finding.
    pub items: Vec<VerificationResultSummary>,
}

/// Describes active and historical POA&M relationships for one finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingRelationshipEntry {
    /// Identifies the composite assessment used by compatibility callers.
    pub assessment_id: Option<Uuid>,
    /// Identifies the stable finding.
    pub finding_id: Uuid,
    /// Contains the one active remediation plan when one exists.
    #[serde(rename = "active_poam", alias = "active")]
    pub active: Option<PoamSummary>,
    /// Contains the selected page of completed historical plans.
    #[serde(rename = "historical_poams", alias = "history")]
    pub history: Vec<PoamSummary>,
    /// Indicates that another historical page is available.
    #[serde(default)]
    pub historical_has_more: bool,
    /// Provides the offset for the next historical page.
    #[serde(default)]
    pub historical_next_offset: Option<i64>,
}

/// Describes POA&Ms that reference one immutable assignment version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentRelationshipEntry {
    /// Identifies the immutable assignment version.
    pub assignment_version_id: Uuid,
    /// Contains the selected page of related POA&Ms.
    pub poams: Vec<PoamSummary>,
    /// Indicates that another related-POA&M page is available.
    #[serde(default)]
    pub poams_has_more: bool,
    /// Provides the offset for the next related-POA&M page.
    #[serde(default)]
    pub poams_next_offset: Option<i64>,
}

/// Selects and paginates visible POA&Ms.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoamListQuery {
    /// Filters by exact lifecycle state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PoamStatus>,
    /// Filters by exact risk category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<PoamRisk>,
    /// Filters by owner label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Filters to findings on one system.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_id: Option<Uuid>,
    /// Filters to findings from one policy lineage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_lineage_id: Option<Uuid>,
    /// Filters to findings related to one bundle lineage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<Uuid>,
    /// Filters by server-recognized requirement identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
    /// Filters to overdue or non-overdue plans.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue: Option<bool>,
    /// Searches human ID, title, plan, and owner fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    /// Limits rows in the requested page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    /// Selects the zero-based page offset.
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
    /// Contains the remediation title.
    pub title: String,
    /// Contains the remediation plan text.
    pub plan: String,
    /// Contains the responsible owner label.
    pub owner: String,
    /// Sets the planned completion date when provided.
    pub target_date: Option<NaiveDate>,
    /// Sets the persisted remediation risk.
    pub risk: PoamRisk,
    /// Requests creation of the server-standard milestone sequence.
    pub default_milestones: bool,
    /// Adds supplemental immutable assignment version references.
    pub assignment_version_ids: Vec<Uuid>,
}

/// Applies optimistic-concurrency updates to POA&M plan metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePoamRequest {
    /// Must equal the current server revision.
    pub revision: i64,
    /// Replaces the title when present.
    pub title: Option<String>,
    /// Replaces the remediation plan when present.
    pub plan: Option<String>,
    /// Replaces the owner label when present.
    pub owner: Option<String>,
    /// Preserves, clears, or sets the target date through nested option semantics.
    pub target_date: Option<Option<NaiveDate>>,
    /// Replaces the risk category when present.
    pub risk: Option<PoamRisk>,
}

/// Requests one server-validated POA&M lifecycle transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionPoamRequest {
    /// Must equal the current server revision.
    pub revision: i64,
    /// Selects the requested destination state.
    pub status: PoamStatus,
    /// Adds an optional durable transition note.
    pub note: Option<String>,
}

/// Supplies optimistic concurrency for a POA&M action without another payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionRequest {
    /// Must equal the current server revision.
    pub revision: i64,
}

/// Adds one durable note to a POA&M.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddNoteRequest {
    /// Must equal the current server revision.
    pub revision: i64,
    /// Contains the note text to append.
    pub text: String,
}

/// Adds one milestone to a POA&M.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddMilestoneRequest {
    /// Must equal the current server revision.
    pub revision: i64,
    /// Contains the milestone title.
    pub title: String,
    /// Sets the milestone target date.
    pub target_date: NaiveDate,
}

/// Applies optimistic-concurrency updates to one POA&M milestone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateMilestoneRequest {
    /// Must equal the current server revision.
    pub revision: i64,
    /// Replaces the milestone title when present.
    pub title: Option<String>,
    /// Replaces the milestone target date when present.
    pub target_date: Option<NaiveDate>,
    /// Sets the milestone completion state when present.
    pub completed: Option<bool>,
}

/// Serializes one authoritative failing observation into a link request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFindingRequest {
    /// Must equal the current server revision.
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

/// Adds an immutable assignment version reference to a POA&M.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentReferenceRequest {
    /// Must equal the current server revision.
    pub revision: i64,
    /// Identifies the immutable assignment version to reference.
    pub assignment_version_id: Uuid,
}

/// Contains a structured error returned by the POA&M server API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoamServerError {
    /// Contains the HTTP response status outside the serialized body.
    #[serde(skip)]
    pub status: u16,
    /// Contains the stable server error code.
    #[serde(rename = "error")]
    pub code: String,
    /// Contains the user-presentable server explanation.
    pub message: String,
    /// Contains error-specific structured context when supplied.
    pub details: Option<Value>,
}

/// Represents transport, contract, and structured server failures.
#[derive(Debug, Clone, PartialEq)]
pub enum PoamApiError {
    /// Indicates that the browser request could not complete.
    Network(String),
    /// Indicates request serialization or response contract failure.
    Deserialize(String),
    /// Contains a non-success response from the server.
    Server(PoamServerError),
}

impl PoamApiError {
    /// Returns whether authentication has expired.
    pub fn is_unauthenticated(&self) -> bool {
        matches!(self, Self::Server(error) if error.status == 401)
    }

    /// Returns whether the authenticated user lacks permission.
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Server(error) if error.status == 403)
    }

    /// Returns whether the server intentionally hides or lacks the resource.
    pub fn is_not_visible(&self) -> bool {
        matches!(self, Self::Server(error) if error.status == 404)
    }

    /// Returns whether optimistic concurrency rejected a stale revision.
    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Server(error) if error.code == "stale_revision")
    }

    /// Returns whether another active remediation already owns the finding.
    pub fn is_active_remediation(&self) -> bool {
        matches!(self, Self::Server(error) if error.code == "finding_already_managed")
    }

    /// Returns whether a server precondition prevented the requested action.
    pub fn is_precondition(&self) -> bool {
        matches!(self, Self::Server(error) if error.status == 412)
    }

    /// Returns whether the response represents an internal server failure.
    pub fn is_internal(&self) -> bool {
        matches!(self, Self::Server(error) if error.status >= 500 || error.code == "internal_error")
    }

    /// Decodes verification details from a failed closure precondition.
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

/// Describes the authoritative verification that prevented POA&M closure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosePreconditionDetails {
    /// Identifies the committed rejected verification attempt.
    pub attempt_id: Uuid,
    /// Contains the server revision committed by that verification.
    #[serde(alias = "revision")]
    pub committed_revision: i64,
    /// Contains the finding results that prevented closure.
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
    redirect_on_authentication_expiration(status);
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

fn redirect_on_authentication_expiration(status: u16) {
    if !is_authentication_expiration(status) {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        // A full navigation re-enters the application authentication bootstrap
        // and clears all account-scoped component state after session expiry.
        let _ = window.location().set_href("/login");
    }
}

const fn is_authentication_expiration(status: u16) -> bool {
    status == 401
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

/// Fetches one validated page of server-visible POA&Ms.
///
/// # Errors
///
/// Returns [`PoamApiError`] when query serialization or the request fails, the
/// response cannot be decoded, or pagination metadata is incoherent.
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

/// Fetches every POA&M page selected by a query.
///
/// The function clamps page size to the server limit and follows only
/// validated cursors.
///
/// # Errors
///
/// Returns [`PoamApiError`] under the conditions documented by [`list_poams`].
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

/// Fetches one POA&M with independently paginated related history.
///
/// # Errors
///
/// Returns [`PoamApiError`] when query serialization or the request fails, or
/// when the response does not match the detail contract.
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
        #[doc = concat!("Sends the `", stringify!($name), "` POA&M mutation.")]
        ///
        /// The request body carries the optimistic concurrency revision where
        /// required by the endpoint.
        ///
        /// # Errors
        ///
        /// Returns [`PoamApiError`] when request serialization or transport
        /// fails, the server rejects the mutation, or the response cannot be
        /// decoded.
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

/// Creates a POA&M for one server-authoritative failing observation.
///
/// # Errors
///
/// Returns [`PoamApiError`] when request serialization or transport fails, the
/// server rejects the finding identity or mutation, or decoding fails.
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

/// Updates one milestone using the containing POA&M revision.
///
/// # Errors
///
/// Returns [`PoamApiError`] when request serialization or transport fails, the
/// server rejects the revision or mutation, or decoding fails.
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

/// Removes one milestone using optimistic concurrency.
///
/// # Errors
///
/// Returns [`PoamApiError`] when transport fails, the server rejects the
/// revision or mutation, or decoding fails.
pub async fn remove_poam_milestone(
    id: Uuid,
    milestone_id: Uuid,
    revision: i64,
) -> Result<PoamDetail, PoamApiError> {
    revision_delete(format!("/poams/{id}/milestones/{milestone_id}"), revision).await
}

/// Retires one finding relationship using optimistic concurrency.
///
/// # Errors
///
/// Returns [`PoamApiError`] when transport fails, the server rejects the
/// revision or mutation, or decoding fails.
pub async fn unlink_poam_finding(
    id: Uuid,
    finding_id: Uuid,
    revision: i64,
) -> Result<PoamDetail, PoamApiError> {
    revision_delete(format!("/poams/{id}/findings/{finding_id}"), revision).await
}

/// Removes one assignment reference using optimistic concurrency.
///
/// # Errors
///
/// Returns [`PoamApiError`] when transport fails, the server rejects the
/// revision or mutation, or decoding fails.
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

/// Searches findings that the server permits this POA&M to link.
///
/// The client clamps the requested page bounds but does not decide finding
/// compatibility.
///
/// # Errors
///
/// Returns [`PoamApiError`] when query serialization or transport fails, the
/// server rejects the request, decoding fails, or pagination is incoherent.
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

/// Searches active POA&Ms compatible with one authoritative finding.
///
/// Composite callers supply `assessment_id`. Source-neutral callers supply
/// `finding_id` and `observation`; the server remains the compatibility
/// authority.
///
/// # Errors
///
/// Returns [`PoamApiError`] when query serialization or transport fails, the
/// server rejects the identity, decoding fails, or pagination is incoherent.
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

/// Fetches server-computed POA&M roll-ups for system IDs in bounded batches.
///
/// # Errors
///
/// Returns [`PoamApiError`] when any batch request fails or cannot be decoded.
pub async fn system_rollups(ids: &[Uuid]) -> Result<Vec<Rollup>, PoamApiError> {
    fetch_batches("/poams/rollups/systems", "ids", ids).await
}

/// Fetches server-computed POA&M roll-ups for bundle IDs in bounded batches.
///
/// # Errors
///
/// Returns [`PoamApiError`] when any batch request fails or cannot be decoded.
pub async fn bundle_rollups(ids: &[Uuid]) -> Result<Vec<Rollup>, PoamApiError> {
    fetch_batches("/poams/rollups/bundles", "ids", ids).await
}

/// Fetches complete finding relationships for composite assessment IDs.
///
/// # Errors
///
/// Returns [`PoamApiError`] when a request fails, response cursors are
/// incoherent, active identity changes during pagination, or the safe history
/// bound is exceeded.
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

/// Fetches complete finding relationships for stable finding IDs.
///
/// # Errors
///
/// Returns [`PoamApiError`] under the conditions documented by
/// [`finding_relationships`].
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

/// Fetches complete POA&M relationships for immutable assignment versions.
///
/// # Errors
///
/// Returns [`PoamApiError`] when a request fails, response cursors are
/// incoherent, or the safe history bound is exceeded.
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
        assert!(is_authentication_expiration(401));
        assert!(!is_authentication_expiration(403));
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
                401,
                r#"{"error":"unauthenticated","message":"expired","details":null}"#
            )
            .unwrap_err()
            .is_unauthenticated()
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
            !parse_response::<PoamDetail>(
                403,
                r#"{"error":"forbidden","message":"denied","details":null}"#
            )
            .unwrap_err()
            .is_unauthenticated()
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
    fn linked_finding_requirement_metadata_is_additive_and_rolling_compatible() {
        let base = serde_json::json!({
            "id": Uuid::from_u128(1),
            "system_id": Uuid::from_u128(2),
            "hostname": "host",
            "environment_id": null,
            "policy_lineage_id": Uuid::from_u128(3),
            "policy_name": "Policy",
            "link_id": Uuid::from_u128(4),
            "linked_at": "2026-08-31T00:00:00Z",
            "linked_by": Uuid::from_u128(5),
            "retired_at": null,
            "retired_by": null,
            "retirement_reason": null,
            "link_active": true,
            "current_assessment_id": null,
            "current_outcome": "fail",
            "current_policy_version_id": null,
            "current_target_store_path": null,
            "assessment_updated_at": null,
            "resolution_state": "fail",
            "effective_set_digest": null,
            "effective_config_digest": null,
            "bundle_ids": [],
            "bundle_version_ids": [],
            "requirement_version_ids": [Uuid::from_u128(6)]
        });
        let legacy: FindingView = serde_json::from_value(base.clone()).unwrap();
        assert!(legacy.requirements.is_empty());

        let mut enriched = base;
        enriched["requirements"] = serde_json::json!([{
            "requirement_version_id": Uuid::from_u128(6),
            "external_id": "AC-2",
            "title": "Account Management",
            "framework_id": Uuid::from_u128(7),
            "framework_name": "NIST SP 800-53",
            "framework_version_id": Uuid::from_u128(8),
            "framework_version": "Rev. 5",
            "framework_title": "Security and Privacy Controls"
        }]);
        let enriched: FindingView = serde_json::from_value(enriched).unwrap();
        assert_eq!(enriched.requirements[0].external_id, "AC-2");
        assert_eq!(enriched.requirements[0].framework_version, "Rev. 5");
    }

    #[test]
    fn verification_identity_is_additive_and_rolling_compatible() {
        let mut value = serde_json::json!({
            "attempt_id": Uuid::from_u128(1),
            "finding_id": Uuid::from_u128(2),
            "system_id": Uuid::from_u128(3),
            "policy_lineage_id": Uuid::from_u128(4),
            "result": "fail",
            "policy_version_id": Uuid::from_u128(5),
            "assessment_id": null,
            "derivation_id": null,
            "target_store_path": null,
            "effective_set_digest": null,
            "effective_config_digest": null,
            "effective_config": null,
            "observed_outcome": "fail",
            "observation_token": null,
            "observation_snapshot": null,
            "assessment_updated_at": null,
            "bundle_ids": [],
            "bundle_version_ids": [],
            "requirement_version_ids": [Uuid::from_u128(6)],
            "waiver_id": null,
            "observed_at": "2026-08-31T00:00:00Z",
            "detail": "Still failing"
        });
        let legacy: VerificationItemView = serde_json::from_value(value.clone()).unwrap();
        assert!(legacy.hostname.is_none());
        assert!(legacy.policy_name.is_none());
        assert!(legacy.requirements.is_empty());

        value["hostname"] = serde_json::json!("host-a");
        value["policy_name"] = serde_json::json!("Account policy");
        value["policy_version"] = serde_json::json!("7");
        value["requirements"] = serde_json::json!([{
            "requirement_version_id": Uuid::from_u128(6),
            "external_id": "AC-2",
            "title": "Account Management",
            "framework_id": Uuid::from_u128(7),
            "framework_name": "NIST SP 800-53",
            "framework_version_id": Uuid::from_u128(8),
            "framework_version": "Rev. 5",
            "framework_title": null
        }]);
        let enriched: VerificationItemView = serde_json::from_value(value).unwrap();
        assert_eq!(enriched.hostname.as_deref(), Some("host-a"));
        assert_eq!(enriched.policy_name.as_deref(), Some("Account policy"));
        assert_eq!(enriched.policy_version.as_deref(), Some("7"));
        assert_eq!(enriched.requirements[0].external_id, "AC-2");
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
