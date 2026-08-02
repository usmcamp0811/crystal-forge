//! Running-state attestation wire protocol types.
//!
//! These DTOs define the canonical attestation payload, the signed envelope
//! exchanged between agents and the server, and the server receipt response.
//! No host inspection, signing, verification, or database logic belongs here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Protocol version for the attestation canonical payload format.
pub const ATTESTATION_PROTOCOL_VERSION: i32 = 1;

/// Canonical attestation payload signed by the agent.
///
/// Field order is fixed for deterministic serialization. The signer and
/// verifier must serialize to the same bytes using compact JSON with sorted
/// keys (via `#[serde(rename_all = "snake_case")]` and alphabetical field
/// declaration).
///
/// **Invariant:** This struct is serialized with `serde_json::to_vec` using
/// the canonical field order defined here. Both agent and server must use
/// the same Rust type (from this crate) to build canonical bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttestationPayload {
    /// Activation source when known (e.g. "cf_deployment", "local_rebuild").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation_source: Option<String>,

    /// Agent build hash (git commit or derivation hash).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_build_hash: Option<String>,

    /// Agent key ID (typically the hostname used during enrollment).
    pub agent_key_id: String,

    /// Agent session ID when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<Uuid>,

    /// Agent software version.
    pub agent_version: String,

    /// Unique attestation ID. Must never be reused.
    pub attestation_id: Uuid,

    /// Linux boot UUID from /proc/sys/kernel/random/boot_id.
    pub boot_id: String,

    /// Boot timestamp when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_timestamp: Option<DateTime<Utc>>,

    /// Booted NixOS generation number when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub booted_generation: Option<i64>,

    /// Current system store path (e.g. /nix/store/...-nixos-system-...).
    pub current_system_store_path: String,

    /// NAR hash of the current system store path when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_system_nar_hash: Option<String>,

    /// Deployment authorization ID when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_authorization_id: Option<Uuid>,

    /// Deployment execution ID when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_execution_id: Option<Uuid>,

    /// Kernel version string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,

    /// Monotonic attestation counter. Must strictly increase per (agent_key_id, boot_id).
    pub monotonic_counter: i64,

    /// Nix version string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nix_version: Option<String>,

    /// Wall-clock observation timestamp.
    pub observed_at: DateTime<Utc>,

    /// Canonical payload digest (SHA-256 of the serialized payload bytes, hex-encoded).
    /// Populated after serialization, before signing.
    pub payload_digest: String,

    /// Protocol version for forward compatibility.
    pub protocol_version: i32,

    /// System ID or enrolled agent identity.
    pub system_id: String,

    /// Current system profile target store path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_profile_store_path: Option<String>,
}

/// Signed attestation envelope sent from agent to server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedAttestationEnvelope {
    /// The canonical payload.
    pub payload: AttestationPayload,
    /// Ed25519 signature of the canonical payload bytes, base64-encoded.
    pub signature: String,
    /// Agent key ID (matches payload.agent_key_id).
    pub key_id: String,
    /// Signature algorithm version.
    #[serde(default = "default_signature_algorithm")]
    pub signature_algorithm: String,
}

fn default_signature_algorithm() -> String {
    "ed25519-v1".to_string()
}

/// Server receipt response for a submitted attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationReceipt {
    /// Server-assigned row ID.
    pub id: Uuid,
    /// The attestation ID from the payload.
    pub attestation_id: Uuid,
    /// Trust classification result.
    pub classification: String,
    /// Verification status.
    pub verification_status: String,
    /// Human-readable reason when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Server receive timestamp.
    pub received_at: DateTime<Utc>,
}

/// Deployment approval status values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalRequestStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Cancelled,
    Superseded,
    Consumed,
}

impl std::fmt::Display for ApprovalRequestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Approved => write!(f, "approved"),
            Self::Rejected => write!(f, "rejected"),
            Self::Expired => write!(f, "expired"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Superseded => write!(f, "superseded"),
            Self::Consumed => write!(f, "consumed"),
        }
    }
}

impl std::str::FromStr for ApprovalRequestStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "expired" => Ok(Self::Expired),
            "cancelled" => Ok(Self::Cancelled),
            "superseded" => Ok(Self::Superseded),
            "consumed" => Ok(Self::Consumed),
            _ => Err(format!("Invalid approval request status: {s}")),
        }
    }
}

/// Running-state verification status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    InvalidSignature,
    UnknownKey,
    RevokedKey,
    IdentityMismatch,
    InvalidSession,
    Replay,
    StaleTimestamp,
    Malformed,
}

impl std::fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Verified => "verified",
            Self::InvalidSignature => "invalid_signature",
            Self::UnknownKey => "unknown_key",
            Self::RevokedKey => "revoked_key",
            Self::IdentityMismatch => "identity_mismatch",
            Self::InvalidSession => "invalid_session",
            Self::Replay => "replay",
            Self::StaleTimestamp => "stale_timestamp",
            Self::Malformed => "malformed",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for VerificationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "verified" => Ok(Self::Verified),
            "invalid_signature" => Ok(Self::InvalidSignature),
            "unknown_key" => Ok(Self::UnknownKey),
            "revoked_key" => Ok(Self::RevokedKey),
            "identity_mismatch" => Ok(Self::IdentityMismatch),
            "invalid_session" => Ok(Self::InvalidSession),
            "replay" => Ok(Self::Replay),
            "stale_timestamp" => Ok(Self::StaleTimestamp),
            "malformed" => Ok(Self::Malformed),
            _ => Err(format!("Invalid verification status: {s}")),
        }
    }
}

/// Running-state trust classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustClassification {
    AuthorizedCurrent,
    AuthorizedButEvidenceStale,
    AuthorizedPreviousGeneration,
    DeploymentPendingReboot,
    ActivationFailed,
    UnauthorizedArtifact,
    UnknownArtifact,
    AgentAttestationStale,
    AgentIdentityInvalid,
    /// Initial state before any attestation is received.
    NoAttestation,
}

impl TrustClassification {
    /// Returns true if this classification requires a flagged attention occurrence.
    pub fn is_flagged(&self) -> bool {
        matches!(
            self,
            Self::UnauthorizedArtifact | Self::UnknownArtifact | Self::AgentIdentityInvalid
        )
    }

    /// Returns true if this is a stale-evidence state (projected, not assessed).
    pub fn is_stale(&self) -> bool {
        matches!(
            self,
            Self::AuthorizedButEvidenceStale | Self::AgentAttestationStale
        )
    }

    /// Display label for UI presentation.
    pub fn label(&self) -> &'static str {
        match self {
            Self::AuthorizedCurrent => "Authorized",
            Self::AuthorizedButEvidenceStale => "Authorized (stale evidence)",
            Self::AuthorizedPreviousGeneration => "Authorized (previous generation)",
            Self::DeploymentPendingReboot => "Pending reboot",
            Self::ActivationFailed => "Activation failed",
            Self::UnauthorizedArtifact => "Unauthorized artifact",
            Self::UnknownArtifact => "Unknown artifact",
            Self::AgentAttestationStale => "Stale attestation",
            Self::AgentIdentityInvalid => "Identity invalid",
            Self::NoAttestation => "No attestation",
        }
    }
}

impl std::fmt::Display for TrustClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::AuthorizedCurrent => "authorized_current",
            Self::AuthorizedButEvidenceStale => "authorized_but_evidence_stale",
            Self::AuthorizedPreviousGeneration => "authorized_previous_generation",
            Self::DeploymentPendingReboot => "deployment_pending_reboot",
            Self::ActivationFailed => "activation_failed",
            Self::UnauthorizedArtifact => "unauthorized_artifact",
            Self::UnknownArtifact => "unknown_artifact",
            Self::AgentAttestationStale => "agent_attestation_stale",
            Self::AgentIdentityInvalid => "agent_identity_invalid",
            Self::NoAttestation => "no_attestation",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for TrustClassification {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "authorized_current" => Ok(Self::AuthorizedCurrent),
            "authorized_but_evidence_stale" => Ok(Self::AuthorizedButEvidenceStale),
            "authorized_previous_generation" => Ok(Self::AuthorizedPreviousGeneration),
            "deployment_pending_reboot" => Ok(Self::DeploymentPendingReboot),
            "activation_failed" => Ok(Self::ActivationFailed),
            "unauthorized_artifact" => Ok(Self::UnauthorizedArtifact),
            "unknown_artifact" => Ok(Self::UnknownArtifact),
            "agent_attestation_stale" => Ok(Self::AgentAttestationStale),
            "agent_identity_invalid" => Ok(Self::AgentIdentityInvalid),
            "no_attestation" => Ok(Self::NoAttestation),
            _ => Err(format!("Invalid trust classification: {s}")),
        }
    }
}

/// Approval decision type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecisionType {
    Approve,
    Reject,
}

impl std::fmt::Display for ApprovalDecisionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Approve => write!(f, "approve"),
            Self::Reject => write!(f, "reject"),
        }
    }
}

impl std::str::FromStr for ApprovalDecisionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "approve" => Ok(Self::Approve),
            "reject" => Ok(Self::Reject),
            _ => Err(format!("Invalid approval decision type: {s}")),
        }
    }
}

/// Attestation resolution action type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionAction {
    Adopt,
    Replace,
    Investigate,
    CloseInvestigation,
}

impl std::fmt::Display for ResolutionAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Adopt => write!(f, "adopt"),
            Self::Replace => write!(f, "replace"),
            Self::Investigate => write!(f, "investigate"),
            Self::CloseInvestigation => write!(f, "close_investigation"),
        }
    }
}

impl std::str::FromStr for ResolutionAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "adopt" => Ok(Self::Adopt),
            "replace" => Ok(Self::Replace),
            "investigate" => Ok(Self::Investigate),
            "close_investigation" => Ok(Self::CloseInvestigation),
            _ => Err(format!("Invalid resolution action: {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify canonical payload serialization produces deterministic JSON.
    #[test]
    fn canonical_payload_deterministic_serialization() {
        let payload = AttestationPayload {
            activation_source: None,
            agent_build_hash: Some("abc123".to_string()),
            agent_key_id: "test-host".to_string(),
            agent_session_id: None,
            agent_version: "0.1.0".to_string(),
            attestation_id: Uuid::nil(),
            boot_id: "12345678-1234-1234-1234-123456789012".to_string(),
            boot_timestamp: None,
            booted_generation: Some(42),
            current_system_store_path: "/nix/store/test-nixos-system".to_string(),
            current_system_nar_hash: None,
            deployment_authorization_id: None,
            deployment_execution_id: None,
            kernel_version: Some("6.1.0".to_string()),
            monotonic_counter: 1,
            nix_version: Some("2.18.0".to_string()),
            observed_at: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            payload_digest: "0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            protocol_version: ATTESTATION_PROTOCOL_VERSION,
            system_id: "test-system-id".to_string(),
            system_profile_store_path: None,
        };

        let bytes1 = serde_json::to_vec(&payload).unwrap();
        let bytes2 = serde_json::to_vec(&payload).unwrap();
        assert_eq!(bytes1, bytes2, "Canonical serialization must be deterministic");

        // Verify fields with skip_serializing_if are actually omitted
        let json_str = String::from_utf8(bytes1).unwrap();
        assert!(
            !json_str.contains("activation_source"),
            "None fields with skip_serializing_if should be omitted"
        );
        assert!(
            json_str.contains("agent_build_hash"),
            "Some fields should be present"
        );
    }

    /// Verify round-trip for all enums.
    #[test]
    fn enum_round_trips() {
        // ApprovalRequestStatus
        for status in [
            ApprovalRequestStatus::Pending,
            ApprovalRequestStatus::Approved,
            ApprovalRequestStatus::Rejected,
            ApprovalRequestStatus::Expired,
            ApprovalRequestStatus::Cancelled,
            ApprovalRequestStatus::Superseded,
            ApprovalRequestStatus::Consumed,
        ] {
            let s = status.to_string();
            let parsed: ApprovalRequestStatus = s.parse().unwrap();
            assert_eq!(parsed, status);
        }

        // VerificationStatus
        for status in [
            VerificationStatus::Verified,
            VerificationStatus::InvalidSignature,
            VerificationStatus::UnknownKey,
            VerificationStatus::RevokedKey,
            VerificationStatus::IdentityMismatch,
            VerificationStatus::InvalidSession,
            VerificationStatus::Replay,
            VerificationStatus::StaleTimestamp,
            VerificationStatus::Malformed,
        ] {
            let s = status.to_string();
            let parsed: VerificationStatus = s.parse().unwrap();
            assert_eq!(parsed, status);
        }

        // TrustClassification
        for class in [
            TrustClassification::AuthorizedCurrent,
            TrustClassification::AuthorizedButEvidenceStale,
            TrustClassification::AuthorizedPreviousGeneration,
            TrustClassification::DeploymentPendingReboot,
            TrustClassification::ActivationFailed,
            TrustClassification::UnauthorizedArtifact,
            TrustClassification::UnknownArtifact,
            TrustClassification::AgentAttestationStale,
            TrustClassification::AgentIdentityInvalid,
            TrustClassification::NoAttestation,
        ] {
            let s = class.to_string();
            let parsed: TrustClassification = s.parse().unwrap();
            assert_eq!(parsed, class);
        }

        // ResolutionAction
        for action in [
            ResolutionAction::Adopt,
            ResolutionAction::Replace,
            ResolutionAction::Investigate,
            ResolutionAction::CloseInvestigation,
        ] {
            let s = action.to_string();
            let parsed: ResolutionAction = s.parse().unwrap();
            assert_eq!(parsed, action);
        }
    }
}
