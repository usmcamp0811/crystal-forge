//! Builder models for multi-builder API support.
//!
//! Builders are registered build workers that communicate with the server via API.
//! Each builder has:
//! - Unique UUID identifier
//! - Ed25519 public key for request signing
//! - Resource limits (CPU, memory, concurrent jobs)
//! - Status tracking (active/inactive/offline)
//! - Environment assignments (1:many relationship)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::config::{CacheConfig, CacheType};
use crate::models::public_key::PublicKey;

/// Builder status enum
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum BuilderStatus {
    #[sqlx(rename = "active")]
    Active,
    #[sqlx(rename = "inactive")]
    Inactive,
    #[sqlx(rename = "offline")]
    Offline,
    #[sqlx(rename = "draining")]
    Draining,
}

impl ToString for BuilderStatus {
    fn to_string(&self) -> String {
        match self {
            BuilderStatus::Active => "active".into(),
            BuilderStatus::Inactive => "inactive".into(),
            BuilderStatus::Offline => "offline".into(),
            BuilderStatus::Draining => "draining".into(),
        }
    }
}

impl From<String> for BuilderStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "active" => BuilderStatus::Active,
            "inactive" => BuilderStatus::Inactive,
            "offline" => BuilderStatus::Offline,
            "draining" => BuilderStatus::Draining,
            _ => BuilderStatus::Inactive, // default to inactive for unknown values
        }
    }
}

/// A registered builder
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Builder {
    pub id: Uuid,
    pub name: String,
    pub host: Option<String>,
    pub arch: String,
    pub public_key: PublicKey,
    #[serde(default)]
    #[sqlx(default)]
    pub public_key_fingerprint: String,
    pub status: BuilderStatus,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: i32,
    pub enabled: bool,
    #[serde(default)]
    #[sqlx(default)]
    pub current_session_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(default)]
    pub current_session_started_at: Option<DateTime<Utc>>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Summary view of a builder (for list endpoints)
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BuilderSummary {
    pub id: Uuid,
    pub name: String,
    pub host: Option<String>,
    pub arch: String,
    pub status: BuilderStatus,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: i32,
    pub enabled: bool,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub assigned_environment_count: i32,
    #[sqlx(default)]
    pub active_jobs: i32,
    #[sqlx(default)]
    pub queued_jobs: i32,
}

impl Builder {
    pub fn with_public_key_fingerprint(mut self) -> Self {
        self.public_key_fingerprint = self.public_key.fingerprint();
        self
    }
}

/// Builder with environment assignments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderWithEnvironments {
    #[serde(flatten)]
    pub builder: Builder,
    pub assigned_environment_ids: Vec<Uuid>,
}

/// Builder environment assignment
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BuilderEnvironmentAssignment {
    pub id: i32,
    pub builder_id: Uuid,
    pub environment_id: Uuid,
    pub created_at: DateTime<Utc>,
}

/// Request to create a new builder
#[derive(Debug, Clone, Deserialize)]
pub struct CreateBuilderRequest {
    pub name: String,
    pub host: Option<String>,
    pub arch: String,
    /// Optional base64-encoded Ed25519 public key
    /// If not provided, server will generate a proper Ed25519 keypair
    pub public_key: Option<String>,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: Option<i32>,
    pub enabled: Option<bool>,
    pub environment_ids: Vec<Uuid>, // Optional environment assignments
}

/// Request to update a builder
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBuilderRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub arch: Option<String>,
    pub status: Option<BuilderStatus>,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: Option<i32>,
    pub enabled: Option<bool>,
}

/// Request to update builder public key
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBuilderPublicKeyRequest {
    pub public_key: String, // base64-encoded Ed25519 public key
}

/// Request for a builder to resolve its server-assigned ID from its public key.
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

/// Request to update builder environment assignments
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBuilderEnvironmentsRequest {
    pub environment_ids: Vec<Uuid>,
}

/// Builder resource metrics
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BuilderMetrics {
    pub id: i64,
    pub builder_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: i64,
    pub system_cpu_usage_percent: Option<f64>,
    pub system_memory_total_mb: Option<i64>,
    pub system_memory_used_mb: Option<i64>,
}

/// Request to report builder metrics (via heartbeat)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReportMetricsRequest {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: i64,
    pub system_cpu_usage_percent: Option<f64>,
    pub system_memory_total_mb: Option<i64>,
    pub system_memory_used_mb: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_status_serialization() {
        assert_eq!(BuilderStatus::Active.to_string(), "active");
        assert_eq!(BuilderStatus::Inactive.to_string(), "inactive");
        assert_eq!(BuilderStatus::Offline.to_string(), "offline");
        assert_eq!(BuilderStatus::Draining.to_string(), "draining");
    }

    #[test]
    fn test_builder_status_from_string() {
        assert_eq!(
            BuilderStatus::from("active".to_string()),
            BuilderStatus::Active
        );
        assert_eq!(
            BuilderStatus::from("inactive".to_string()),
            BuilderStatus::Inactive
        );
        assert_eq!(
            BuilderStatus::from("offline".to_string()),
            BuilderStatus::Offline
        );
        assert_eq!(
            BuilderStatus::from("draining".to_string()),
            BuilderStatus::Draining
        );
        assert_eq!(
            BuilderStatus::from("unknown".to_string()),
            BuilderStatus::Inactive
        );
    }

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
}

// =============================================================================
// BUILD JOB MODELS (for work queue)
// =============================================================================

/// Status of a build job
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildJobStatus {
    Queued,
    Building,
    Cancelling,
    Success,
    Failed,
    Cancelled,
}

impl std::fmt::Display for BuildJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildJobStatus::Queued => write!(f, "queued"),
            BuildJobStatus::Building => write!(f, "building"),
            BuildJobStatus::Cancelling => write!(f, "cancelling"),
            BuildJobStatus::Success => write!(f, "success"),
            BuildJobStatus::Failed => write!(f, "failed"),
            BuildJobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// A build job in the queue
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BuildJob {
    pub id: Uuid,
    pub builder_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(default)]
    pub builder_session_id: Option<Uuid>,
    pub derivation_id: i32,
    pub environment_id: Option<Uuid>,
    pub status: String, // Will be parsed to BuildJobStatus
    pub retry_count: i32,
    pub max_retries: i32,
    pub priority_weight: f64,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub logs: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response for append_job_logs endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct AppendLogsRequest {
    pub logs: String,
}

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
    /// Explicit remote build execution strategy. Defaults to the current
    /// server-authoritative derivation flow for older servers/clients.
    #[serde(default)]
    pub execution_strategy: RemoteBuildExecutionStrategy,
    /// Source metadata used by verified source re-evaluation. This is optional
    /// for `server_derivation` jobs and required for verified source jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<VerifiedSourceIdentity>,
    /// How the builder should obtain flake inputs for local evaluation.
    #[serde(default)]
    pub source_input_delivery: SourceInputDeliveryMode,
    /// Server-authorized toplevel derivation identity. For current
    /// `server_derivation` jobs this is the same as `derivation_path`; for
    /// verified source jobs the builder compares its local eval result to this
    /// string before building.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_drv_path: Option<String>,
    /// Server-recorded evaluator fingerprint for audit/debugging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluator: Option<EvaluatorFingerprint>,
    /// Server-selected cache destination for builder-side output pushes. Current
    /// servers always include this field; it remains optional so newer builders
    /// can still communicate with older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_push: Option<BuilderCachePushConfig>,
}

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
    #[serde(default)]
    pub attic_ignore_upstream_cache_filter: bool,
    #[serde(default)]
    pub attic_jobs: u32,
    #[serde(default)]
    pub max_retries: u32,
    #[serde(default)]
    pub retry_delay_seconds: u64,
    #[serde(default = "CacheConfig::default_push_timeout_seconds")]
    pub push_timeout_seconds: u64,
    #[serde(default)]
    pub force_repush: bool,
    #[serde(default)]
    pub require_sigs: bool,
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
            attic_ignore_upstream_cache_filter: true,
            attic_jobs: 5,
            max_retries: 3,
            retry_delay_seconds: 5,
            push_timeout_seconds: CacheConfig::default_push_timeout_seconds(),
            force_repush: false,
            require_sigs: true,
        }
    }

    pub fn to_cache_config(&self, local_fallback: &CacheConfig) -> CacheConfig {
        CacheConfig {
            cache_type: self.cache_type.clone(),
            push_to: self.push_to.clone(),
            push_after_build: self.push_after_build,
            signing_key: self
                .signing_key
                .clone()
                .or_else(|| local_fallback.signing_key.clone()),
            compression: self.compression.clone(),
            push_filter: None,
            parallel_uploads: local_fallback.parallel_uploads,
            s3_region: self.s3_region.clone(),
            s3_profile: self.s3_profile.clone(),
            s3_access_key_id: self.s3_access_key_id.clone(),
            s3_secret_access_key: self.s3_secret_access_key.clone(),
            s3_session_token: self.s3_session_token.clone(),
            s3_endpoint_url: self.s3_endpoint_url.clone(),
            attic_token: self.attic_token.clone(),
            attic_cache_name: self.attic_cache_name.clone(),
            attic_ignore_upstream_cache_filter: self.attic_ignore_upstream_cache_filter,
            attic_jobs: if self.attic_jobs == 0 {
                local_fallback.attic_jobs
            } else {
                self.attic_jobs
            },
            max_retries: self.max_retries,
            retry_delay_seconds: self.retry_delay_seconds,
            poll_interval: local_fallback.poll_interval,
            push_timeout_seconds: self.push_timeout_seconds,
            force_repush: self.force_repush,
            require_sigs: self.require_sigs,
        }
    }
}

/// Signed request body for POST /api/v1/builders/:id/next-job.
///
/// Older builders poll the same endpoint with GET and an empty body; the server
/// treats those as protocol v1 builders that support only `server_derivation` so
/// they never receive newer source-verified jobs by accident.
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

/// Response returned by GET/POST /api/v1/builders/:id/next-job.
///
/// Embeds both the claimed job and the derivation build payload so the remote
/// builder needs only a single round trip and no database connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextJobResponse {
    pub job: BuildJob,
    pub derivation: BuildJobDerivation,
}

/// Build progress report sent by API builders (HTTP fallback for the WS frame).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildProgressRequest {
    pub derivation_id: i32,
    pub elapsed_seconds: i32,
    pub current_target: Option<String>,
    pub last_activity_seconds: i32,
}

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

/// Response for builder creation with generated keypair
/// WARNING: private_key is returned ONLY ONCE and never stored
#[derive(Debug, Serialize, Deserialize)]
pub struct BuilderCreatedResponse {
    pub builder: Builder,
    /// Base64-encoded Ed25519 private key (64 bytes)
    /// This is shown ONLY ONCE at creation time and NEVER stored server-side
    pub private_key: Option<String>,
    pub assigned_environment_ids: Vec<Uuid>,
}

/// Response for keypair regeneration
/// WARNING: private_key is returned ONLY ONCE and never stored
#[derive(Debug, Serialize, Deserialize)]
pub struct KeypairRegeneratedResponse {
    /// Base64-encoded Ed25519 public key (32 bytes)
    pub public_key: String,
    /// Base64-encoded Ed25519 private key (64 bytes)
    /// This is shown ONLY ONCE at creation time and NEVER stored server-side
    pub private_key: String,
}
