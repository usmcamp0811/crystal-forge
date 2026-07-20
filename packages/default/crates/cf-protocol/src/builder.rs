//! Builder ↔ server wire protocol types.
//!
//! These types are serialized over HTTP between the Crystal Forge server and
//! remote build workers. No database, Axum, or server-internal types are
//! permitted here.

use crate::cache::CacheType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// EXECUTION STRATEGY TYPES
// =============================================================================

/// Explicit remote build execution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteBuildExecutionStrategy {
    /// Server evaluates and provides the authoritative `.drv` path.
    #[default]
    ServerDerivation,
    /// Builder evaluates immutable source locally and must match the server's
    /// expected `.drvPath` before building.
    SourceReEvaluateVerified,
}

/// Source/input delivery mode for verified source re-evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceInputDeliveryMode {
    /// Not applicable for the current job strategy.
    #[default]
    None,
    /// Server packages the top-level flake repository as a tar.gz of its
    /// bare Git mirror and serves it via an authenticated API endpoint.
    /// The builder downloads, verifies (SHA-256), and extracts the archive
    /// into a job-scoped bare mirror, then evaluates the flake from a
    /// detached worktree.
    ///
    /// **Scope:** only the top-level repository is bundled. Locked flake
    /// inputs that are NOT already in the builder's Nix store or reachable
    /// via configured substituters may still require network access during
    /// `nix eval`. Private flake inputs must be publicly accessible, cached,
    /// or pre-seeded on the builder for air-gapped operation.
    ServerBundledArchive,
    /// Builder uses or creates a detached local Git worktree from a local mirror
    /// at the authorized commit. Colocated server/builder deployments may share
    /// these roots.
    LocalGitWorktree,
    /// Builder may fetch public flake inputs itself. Builders still must not
    /// receive broad private Git credentials.
    BuilderFetchPublicInputs,
}

// =============================================================================
// SOURCE IDENTITY
// =============================================================================

/// Immutable source identity for verified source re-evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedSourceIdentity {
    pub repo_url: String,
    pub commit_hash: String,
    pub flake_target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_sha256: Option<String>,
}

/// Evaluator fingerprint recorded in the job manifest for auditability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluatorFingerprint {
    pub nix_version: String,
    #[serde(default)]
    pub pure_eval: bool,
    #[serde(default)]
    pub lockfile_mutation_allowed: bool,
}

// =============================================================================
// CACHE PUSH CONFIG
// =============================================================================

/// Cache-push settings selected by the server for a remote builder job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderCachePushConfig {
    #[serde(default)]
    pub cache_type: CacheType,
    pub push_to: Option<String>,
    #[serde(default)]
    pub push_after_build: bool,
    pub signing_key: Option<String>,
    pub compression: Option<String>,
    pub s3_region: Option<String>,
    pub s3_profile: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_session_token: Option<String>,
    pub s3_endpoint_url: Option<String>,
    pub attic_token: Option<String>,
    pub attic_cache_name: Option<String>,
    pub attic_public_key: Option<String>,
    #[serde(default)]
    pub attic_ignore_upstream_cache_filter: bool,
    #[serde(default)]
    pub attic_jobs: u32,
    #[serde(default)]
    pub max_retries: u32,
    #[serde(default)]
    pub retry_delay_seconds: u64,
    #[serde(default = "default_push_timeout_seconds")]
    pub push_timeout_seconds: u64,
    #[serde(default)]
    pub force_repush: bool,
    #[serde(default)]
    pub require_sigs: bool,
}

fn default_push_timeout_seconds() -> u64 {
    3600 // 1 hour
}

impl BuilderCachePushConfig {
    pub fn disabled() -> Self {
        Self {
            cache_type: CacheType::Nix,
            push_to: None,
            push_after_build: false,
            signing_key: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: None,
            attic_cache_name: None,
            attic_public_key: None,
            attic_ignore_upstream_cache_filter: true,
            attic_jobs: 5,
            max_retries: 3,
            retry_delay_seconds: 5,
            push_timeout_seconds: default_push_timeout_seconds(),
            force_repush: false,
            require_sigs: true,
        }
    }
}

// =============================================================================
// BUILD JOB WIRE TYPES
// =============================================================================

/// Minimal derivation build payload delivered to API-mode builders so they can
/// build without any direct database access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildJobDerivation {
    pub id: i32,
    pub derivation_name: String,
    /// "nixos" or "package"
    pub derivation_type: String,
    /// .drv path populated during the dry-run/eval phase.
    pub derivation_path: Option<String>,
    /// Resolved output store path, if already known.
    pub store_path: Option<String>,
    /// Explicit remote build execution strategy.
    #[serde(default)]
    pub execution_strategy: RemoteBuildExecutionStrategy,
    /// Source metadata used by verified source re-evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<VerifiedSourceIdentity>,
    /// How the builder should obtain flake inputs for local evaluation.
    #[serde(default)]
    pub source_input_delivery: SourceInputDeliveryMode,
    /// Server-authorized toplevel derivation identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_drv_path: Option<String>,
    /// Server-recorded evaluator fingerprint for audit/debugging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluator: Option<EvaluatorFingerprint>,
    /// Server-selected cache destination for builder-side output pushes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_push: Option<BuilderCachePushConfig>,
}

/// Build job wire representation (as delivered in `NextJobResponse`).
///
/// This is the serde-only form for the builder ↔ server API.
/// The server maps from its DB row type before sending.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildJob {
    pub id: Uuid,
    pub builder_id: Option<Uuid>,
    #[serde(default)]
    pub builder_session_id: Option<Uuid>,
    pub derivation_id: i32,
    pub environment_id: Option<Uuid>,
    pub status: String,
    pub retry_count: i32,
    pub max_retries: i32,
    pub priority_weight: f64,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub logs: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response returned by GET/POST /api/v1/builders/:id/next-job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextJobResponse {
    pub job: BuildJob,
    pub derivation: BuildJobDerivation,
}

/// Signed request body for POST /api/v1/builders/:id/next-job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextJobRequest {
    #[serde(default = "default_builder_protocol_version")]
    pub protocol_version: u32,
    #[serde(default = "default_supported_execution_strategies")]
    pub supported_execution_strategies: Vec<RemoteBuildExecutionStrategy>,
}

fn default_builder_protocol_version() -> u32 {
    1
}

fn default_supported_execution_strategies() -> Vec<RemoteBuildExecutionStrategy> {
    vec![RemoteBuildExecutionStrategy::ServerDerivation]
}

// =============================================================================
// BUILDER SESSION / REGISTRATION TYPES
// =============================================================================

/// Request to resolve a builder's server-assigned ID from its public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveBuilderIdRequest {
    /// Base64-encoded Ed25519 public key derived from the builder's local private key.
    pub public_key: String,
    /// Per-process session UUID generated on builder startup.
    #[serde(default)]
    pub session_id: Option<Uuid>,
}

/// Response returned when a builder public key has been registered/approved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveBuilderIdResponse {
    pub builder_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
}

/// Request to establish a process/session for a configured builder ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstablishBuilderSessionRequest {
    pub session_id: Uuid,
}

/// Response returned after establishing a builder process/session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstablishBuilderSessionResponse {
    pub builder_id: Uuid,
    pub session_id: Uuid,
    pub recovered_jobs: usize,
}

// =============================================================================
// BUILD PROGRESS / STATUS REPORTING
// =============================================================================

/// Distinct pre-build/build failure phases reported by API builders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildFailurePhase {
    SourceFetch,
    SourceInputAvailability,
    Evaluation,
    DerivationMismatch,
    PathMaterialization,
    Build,
}

impl std::fmt::Display for BuildFailurePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildFailurePhase::SourceFetch => write!(f, "source_fetch"),
            BuildFailurePhase::SourceInputAvailability => write!(f, "source_input_availability"),
            BuildFailurePhase::Evaluation => write!(f, "evaluation"),
            BuildFailurePhase::DerivationMismatch => write!(f, "derivation_mismatch"),
            BuildFailurePhase::PathMaterialization => write!(f, "path_materialization"),
            BuildFailurePhase::Build => write!(f, "build"),
        }
    }
}

/// Build progress report sent by API builders (HTTP fallback for the WS frame).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildProgressRequest {
    pub derivation_id: i32,
    pub elapsed_seconds: i32,
    pub current_target: Option<String>,
    pub last_activity_seconds: i32,
}

/// Request to report builder metrics (via heartbeat).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReportMetricsRequest {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: i64,
    pub system_cpu_usage_percent: Option<f64>,
    pub system_memory_total_mb: Option<i64>,
    pub system_memory_used_mb: Option<i64>,
}

/// Append build logs request.
#[derive(Debug, Serialize, Deserialize)]
pub struct AppendLogsRequest {
    pub logs: String,
}

// =============================================================================
// DERIVATION MANIFEST / ARCHIVE
// =============================================================================

/// Response for GET /api/v1/builders/:id/jobs/:job_id/derivation-manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivationManifestResponse {
    pub job_id: Uuid,
    pub drv_path: String,
    /// Sorted, deduplicated requisite store paths for `drv_path`.
    pub paths: Vec<String>,
}

/// Request body for POST /api/v1/builders/:id/jobs/:job_id/derivation-archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivationArchiveRequest {
    pub paths: Vec<String>,
}

// =============================================================================
// CACHE PUSH REPORTING
// =============================================================================

/// A cache-push job handed to an API builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePushJobPayload {
    pub id: Uuid,
    pub derivation_id: i32,
    pub derivation_name: String,
    /// Path to push: store_path or derivation_path.
    pub path: String,
    /// Optional cache destination name for last-used bookkeeping.
    pub cache_destination_name: Option<String>,
}

/// Result of a successful cache push reported by an API builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePushCompleteRequest {
    pub duration_ms: Option<i32>,
}

/// Failure report for a cache push.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePushFailRequest {
    pub error_message: String,
}

// =============================================================================
// CVE SCAN REPORTING
// =============================================================================

/// A CVE scan target handed to an API builder, including the created scan id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveScanTarget {
    pub scan_id: i32,
    pub derivation_id: i32,
    pub derivation_name: String,
    pub store_path: String,
}

/// Raw vulnix output uploaded by an API builder for server-side parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveScanResultsRequest {
    /// Raw vulnix JSON (stdout) for server-side parsing/persistence.
    pub raw_output: String,
    pub scan_duration_ms: Option<i32>,
}

/// Failure report for a CVE scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveScanFailRequest {
    pub error_message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_job_derivation_defaults_to_server_derivation_strategy() {
        let json = r#"{
            "id": 42,
            "derivation_name": "host-a",
            "derivation_type": "nixos",
            "derivation_path": "/nix/store/server-host-a.drv",
            "store_path": null
        }"#;

        let payload: BuildJobDerivation = serde_json::from_str(json).expect("payload should parse");
        assert_eq!(
            payload.execution_strategy,
            RemoteBuildExecutionStrategy::ServerDerivation
        );
        assert_eq!(payload.source_input_delivery, SourceInputDeliveryMode::None);
        assert_eq!(payload.expected_drv_path, None);
        assert!(payload.cache_push.is_none());
    }

    #[test]
    fn verified_source_strategy_serializes_as_snake_case() {
        let payload = BuildJobDerivation {
            id: 42,
            derivation_name: "host-a".to_string(),
            derivation_type: "nixos".to_string(),
            derivation_path: None,
            store_path: None,
            execution_strategy: RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            source: Some(VerifiedSourceIdentity {
                repo_url: "https://gitlab.com/example/private.git".to_string(),
                commit_hash: "abc123".to_string(),
                flake_target: "nixosConfigurations.host-a.config.system.build.toplevel".to_string(),
                mirror_id: Some("repo-test".to_string()),
                mirror_path: Some("/var/lib/crystal-forge/flake-mirrors/repo-test.git".to_string()),
                worktree_path: Some(
                    "/var/lib/crystal-forge/flake-worktrees/repo-test/abc123".to_string(),
                ),
                lock_hash: Some("sha256-lock".to_string()),
                archive_url: Some("file:///tmp/source".to_string()),
                archive_sha256: Some("sha256-source".to_string()),
            }),
            source_input_delivery: SourceInputDeliveryMode::ServerBundledArchive,
            expected_drv_path: Some("/nix/store/server-host-a.drv".to_string()),
            evaluator: Some(EvaluatorFingerprint {
                nix_version: "2.28.0".to_string(),
                pure_eval: true,
                lockfile_mutation_allowed: false,
            }),
            cache_push: None,
        };

        let value = serde_json::to_value(payload).expect("payload should serialize");
        assert_eq!(value["execution_strategy"], "source_re_evaluate_verified");
        assert_eq!(value["source_input_delivery"], "server_bundled_archive");
        assert_eq!(value["expected_drv_path"], "/nix/store/server-host-a.drv");
    }

    #[test]
    fn build_failure_phase_display() {
        assert_eq!(BuildFailurePhase::SourceFetch.to_string(), "source_fetch");
        assert_eq!(
            BuildFailurePhase::DerivationMismatch.to_string(),
            "derivation_mismatch"
        );
    }
}
