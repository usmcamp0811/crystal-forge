//! Domain models for running-state attestations, trust classification,
//! investigations, and resolution actions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Immutable signed running-state attestation record.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RunningStateAttestation {
    pub id: Uuid,
    pub attestation_id: Uuid,
    pub system_id: Uuid,
    pub agent_key_id: String,
    pub agent_session_id: Option<Uuid>,
    pub protocol_version: i32,
    pub boot_id: String,
    pub boot_timestamp: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub monotonic_counter: i64,
    pub current_system_store_path: String,
    pub current_system_nar_hash: Option<String>,
    pub system_profile_store_path: Option<String>,
    pub booted_generation: Option<i64>,
    pub kernel_version: Option<String>,
    pub nix_version: Option<String>,
    pub agent_version: String,
    pub agent_build_hash: Option<String>,
    pub reported_authorization_id: Option<Uuid>,
    pub reported_execution_id: Option<Uuid>,
    pub activation_source: Option<String>,
    pub canonical_payload: Vec<u8>,
    pub payload_digest: Vec<u8>,
    pub signature: Vec<u8>,
    pub verification_status: String,
    pub verification_reason_code: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Per-attestation trust classification assessment.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct AttestationAssessment {
    pub id: Uuid,
    pub attestation_id: Uuid,
    pub system_id: Uuid,
    pub classification: String,
    pub reason_code: String,
    pub matched_authorization_id: Option<Uuid>,
    pub matched_deployment_execution_id: Option<Uuid>,
    pub matched_artifact_id: Option<Uuid>,
    pub classifier_version: i32,
    pub assessed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Current projected trust state for a system.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct SystemTrustState {
    pub system_id: Uuid,
    pub current_classification: String,
    pub reason_code: String,
    pub latest_attestation_id: Option<Uuid>,
    pub latest_authorization_id: Option<Uuid>,
    pub observed_store_path: Option<String>,
    pub expected_store_path: Option<String>,
    pub evidence_age_seconds: Option<i64>,
    pub investigation_id: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Investigation case for suspicious running-state trust conditions.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct AttestationInvestigation {
    pub id: Uuid,
    pub system_id: Uuid,
    pub source_attestation_id: Uuid,
    pub status: String,
    pub opened_by_user_id: Uuid,
    pub owner_user_id: Option<Uuid>,
    pub opening_note: String,
    pub opened_at: DateTime<Utc>,
    pub resolved_by_user_id: Option<Uuid>,
    pub resolution_reason: Option<String>,
    pub resolution_note: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Immutable resolution action record.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct AttestationResolutionAction {
    pub id: Uuid,
    pub system_id: Uuid,
    pub attestation_id: Uuid,
    pub actor_user_id: Uuid,
    pub action: String,
    pub note: String,
    pub created_authorization_id: Option<Uuid>,
    pub created_deployment_request_id: Option<Uuid>,
    pub investigation_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Summary counts for the attestation trust dashboard widget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationTrustSummary {
    pub flagged_unresolved: i64,
    pub authorized_current: i64,
    pub stale_evidence: i64,
}

/// System trust detail for the API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemTrustDetail {
    pub system_id: Uuid,
    pub current_classification: String,
    pub reason_code: String,
    pub classification_label: String,
    pub latest_attestation: Option<AttestationSummary>,
    pub verification_status: Option<String>,
    pub observed_store_path: Option<String>,
    pub expected_store_path: Option<String>,
    pub matched_authorization_id: Option<Uuid>,
    pub evidence_age_seconds: Option<i64>,
    pub investigation: Option<AttestationInvestigation>,
    pub allowed_actions: Vec<String>,
}

/// Compact attestation info for API responses (no payload/signature bytes).
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct AttestationSummary {
    pub id: Uuid,
    pub attestation_id: Uuid,
    pub system_id: Uuid,
    pub agent_key_id: String,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub current_system_store_path: String,
    pub verification_status: String,
    pub boot_id: String,
    pub monotonic_counter: i64,
    pub agent_version: String,
}

/// Compact system trust info for system list DTOs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemTrustSummary {
    pub current_classification: Option<String>,
    pub has_flagged_trust: bool,
    pub investigation_status: Option<String>,
}

/// Component counts for the sidebar Systems badge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemsAttentionCounts {
    pub critical_or_offline: i64,
    pub pending_approvals: i64,
    pub flagged_attestations: i64,
    pub total: i64,
}

/// Default attestation interval (6 hours).
pub const DEFAULT_ATTESTATION_INTERVAL_SECS: u64 = 6 * 60 * 60;

/// Default evidence freshness interval (12 hours).
pub const DEFAULT_EVIDENCE_FRESHNESS_SECS: u64 = 12 * 60 * 60;
