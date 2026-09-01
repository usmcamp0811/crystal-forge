//! Defines POA&M lifecycle requests, durable views, and pagination contracts.
//!
//! A POA&M tracks remediation for stable findings. Mutations use optimistic
//! revisions, and closure retains immutable verification evidence.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents the persisted lifecycle state of a POA&M.
///
/// `Open`, `InProgress`, and `Blocked` can transition among each other or to
/// `AwaitingVerification`. `AwaitingVerification` can return to `InProgress`
/// or `Blocked`. Only authoritative closure can enter `Completed`, and only
/// the reopen operation can leave `Completed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoamStatus {
    /// Indicates remediation has not started.
    Open,
    /// Indicates active remediation work.
    InProgress,
    /// Indicates remediation cannot proceed until an external blocker clears.
    Blocked,
    /// Indicates remediation is ready for authoritative verification.
    AwaitingVerification,
    /// Indicates authoritative verification passed and closed the POA&M.
    Completed,
}

impl PoamStatus {
    /// Returns the stable database and API representation of the status.
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

/// Classifies the remediation risk assigned to a POA&M.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoamRisk {
    /// Requires the highest remediation priority.
    High,
    /// Requires normal remediation priority.
    Medium,
    /// Permits the lowest remediation priority.
    Low,
}

impl PoamRisk {
    /// Returns the stable database and API representation of the risk.
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
/// Callers provide `assessment_id` for composite-assessment compatibility or
/// provide `finding_id` and `observation` together for source-neutral evidence.
/// Mixing or omitting these forms is invalid.
#[derive(Debug, Deserialize)]
pub struct CreatePoamRequest {
    /// Identifies a current composite assessment when using the compatibility API.
    #[serde(default)]
    pub assessment_id: Option<Uuid>,
    /// Identifies the stable finding when using source-neutral evidence.
    #[serde(default)]
    pub finding_id: Option<Uuid>,
    /// Binds a stable finding to the authoritative observation shown to the user.
    #[serde(default)]
    pub observation: Option<FindingObservationReference>,
    /// Gives the operator-facing remediation title.
    pub title: String,
    /// Gives the remediation plan; an empty value records no plan yet.
    #[serde(default)]
    pub plan: String,
    /// Identifies the responsible person or team; an empty value is unassigned.
    #[serde(default)]
    pub owner: String,
    /// Gives the planned completion date when one has been selected.
    pub target_date: Option<NaiveDate>,
    /// Classifies the remediation risk.
    pub risk: PoamRisk,
    /// Requests the server's standard milestone set when true.
    #[serde(default = "default_true")]
    pub default_milestones: bool,
    /// Links immutable assignment versions that define the remediation scope.
    #[serde(default)]
    pub assignment_version_ids: Vec<Uuid>,
}

fn default_true() -> bool {
    true
}

/// Requests an optimistic update to editable POA&M attributes.
#[derive(Debug, Default, Deserialize)]
pub struct UpdatePoamRequest {
    /// Requires the current persisted revision to prevent lost updates.
    pub revision: i64,
    /// Replaces the title when present.
    pub title: Option<String>,
    /// Replaces the remediation plan when present.
    pub plan: Option<String>,
    /// Replaces the responsible owner when present.
    pub owner: Option<String>,
    /// Replaces, clears, or preserves the target date.
    pub target_date: Option<Option<NaiveDate>>,
    /// Replaces the risk classification when present.
    pub risk: Option<PoamRisk>,
}

/// Requests one validated POA&M lifecycle transition.
#[derive(Debug, Deserialize)]
pub struct TransitionPoamRequest {
    /// Requires the current persisted revision to prevent lost transitions.
    pub revision: i64,
    /// Selects the requested destination state.
    pub status: PoamStatus,
    /// Records optional operator context with the transition activity.
    pub note: Option<String>,
}

/// Supplies an optimistic revision for a mutation with no other input.
#[derive(Debug, Deserialize)]
pub struct RevisionRequest {
    /// Requires the current persisted revision to prevent lost updates.
    pub revision: i64,
}

/// Requests linking one authoritative failing observation to an existing POA&M.
///
/// The identity rules match [`CreatePoamRequest`].
#[derive(Debug, Deserialize)]
pub struct AddFindingRequest {
    /// Requires the current POA&M revision to prevent lost links.
    pub revision: i64,
    /// Identifies a current composite assessment when using the compatibility API.
    #[serde(default)]
    pub assessment_id: Option<Uuid>,
    /// Identifies the stable finding when using source-neutral evidence.
    #[serde(default)]
    pub finding_id: Option<Uuid>,
    /// Binds a stable finding to the authoritative observation shown to the user.
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

/// Requests a durable operator note on a POA&M.
#[derive(Debug, Deserialize)]
pub struct AddNoteRequest {
    /// Requires the current POA&M revision to prevent lost activity.
    pub revision: i64,
    /// Gives the note text retained in activity history.
    pub text: String,
}

/// Requests a new dated remediation milestone.
#[derive(Debug, Deserialize)]
pub struct AddMilestoneRequest {
    /// Requires the current POA&M revision to prevent lost milestones.
    pub revision: i64,
    /// Gives the milestone's operator-facing title.
    pub title: String,
    /// Gives the planned milestone completion date.
    pub target_date: NaiveDate,
}

/// Requests changes to one existing remediation milestone.
#[derive(Debug, Deserialize)]
pub struct UpdateMilestoneRequest {
    /// Requires the current POA&M revision to prevent lost updates.
    pub revision: i64,
    /// Replaces the milestone title when present.
    pub title: Option<String>,
    /// Replaces the target date when present.
    pub target_date: Option<NaiveDate>,
    /// Marks the milestone complete or incomplete when present.
    pub completed: Option<bool>,
}

/// Requests a link between a POA&M and an immutable assignment version.
#[derive(Debug, Deserialize)]
pub struct AssignmentReferenceRequest {
    /// Requires the current POA&M revision to prevent lost links.
    pub revision: i64,
    /// Identifies the assignment version to add or remove.
    pub assignment_version_id: Uuid,
}

/// Requests a waiver for one exact failing finding observation.
///
/// The server revalidates the assessment or source-neutral observation before
/// creating the waiver.
#[derive(Debug, Deserialize)]
pub struct CreateWaiverRequest {
    /// Identifies the stable finding receiving the waiver.
    pub finding_id: Uuid,
    /// Identifies composite evidence for a composite-assessment finding.
    pub assessment_id: Option<Uuid>,
    /// Identifies source-neutral evidence for a non-composite finding.
    pub observation: Option<FindingObservationReference>,
    /// Explains why the observed failure can be accepted.
    pub justification: String,
}

/// Requests an authorized decision on a pending waiver.
#[derive(Debug, Deserialize)]
pub struct WaiverDecisionRequest {
    /// Selects the new waiver decision state.
    pub status: WaiverDecision,
    /// Sets the accepted waiver's expiration time when applicable.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Represents a terminal or effective waiver decision.
///
/// A pending waiver can become `Accepted` or `Rejected`. An accepted waiver
/// can become `Revoked` or `Expired`. Rejected, revoked, and expired decisions
/// are terminal, and no decision can return to pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaiverDecision {
    /// Allows the exact waived observation until optional expiration.
    Accepted,
    /// Denies the waiver request.
    Rejected,
    /// Removes a previously accepted waiver.
    Revoked,
    /// Indicates an accepted waiver passed its expiration time.
    Expired,
}

impl WaiverDecision {
    /// Returns the stable database and API representation of the decision.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
        }
    }
}

/// Filters and bounds a waiver list query.
#[derive(Debug, Default, Deserialize)]
pub struct WaiverListQuery {
    /// Filters by normalized waiver status when present.
    pub status: Option<String>,
    /// Filters waivers to one stable finding.
    pub finding_id: Option<Uuid>,
    /// Requests a bounded page size.
    pub limit: Option<i64>,
    /// Selects the zero-based page offset.
    pub offset: Option<i64>,
}

/// Reports one durable waiver and the observation it authorizes.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct WaiverView {
    /// Identifies the waiver.
    pub id: Uuid,
    /// Identifies the stable finding receiving the waiver.
    pub finding_id: Uuid,
    /// Identifies the system on which the finding was observed.
    pub system_id: Uuid,
    /// Identifies the stable policy lineage behind the finding.
    pub policy_lineage_id: Uuid,
    /// Gives the normalized waiver lifecycle status.
    pub status: String,
    /// Explains why the observation can be accepted.
    pub justification: String,
    /// Identifies the immutable policy version that produced the observation.
    pub policy_version_id: Uuid,
    /// Identifies composite assessment evidence when applicable.
    pub assessment_id: Option<Uuid>,
    /// Binds the waiver to the exact semantic observation.
    pub observation_token: String,
    /// Preserves the authoritative observation accepted by the waiver.
    pub observation_snapshot: serde_json::Value,
    /// Identifies the user who accepted the waiver.
    pub accepted_by: Option<Uuid>,
    /// Records when the waiver was accepted.
    pub accepted_at: Option<DateTime<Utc>>,
    /// Records when an accepted waiver stops applying.
    pub expires_at: Option<DateTime<Utc>>,
    /// Identifies the user who requested the waiver.
    pub created_by: Uuid,
    /// Records when the waiver was requested.
    pub created_at: DateTime<Utc>,
    /// Records the most recent waiver state change.
    pub updated_at: DateTime<Utc>,
}

/// Filters, searches, and bounds a POA&M list query.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PoamListQuery {
    /// Filters by normalized lifecycle status.
    pub status: Option<String>,
    /// Filters by normalized risk classification.
    pub risk: Option<String>,
    /// Filters by responsible owner.
    pub owner: Option<String>,
    /// Filters to POA&Ms with findings on one system.
    pub system_id: Option<Uuid>,
    /// Filters to POA&Ms for one stable policy lineage.
    pub policy_lineage_id: Option<Uuid>,
    /// Filters to POA&Ms linked to one compliance bundle lineage.
    pub bundle_id: Option<Uuid>,
    /// Filters by framework requirement identifier or title.
    pub requirement: Option<String>,
    /// Filters by computed overdue state when present.
    pub overdue: Option<bool>,
    /// Searches human ID, title, plan, owner, and finding metadata.
    pub q: Option<String>,
    /// Requests a bounded page size.
    pub limit: Option<i64>,
    /// Selects the zero-based page offset.
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

/// Summarizes one POA&M for lists and relationship responses.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PoamSummary {
    /// Identifies the POA&M internally.
    pub id: Uuid,
    /// Gives the stable operator-facing POA&M identifier.
    pub human_id: String,
    /// Gives the remediation title.
    pub title: String,
    /// Gives the current remediation plan.
    pub plan: String,
    /// Identifies the responsible person or team.
    pub owner: String,
    /// Gives the planned completion date.
    pub target_date: Option<NaiveDate>,
    /// Gives the normalized risk classification.
    pub risk: String,
    /// Gives the normalized lifecycle status.
    pub status: String,
    /// Gives the optimistic concurrency revision.
    pub revision: i64,
    /// Indicates that an incomplete POA&M is past its target date.
    pub overdue: bool,
    /// Counts active findings, or the closure finding set for a completed POA&M.
    pub finding_count: i64,
    /// Records when the POA&M was created.
    pub created_at: DateTime<Utc>,
    /// Records the most recent POA&M mutation.
    pub updated_at: DateTime<Utc>,
    /// Records when successful verification closed the POA&M.
    pub closed_at: Option<DateTime<Utc>>,
    /// Identifies the verification attempt that closed the POA&M.
    pub closure_attempt_id: Option<Uuid>,
}

/// Reports active and historical POA&M links for one finding.
#[derive(Debug, Clone, Serialize)]
pub struct FindingPoamRelationship {
    /// Identifies the compatible composite assessment when applicable.
    pub assessment_id: Option<Uuid>,
    /// Identifies the stable finding.
    pub finding_id: Uuid,
    /// Gives the single active remediation, if one exists.
    pub active_poam: Option<PoamSummary>,
    /// Gives the requested page of inactive historical remediations.
    pub historical_poams: Vec<PoamSummary>,
    /// Indicates that another historical page is available.
    pub historical_has_more: bool,
    /// Provides the offset for the next historical page.
    pub historical_next_offset: Option<i64>,
}

/// Reports POA&Ms related to one immutable assignment version.
#[derive(Debug, Clone, Serialize)]
pub struct AssignmentPoamRelationship {
    /// Identifies the assignment version used for the relationship.
    pub assignment_version_id: Uuid,
    /// Gives the requested page of related POA&Ms.
    pub poams: Vec<PoamSummary>,
    /// Indicates that another related-POA&M page is available.
    pub poams_has_more: bool,
    /// Provides the offset for the next related-POA&M page.
    pub poams_next_offset: Option<i64>,
}

/// Identifies one authoritative requirement release linked to a finding.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FindingRequirementView {
    /// Identifies the requirement version retained in compatibility arrays.
    pub requirement_version_id: Uuid,
    /// Gives the requirement identifier published by the framework release.
    pub external_id: String,
    /// Gives the optional human-readable requirement title.
    pub title: Option<String>,
    /// Identifies the authoritative framework lineage.
    pub framework_id: Uuid,
    /// Gives the human-readable framework name.
    pub framework_name: String,
    /// Identifies the immutable framework release.
    pub framework_version_id: Uuid,
    /// Gives the human-readable framework release version.
    pub framework_version: String,
    /// Gives the optional human-readable framework release title.
    pub framework_title: Option<String>,
}

/// Reports one finding link and its current authoritative evidence identity.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FindingView {
    /// Identifies the stable finding.
    pub id: Uuid,
    /// Identifies the affected system.
    pub system_id: Uuid,
    /// Gives the system hostname used for display.
    pub hostname: String,
    /// Identifies the system environment when assigned.
    pub environment_id: Option<Uuid>,
    /// Identifies the stable policy lineage behind the finding.
    pub policy_lineage_id: Uuid,
    /// Gives the current display name of the policy lineage.
    pub policy_name: String,
    /// Identifies this POA&M-to-finding link.
    pub link_id: Uuid,
    /// Records when the finding was linked.
    pub linked_at: DateTime<Utc>,
    /// Identifies the user who linked the finding.
    pub linked_by: Uuid,
    /// Records when this link stopped being active.
    pub retired_at: Option<DateTime<Utc>>,
    /// Identifies the user who retired the link.
    pub retired_by: Option<Uuid>,
    /// Explains why the link was retired.
    pub retirement_reason: Option<String>,
    /// Indicates whether this link participates in current remediation.
    pub link_active: bool,
    /// Identifies the current composite assessment when applicable.
    pub current_assessment_id: Option<Uuid>,
    /// Gives the current normalized evidence outcome.
    pub current_outcome: Option<String>,
    /// Identifies the policy version used by current evidence.
    pub current_policy_version_id: Option<Uuid>,
    /// Identifies the deployment target assessed by current evidence.
    pub current_target_store_path: Option<String>,
    /// Records when the current assessment last changed.
    pub assessment_updated_at: Option<DateTime<Utc>>,
    /// Indicates whether current evidence is open, waived, or resolved.
    pub resolution_state: String,
    /// Binds current evidence to the effective policy-version set.
    pub effective_set_digest: Option<String>,
    /// Binds current evidence to the effective policy configuration.
    pub effective_config_digest: Option<String>,
    /// Identifies related compliance bundle lineages.
    pub bundle_ids: Vec<Uuid>,
    /// Identifies related immutable bundle versions.
    pub bundle_version_ids: Vec<Uuid>,
    /// Retains immutable requirement IDs for compatibility clients.
    pub requirement_version_ids: Vec<Uuid>,
    /// Provides authoritative display metadata for available requirement versions.
    pub requirements: sqlx::types::Json<Vec<FindingRequirementView>>,
}

/// Reports one ordered remediation milestone.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MilestoneView {
    /// Identifies the milestone.
    pub id: Uuid,
    /// Determines milestone display order within the POA&M.
    pub ordinal: i32,
    /// Gives the milestone title.
    pub title: String,
    /// Gives the planned completion date.
    pub target_date: NaiveDate,
    /// Records when the milestone was completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Identifies the user who completed the milestone.
    pub completed_by: Option<Uuid>,
    /// Identifies the user who created the milestone.
    pub created_by: Uuid,
    /// Identifies the user who most recently changed the milestone.
    pub updated_by: Uuid,
    /// Records when the milestone was created.
    pub created_at: DateTime<Utc>,
    /// Records the most recent milestone change.
    pub updated_at: DateTime<Utc>,
}

/// Reports one immutable assignment-version reference attached to a POA&M.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AssignmentReferenceView {
    /// Identifies the assignment lineage.
    pub assignment_id: Uuid,
    /// Identifies the exact assignment version retained by the POA&M.
    pub assignment_version_id: Uuid,
    /// Identifies the user who added the reference.
    pub added_by: Uuid,
    /// Records when the reference was added.
    pub added_at: DateTime<Utc>,
    /// Identifies the assigned bundle lineage.
    pub bundle_id: Uuid,
    /// Identifies the assigned immutable bundle version.
    pub bundle_version_id: Uuid,
    /// Gives the bundle name used for display.
    pub bundle_name: String,
    /// Gives the bundle version label used for display.
    pub bundle_version: String,
    /// Identifies a directly assigned system.
    pub system_id: Option<Uuid>,
    /// Gives the directly assigned system hostname.
    pub system_hostname: Option<String>,
    /// Identifies a directly assigned environment.
    pub environment_id: Option<Uuid>,
    /// Gives the directly assigned environment name.
    pub environment_name: Option<String>,
}

/// Reports one immutable POA&M verification attempt and its finding results.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct VerificationAttemptView {
    /// Identifies the verification attempt.
    pub id: Uuid,
    /// Gives the normalized aggregate verification outcome.
    pub outcome: String,
    /// Records the POA&M revision that the attempt verified.
    pub poam_revision: i64,
    /// Identifies the user who requested verification.
    pub attempted_by: Uuid,
    /// Records when verification ran.
    pub attempted_at: DateTime<Utc>,
    /// Reports the immutable per-finding observations in the attempt.
    pub items: Vec<VerificationItemView>,
}

/// Reports one immutable finding observation from a verification attempt.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct VerificationItemView {
    /// Identifies the parent verification attempt.
    pub attempt_id: Uuid,
    /// Identifies the stable finding that was verified.
    pub finding_id: Uuid,
    /// Identifies the system that supplied the evidence.
    pub system_id: Uuid,
    /// Contains the captured hostname, or the current hostname for legacy rows.
    pub hostname: String,
    /// Identifies the stable policy lineage behind the finding.
    pub policy_lineage_id: Uuid,
    /// Contains the immutable policy-version name when available.
    pub policy_name: String,
    /// Contains the immutable policy version label when available.
    pub policy_version: Option<String>,
    /// Gives the normalized per-finding verification result.
    pub result: String,
    /// Identifies the immutable policy version that was verified.
    pub policy_version_id: Option<Uuid>,
    /// Identifies the composite assessment when applicable.
    pub assessment_id: Option<Uuid>,
    /// Identifies the derivation used by source-neutral evidence.
    pub derivation_id: Option<i32>,
    /// Identifies the deployment target observed during verification.
    pub target_store_path: Option<String>,
    /// Binds evidence to the effective policy-version set.
    pub effective_set_digest: Option<String>,
    /// Binds evidence to the effective policy configuration.
    pub effective_config_digest: Option<String>,
    /// Preserves the effective composite configuration when applicable.
    pub effective_config: Option<serde_json::Value>,
    /// Gives the authoritative source outcome observed by verification.
    pub observed_outcome: Option<String>,
    /// Binds source-neutral evidence to exact semantics and deployment identity.
    pub observation_token: Option<String>,
    /// Preserves the source-neutral evidence used by this attempt.
    pub observation_snapshot: Option<serde_json::Value>,
    /// Records the source assessment timestamp used by verification.
    pub assessment_updated_at: Option<DateTime<Utc>>,
    /// Identifies related compliance bundle lineages.
    pub bundle_ids: Vec<Uuid>,
    /// Identifies related immutable bundle versions.
    pub bundle_version_ids: Vec<Uuid>,
    /// Retains immutable requirement IDs for compatibility clients.
    pub requirement_version_ids: Vec<Uuid>,
    /// Provides requirement display metadata hydrated from immutable version IDs.
    pub requirements: sqlx::types::Json<Vec<FindingRequirementView>>,
    /// Identifies the accepted waiver applied to the observation.
    pub waiver_id: Option<Uuid>,
    /// Records when the authoritative evidence was observed.
    pub observed_at: DateTime<Utc>,
    /// Explains the per-finding verification result.
    pub detail: String,
}

/// Reports one durable POA&M activity event.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ActivityView {
    /// Identifies the activity event.
    pub id: Uuid,
    /// Identifies the acting user when the event was user initiated.
    pub actor_user_id: Option<Uuid>,
    /// Contains the current username or email for a known actor.
    pub actor_display: Option<String>,
    /// Gives the stable activity event kind.
    pub kind: String,
    /// Preserves event-specific audit data.
    pub payload: serde_json::Value,
    /// Records when the activity occurred.
    pub created_at: DateTime<Utc>,
}

/// Reports a POA&M with bounded independent evidence and activity histories.
#[derive(Debug, Serialize)]
pub struct PoamDetail {
    /// Provides the POA&M's current summary and optimistic revision.
    #[serde(flatten)]
    pub poam: PoamSummary,
    /// Gives the requested page of linked findings.
    pub findings: Vec<FindingView>,
    /// Indicates that an older findings page is available.
    pub findings_has_more: bool,
    /// Selects the next older findings page when present.
    pub findings_next_cursor: Option<HistoryCursor>,
    /// Gives all current ordered remediation milestones.
    pub milestones: Vec<MilestoneView>,
    /// Gives all immutable assignment-version references.
    pub assignment_references: Vec<AssignmentReferenceView>,
    /// Gives the requested page of verification attempts.
    pub verification_attempts: Vec<VerificationAttemptView>,
    /// Indicates that an older verification page is available.
    pub verification_has_more: bool,
    /// Selects the next older verification page when present.
    pub verification_next_cursor: Option<HistoryCursor>,
    /// Gives the requested page of durable activity events.
    pub activity: Vec<ActivityView>,
    /// Indicates that an older activity page is available.
    pub activity_has_more: bool,
    /// Selects the next older activity page when present.
    pub activity_next_cursor: Option<HistoryCursor>,
}

/// Reports a failing finding that can be linked to the selected POA&M.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CompatibleFinding {
    /// Identifies the stable finding.
    pub finding_id: Uuid,
    /// Identifies the affected system.
    pub system_id: Uuid,
    /// Gives the system hostname used for display.
    pub hostname: String,
    /// Identifies the system environment when assigned.
    pub environment_id: Option<Uuid>,
    /// Identifies the stable policy lineage behind the finding.
    pub policy_lineage_id: Uuid,
    /// Gives the policy lineage name used for display.
    pub policy_name: String,
    /// Identifies compatible composite evidence when applicable.
    pub assessment_id: Option<Uuid>,
    /// Gives the current authoritative evidence outcome.
    pub outcome: Option<String>,
}

/// Aggregates POA&M and finding counts for one requested scope.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Rollup {
    /// Identifies the system, environment, or bundle scope.
    pub scope_id: Uuid,
    /// Counts all POA&Ms in the scope.
    pub total: i64,
    /// Counts non-completed POA&Ms in the scope.
    pub active: i64,
    /// Counts active POA&Ms past their target date.
    pub overdue: i64,
    /// Counts POA&Ms waiting for authoritative verification.
    pub awaiting_verification: i64,
    /// Counts successfully closed POA&Ms.
    pub completed: i64,
    /// Counts currently failing findings in the scope.
    pub open_findings: i64,
    /// Counts open findings linked to an active POA&M.
    pub on_poam_findings: i64,
    /// Counts open findings without an active POA&M.
    pub no_poam_findings: i64,
}

/// Aggregates fleet-wide POA&M lifecycle counts for the dashboard.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DashboardSummary {
    /// Counts all POA&Ms visible to the caller.
    pub total: i64,
    /// Counts visible non-completed POA&Ms.
    pub active: i64,
    /// Counts visible active POA&Ms past their target date.
    pub overdue: i64,
    /// Counts visible POA&Ms waiting for verification.
    pub awaiting_verification: i64,
    /// Counts visible successfully closed POA&Ms.
    pub completed: i64,
}

/// Wraps one bounded offset page and its continuation metadata.
#[derive(Debug, Serialize)]
pub struct Page<T> {
    /// Gives the rows in this page.
    pub items: Vec<T>,
    /// Gives the effective bounded page size.
    pub limit: i64,
    /// Gives the zero-based offset of this page.
    pub offset: i64,
    /// Indicates that another page is available.
    pub has_more: bool,
    /// Gives the next page offset when another page is available.
    pub next_offset: Option<i64>,
}
