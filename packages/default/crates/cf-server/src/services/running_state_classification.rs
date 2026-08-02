//! Deterministic running-state trust classification rules.
//!
//! Given an attestation's verification result and deployment authorization
//! context, produces a stable trust classification. This module contains
//! pure logic with no database or I/O dependencies.

use cf_protocol::attestation::{TrustClassification, VerificationStatus};

/// Inputs to the classification engine.
pub struct ClassificationInput {
    /// The verification status of the attestation.
    pub verification_status: VerificationStatus,
    /// The observed current system store path.
    pub observed_store_path: String,
    /// The expected authorized store path for this system (if known).
    pub expected_store_path: Option<String>,
    /// Whether the observed store path has a valid authorization for this system.
    pub has_valid_authorization: bool,
    /// Whether the observed store path is known to Crystal Forge at all.
    pub artifact_is_known: bool,
    /// Whether the observed store path was previously authorized for this system.
    pub was_previously_authorized: bool,
    /// Whether the latest deployment for this system failed.
    pub deployment_failed: bool,
    /// Whether the system needs a reboot to apply the authorized profile.
    pub pending_reboot: bool,
    /// Evidence age in seconds since the observation.
    pub evidence_age_seconds: i64,
    /// Freshness threshold in seconds.
    pub freshness_threshold_seconds: i64,
}

/// Classification result with reason code.
pub struct ClassificationResult {
    pub classification: TrustClassification,
    pub reason_code: String,
}

/// Classify the trust state of a system based on attestation and context.
///
/// Applies the precedence rules defined in the task specification.
pub fn classify(input: &ClassificationInput) -> ClassificationResult {
    // 1. Agent identity invalid (highest precedence)
    match input.verification_status {
        VerificationStatus::InvalidSignature => {
            return ClassificationResult {
                classification: TrustClassification::AgentIdentityInvalid,
                reason_code: "invalid_signature".to_string(),
            };
        }
        VerificationStatus::UnknownKey => {
            return ClassificationResult {
                classification: TrustClassification::AgentIdentityInvalid,
                reason_code: "unknown_key".to_string(),
            };
        }
        VerificationStatus::RevokedKey => {
            return ClassificationResult {
                classification: TrustClassification::AgentIdentityInvalid,
                reason_code: "revoked_key".to_string(),
            };
        }
        VerificationStatus::IdentityMismatch => {
            return ClassificationResult {
                classification: TrustClassification::AgentIdentityInvalid,
                reason_code: "identity_mismatch".to_string(),
            };
        }
        VerificationStatus::InvalidSession => {
            return ClassificationResult {
                classification: TrustClassification::AgentIdentityInvalid,
                reason_code: "invalid_session".to_string(),
            };
        }
        VerificationStatus::Replay => {
            return ClassificationResult {
                classification: TrustClassification::AgentIdentityInvalid,
                reason_code: "replay_attack".to_string(),
            };
        }
        VerificationStatus::StaleTimestamp => {
            return ClassificationResult {
                classification: TrustClassification::AgentIdentityInvalid,
                reason_code: "stale_timestamp".to_string(),
            };
        }
        VerificationStatus::Malformed => {
            return ClassificationResult {
                classification: TrustClassification::AgentIdentityInvalid,
                reason_code: "malformed_attestation".to_string(),
            };
        }
        VerificationStatus::Verified => {
            // Continue to artifact-level classification
        }
    }

    // 2. Activation failed
    if input.deployment_failed {
        return ClassificationResult {
            classification: TrustClassification::ActivationFailed,
            reason_code: "deployment_activation_failed".to_string(),
        };
    }

    // 3. Deployment pending reboot
    if input.pending_reboot {
        return ClassificationResult {
            classification: TrustClassification::DeploymentPendingReboot,
            reason_code: "reboot_required_for_authorized_profile".to_string(),
        };
    }

    // 4. Authorized current
    if input.has_valid_authorization {
        let matches_expected = input
            .expected_store_path
            .as_ref()
            .is_some_and(|expected| expected == &input.observed_store_path);

        if matches_expected {
            // Check evidence freshness
            if input.evidence_age_seconds > input.freshness_threshold_seconds {
                return ClassificationResult {
                    classification: TrustClassification::AuthorizedButEvidenceStale,
                    reason_code: "evidence_beyond_freshness_threshold".to_string(),
                };
            }

            return ClassificationResult {
                classification: TrustClassification::AuthorizedCurrent,
                reason_code: "authorized_target_matches_observation".to_string(),
            };
        }
    }

    // 5. Authorized previous generation
    if input.was_previously_authorized {
        return ClassificationResult {
            classification: TrustClassification::AuthorizedPreviousGeneration,
            reason_code: "artifact_was_previously_authorized".to_string(),
        };
    }

    // 6. Unauthorized artifact (known but not authorized for this system)
    if input.artifact_is_known && !input.has_valid_authorization {
        return ClassificationResult {
            classification: TrustClassification::UnauthorizedArtifact,
            reason_code: "known_artifact_not_authorized_for_system".to_string(),
        };
    }

    // 7. Unknown artifact
    if !input.artifact_is_known {
        return ClassificationResult {
            classification: TrustClassification::UnknownArtifact,
            reason_code: "artifact_not_in_crystal_forge_records".to_string(),
        };
    }

    // Fallback: authorized but not the expected target
    if input.has_valid_authorization {
        return ClassificationResult {
            classification: TrustClassification::AuthorizedCurrent,
            reason_code: "authorized_target_observed".to_string(),
        };
    }

    // Should not reach here, but default to unknown
    ClassificationResult {
        classification: TrustClassification::UnknownArtifact,
        reason_code: "classification_fallback".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> ClassificationInput {
        ClassificationInput {
            verification_status: VerificationStatus::Verified,
            observed_store_path: "/nix/store/test-system".to_string(),
            expected_store_path: Some("/nix/store/test-system".to_string()),
            has_valid_authorization: true,
            artifact_is_known: true,
            was_previously_authorized: false,
            deployment_failed: false,
            pending_reboot: false,
            evidence_age_seconds: 0,
            freshness_threshold_seconds: 43200, // 12 hours
        }
    }

    #[test]
    fn authorized_current() {
        let input = base_input();
        let result = classify(&input);
        assert_eq!(result.classification, TrustClassification::AuthorizedCurrent);
    }

    #[test]
    fn identity_invalid_takes_precedence() {
        let mut input = base_input();
        input.verification_status = VerificationStatus::InvalidSignature;
        let result = classify(&input);
        assert_eq!(
            result.classification,
            TrustClassification::AgentIdentityInvalid
        );
    }

    #[test]
    fn stale_evidence() {
        let mut input = base_input();
        input.evidence_age_seconds = 50000; // > 12 hours
        let result = classify(&input);
        assert_eq!(
            result.classification,
            TrustClassification::AuthorizedButEvidenceStale
        );
    }

    #[test]
    fn unauthorized_artifact() {
        let mut input = base_input();
        input.has_valid_authorization = false;
        input.observed_store_path = "/nix/store/different-system".to_string();
        let result = classify(&input);
        assert_eq!(
            result.classification,
            TrustClassification::UnauthorizedArtifact
        );
    }

    #[test]
    fn unknown_artifact() {
        let mut input = base_input();
        input.has_valid_authorization = false;
        input.artifact_is_known = false;
        input.observed_store_path = "/nix/store/totally-unknown".to_string();
        let result = classify(&input);
        assert_eq!(
            result.classification,
            TrustClassification::UnknownArtifact
        );
    }

    #[test]
    fn pending_reboot() {
        let mut input = base_input();
        input.pending_reboot = true;
        input.observed_store_path = "/nix/store/old-system".to_string();
        let result = classify(&input);
        assert_eq!(
            result.classification,
            TrustClassification::DeploymentPendingReboot
        );
    }

    #[test]
    fn activation_failed() {
        let mut input = base_input();
        input.deployment_failed = true;
        input.observed_store_path = "/nix/store/old-system".to_string();
        let result = classify(&input);
        assert_eq!(
            result.classification,
            TrustClassification::ActivationFailed
        );
    }

    #[test]
    fn previously_authorized() {
        let mut input = base_input();
        input.has_valid_authorization = false;
        input.was_previously_authorized = true;
        input.expected_store_path = Some("/nix/store/newer-system".to_string());
        input.observed_store_path = "/nix/store/older-system".to_string();
        let result = classify(&input);
        assert_eq!(
            result.classification,
            TrustClassification::AuthorizedPreviousGeneration
        );
    }
}
