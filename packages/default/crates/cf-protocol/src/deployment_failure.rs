//! Defines deployment failure data shared by agents and the server.
//!
//! Summary and remediation text are fixed vocabulary. Diagnostics are untrusted
//! even after agent masking: servers must sanitize them before administrator
//! display and must not expose them through general history summaries. Legacy
//! report detection and incomplete-report policy belong to the server.

use serde::{Deserialize, Serialize};

/// Describes the operation that failed, without embedding process output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentFailure {
    /// Identifies the failed operation boundary.
    pub stage: FailureStage,
    /// Identifies the fixed cause category.
    pub code: FailureCode,
    /// Records the child exit code, when a child exited normally.
    pub exit_code: Option<i32>,
    /// Records the number of attempts actually started, when applicable.
    pub attempt_count: Option<u32>,
    /// Records the configured attempt limit, when applicable.
    pub max_attempts: Option<u32>,
    /// Contains bounded masked detail for administrator-only consumption.
    pub diagnostics: Option<FailureDiagnostics>,
}

/// Identifies the deployment operation that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureStage {
    /// Selects or prepares the configured read cache.
    CacheConfiguration,
    /// Copies the target closure from the selected cache.
    CacheAcquisition,
    /// Creates the system profile generation.
    GenerationCreation,
    /// Checks system profile convergence to the desired generation.
    GenerationVerification,
    /// Validates the activation script or submits it to systemd.
    Activation,
    /// Performs deployment setup outside a more specific operation.
    Deployment,
}

impl FailureStage {
    /// Returns a fixed display label for this operation.
    pub fn label(self) -> &'static str {
        match self {
            Self::CacheConfiguration => "Cache configuration",
            Self::CacheAcquisition => "Cache acquisition",
            Self::GenerationCreation => "Generation creation",
            Self::GenerationVerification => "Generation verification",
            Self::Activation => "Activation",
            Self::Deployment => "Deployment",
        }
    }
}

// Keep wire names, summaries and remediation together so consumers do not
// maintain separate allowlists of arbitrary agent-supplied summary text.
macro_rules! failure_codes {
    ($( $(#[$doc:meta])* $variant:ident => ($wire:literal, $summary:literal, $remediation:expr)),+ $(,)?) => {
        /// Identifies a fixed deployment cause without disclosing diagnostics.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum FailureCode { $( $(#[$doc])* $variant, )+ }

        impl FailureCode {
            /// Returns the stable snake-case wire identifier.
            pub fn as_str(self) -> &'static str {
                match self { $( Self::$variant => $wire, )+ }
            }
            /// Returns a credential-free summary suitable for general display.
            pub fn summary(self) -> &'static str {
                match self { $( Self::$variant => $summary, )+ }
            }
            /// Returns fixed guidance when this category has an actionable remedy.
            pub fn remediation(self) -> Option<&'static str> {
                match self { $( Self::$variant => $remediation, )+ }
            }
        }

        #[cfg(test)]
        #[test]
        fn cause_wire_names_match_the_typed_allowlist() {
            for code in [$( FailureCode::$variant, )+] {
                let wire = serde_json::to_value(code).unwrap();
                assert_eq!(wire, code.as_str());
                assert_eq!(serde_json::from_value::<FailureCode>(wire).unwrap(), code);
                assert!(!code.summary().is_empty());
            }
            assert!(serde_json::from_str::<FailureCode>("\"arbitrary raw message\"").is_err());
        }
    };
}

failure_codes! {
    /// The daemon ignored a signing key from an untrusted agent user.
    UntrustedUser => ("untrusted_user", "signature verification failed because the Nix daemon ignored the cache signing key: the agent user is not a trusted Nix user", Some("Add the agent user to nix.settings.trusted-users.")),
    /// No configured trusted key validated the path signature.
    SignatureVerification => ("signature_verification", "signature verification failed: check the cache signing public key", Some("Check the cache signing public key and the published path signature.")),
    /// Client TLS credentials were rejected or could not be loaded.
    ClientCertificate => ("client_certificate", "the cache rejected or could not use the client certificate", Some("Check the mTLS credentials.")),
    /// The server certificate could not be validated.
    ServerCertificate => ("server_certificate", "TLS verification of the cache server certificate failed", Some("Check the cache CA and server certificate.")),
    /// HTTP authentication or authorization failed.
    Authorization => ("authorization", "the cache denied read authorization (HTTP 401 or 403)", Some("Check cache read credentials and permissions.")),
    /// The selected path or archive was absent.
    MissingPath => ("missing_path", "the store path or its NAR is missing from the cache", Some("Publish the target closure to the selected cache.")),
    /// The server limited request throughput.
    RateLimited => ("rate_limited", "the cache rate-limited the read (HTTP 429)", Some("Check cache rate limits and retry availability.")),
    /// The server returned a 5xx response.
    ServerError => ("server_error", "the cache server returned a server error (HTTP 5xx)", Some("Check the cache service health.")),
    /// Name resolution failed.
    Dns => ("dns", "DNS resolution of the cache host failed", Some("Check DNS configuration and cache hostname.")),
    /// The host was unreachable or refused the connection.
    Connection => ("connection", "could not connect to the cache server (connection refused or unreachable)", Some("Check cache availability and network access.")),
    /// A child operation or network request exceeded its deadline.
    Timeout => ("timeout", "the operation timed out", Some("Check service availability and the deployment timeout.")),
    /// A transfer ended before completion.
    InterruptedTransfer => ("interrupted_transfer", "the transfer from the cache was interrupted", Some("Check network stability and cache service health.")),
    /// Downloaded bytes did not match the expected hash.
    HashMismatch => ("hash_mismatch", "the downloaded data did not match its expected hash", Some("Check the published cache entry for corruption.")),
    /// The local host had no free space.
    DiskFull => ("disk_full", "the agent host is out of disk space", Some("Free disk space on the agent host.")),
    /// The local Nix daemon could not be reached.
    NixDaemon => ("nix_daemon", "the agent could not reach the local Nix daemon", Some("Check the Nix daemon service and socket access.")),
    /// A local operation was denied permission.
    PermissionDenied => ("permission_denied", "a local Nix operation was denied permission", Some("Check agent access to the Nix daemon, store and cache directory.")),
    /// A required executable could not be started.
    Spawn => ("spawn", "the deployment command could not be started", Some("Check that the required executable is installed and accessible.")),
    /// Waiting for a child process failed.
    Wait => ("wait", "the deployment command status could not be collected", Some("Check the agent service and host process resources.")),
    /// Command diagnostics could not be captured.
    Capture => ("capture", "the deployment command diagnostics could not be captured", Some("Check agent service resources and local diagnostics.")),
    /// A required cache configuration was absent.
    MissingCacheConfiguration => ("missing_cache_configuration", "deployment requires cache read configuration", Some("Configure a deployment cache and its required read credentials.")),
    /// Cache selection, credentials or runtime feature settings were invalid.
    InvalidCacheConfiguration => ("invalid_cache_configuration", "cache read configuration is invalid or unsupported", Some("Check cache type, endpoint, signing keys, credentials and Nix runtime support.")),
    /// The requested target was not a supported store path.
    InvalidTarget => ("invalid_target", "the deployment target is not a supported Nix store path", Some("Select a published NixOS system store path.")),
    /// The running system could not be read.
    CurrentSystem => ("current_system", "the current system target could not be read", Some("Check /run/current-system and the agent's filesystem access.")),
    /// Generation creation exited unsuccessfully.
    GenerationCreation => ("generation_creation", "the NixOS generation could not be created", Some("Check the system profile, Nix daemon and agent permissions.")),
    /// Generation verification failed or did not converge.
    GenerationVerification => ("generation_verification", "the NixOS generation could not be verified", Some("Check the system profile and target closure availability.")),
    /// The target has no activation script.
    MissingActivationScript => ("missing_activation_script", "the target activation script is missing", Some("Check that the target is a complete NixOS system closure.")),
    /// systemd rejected submission of the activation command.
    Activation => ("activation", "systemd could not start the deployment activation", Some("Check systemd service availability and agent permissions.")),
    /// The submitted activation command failed during execution.
    ActivationExecution => ("activation_execution", "the deployment activation command failed", Some("Check the activation unit journal and the target system configuration.")),
    /// No more specific fixed category was recognized.
    Unknown => ("unknown", "Deployment failed for an unrecognized reason; details are redacted", None),
}

/// Contains masked, bounded diagnostics, never raw command output.
///
/// Agents suppress all lines and endpoints for incomplete captures, provider
/// response bodies, or unavailable credential inventories. Servers must
/// independently enforce their display policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureDiagnostics {
    /// Contains at most three masked error lines, each at most 300 characters.
    pub lines: Vec<String>,
    /// Contains a URL host/path with no user information, query or fragment.
    pub cache_endpoint: Option<String>,
    /// Contains a fixed reason: `provider_response_body`, `incomplete_capture`,
    /// or `credential_inventory_unavailable`.
    pub suppression_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_contract_roundtrips_all_stages_and_optional_fields() {
        for (stage, wire) in [
            (FailureStage::CacheConfiguration, "cache_configuration"),
            (FailureStage::CacheAcquisition, "cache_acquisition"),
            (FailureStage::GenerationCreation, "generation_creation"),
            (
                FailureStage::GenerationVerification,
                "generation_verification",
            ),
            (FailureStage::Activation, "activation"),
            (FailureStage::Deployment, "deployment"),
        ] {
            let failure = DeploymentFailure {
                stage,
                code: FailureCode::Timeout,
                exit_code: None,
                attempt_count: Some(3),
                max_attempts: Some(3),
                diagnostics: Some(FailureDiagnostics {
                    lines: Vec::new(),
                    cache_endpoint: None,
                    suppression_reason: Some("incomplete_capture".into()),
                }),
            };
            let value = serde_json::to_value(&failure).unwrap();
            assert_eq!(value["stage"], wire);
            assert_eq!(value["code"], "timeout");
            assert_eq!(value["attempt_count"], 3);
            assert_eq!(
                serde_json::from_value::<DeploymentFailure>(value).unwrap(),
                failure
            );
            assert!(!stage.label().is_empty());
        }
        // Missing optional fields remain readable without inventing legacy
        // detection or completeness policy in the shared data type.
        let minimal: DeploymentFailure =
            serde_json::from_str(r#"{"stage":"deployment","code":"unknown"}"#).unwrap();
        assert_eq!(minimal.diagnostics, None);
        assert_eq!(minimal.attempt_count, None);
    }
}
