//! Domain models for deployment approval requests, decisions, and authorizations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A deployment approval request with full lifecycle tracking.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct DeploymentApprovalRequest {
    pub id: Uuid,
    pub system_id: Uuid,
    pub environment_id: Option<Uuid>,
    pub target_store_path: String,
    pub target_derivation_path: Option<String>,
    pub target_commit_id: Option<Uuid>,
    pub target_commit_hash: Option<String>,
    pub flake_id: Option<Uuid>,
    pub deployment_policy_id: Option<Uuid>,
    pub deployment_policy_version_id: Option<Uuid>,
    pub requester_kind: String,
    pub requested_by_user_id: Option<Uuid>,
    pub requested_by_automation: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub required_approvals: i32,
    pub required_role: Option<String>,
    pub distinct_approvers: bool,
    pub requester_may_approve: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub status: String,
    pub request_fingerprint: String,
    pub deployment_authorization_id: Option<Uuid>,
    pub superseded_by_id: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An approval or rejection decision against a request.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct DeploymentApprovalDecision {
    pub id: Uuid,
    pub request_id: Uuid,
    pub actor_user_id: Uuid,
    pub decision: String,
    pub note: Option<String>,
    pub actor_role_snapshot: Option<String>,
    pub request_fingerprint: String,
    pub status_before: String,
    pub status_after: String,
    pub created_at: DateTime<Utc>,
}

/// Immutable deployment authorization record.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct DeploymentAuthorization {
    pub id: Uuid,
    pub system_id: Uuid,
    pub target_store_path: String,
    pub target_derivation_path: Option<String>,
    pub target_commit_id: Option<Uuid>,
    pub policy_version_id: Option<Uuid>,
    pub source_approval_request_id: Option<Uuid>,
    pub authorization_source: String,
    pub issued_by_user_id: Option<Uuid>,
    pub issued_by_automation: Option<String>,
    pub issued_at: DateTime<Utc>,
    pub valid_from: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub deployment_execution_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Requester kind for deployment approval requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequesterKind {
    User,
    Automation,
}

impl RequesterKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Automation => "automation",
        }
    }
}

/// Authorization source type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationSource {
    Approval,
    PolicyBypass,
    OperatorAdopt,
    Automation,
}

impl AuthorizationSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::PolicyBypass => "policy_bypass",
            Self::OperatorAdopt => "operator_adopt",
            Self::Automation => "automation",
        }
    }
}

/// Summary counts for the approval dashboard widget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalSummary {
    pub pending: i64,
    pub waiting_more_than_one_hour: i64,
    pub requires_multiple_approvers: i64,
    pub partially_approved: i64,
}

/// Parameters for creating a new approval request.
#[derive(Debug, Clone)]
pub struct CreateApprovalRequest {
    pub system_id: Uuid,
    pub environment_id: Option<Uuid>,
    pub target_store_path: String,
    pub target_derivation_path: Option<String>,
    pub target_commit_id: Option<Uuid>,
    pub target_commit_hash: Option<String>,
    pub flake_id: Option<Uuid>,
    pub deployment_policy_id: Option<Uuid>,
    pub deployment_policy_version_id: Option<Uuid>,
    pub requester_kind: RequesterKind,
    pub requested_by_user_id: Option<Uuid>,
    pub requested_by_automation: Option<String>,
    pub required_approvals: i32,
    pub required_role: Option<String>,
    pub distinct_approvers: bool,
    pub requester_may_approve: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub request_fingerprint: String,
}

/// Approval request with progress info, suitable for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequestDetail {
    #[serde(flatten)]
    pub request: DeploymentApprovalRequest,
    pub current_approval_count: i64,
    pub decisions: Vec<DeploymentApprovalDecision>,
    /// Actions the requesting user can take.
    pub allowed_actions: Vec<String>,
}

/// Compact approval info for system list DTOs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemApprovalSummary {
    pub pending_approval_count: i64,
    pub oldest_pending_request_at: Option<DateTime<Utc>>,
}

/// Compact approval info for environment list DTOs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvironmentApprovalSummary {
    pub pending_approval_count: i64,
    pub systems_with_pending_approval: i64,
}
