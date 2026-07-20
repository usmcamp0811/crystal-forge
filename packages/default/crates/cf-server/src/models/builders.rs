//! Builder models for multi-builder API support.
//!
//! Builders are registered build workers that communicate with the server via API.
//! Each builder has:
//! - Unique UUID identifier
//! - Ed25519 public key for request signing
//! - Resource limits (CPU, memory, concurrent jobs)
//! - Status tracking (active/inactive/offline)
//! - Environment assignments (1:many relationship)
//!
//! # Module layout
//!
//! Wire protocol types that are shared between server, builder, and agent are
//! defined in `cf-protocol` and re-exported here for backward compatibility.
//! Server-only types (those with `sqlx::FromRow` derives or that reference
//! server-internal state) remain defined in this module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::config::{CacheConfig, CacheType};
use crate::models::public_key::PublicKey;

// ─────────────────────────────────────────────────────────────────────────────
// Re-exports: wire protocol types from cf-protocol (no SQLx, no server deps)
// ─────────────────────────────────────────────────────────────────────────────

// Re-export pure wire-protocol types (no SQLx derives).
// NOTE: BuildJob is intentionally NOT re-exported from cf-protocol here.
// The server keeps its own BuildJob (with sqlx::FromRow) for DB queries.
// When cf-builder is extracted, it will use cf_protocol::builder::BuildJob
// directly. The server maps BuildJob → cf_protocol::builder::BuildJob when
// building NextJobResponse.
pub use cf_protocol::builder::{
    AppendLogsRequest, BuildFailurePhase, BuildJobDerivation, BuildProgressRequest,
    BuilderCachePushConfig, CachePushCompleteRequest, CachePushFailRequest, CachePushJobPayload,
    CveScanFailRequest, CveScanResultsRequest, CveScanTarget, DerivationArchiveRequest,
    DerivationManifestResponse, EstablishBuilderSessionRequest, EstablishBuilderSessionResponse,
    EvaluatorFingerprint, NextJobRequest, RemoteBuildExecutionStrategy, ReportMetricsRequest,
    ResolveBuilderIdRequest, ResolveBuilderIdResponse, SourceInputDeliveryMode,
    VerifiedSourceIdentity,
};

// Re-export NextJobResponse as an alias using the protocol's BuildJob type.
// Server handlers that build NextJobResponse should convert from BuildJob (server)
// → cf_protocol::builder::BuildJob using the From impl below.
pub use cf_protocol::builder::NextJobResponse;

// ─────────────────────────────────────────────────────────────────────────────
// Server-side extensions for wire types
// ─────────────────────────────────────────────────────────────────────────────

/// Extension trait providing server-side conversions for `BuilderCachePushConfig`.
///
/// The `disabled()` associated function is defined in `cf-protocol` directly on
/// `BuilderCachePushConfig`, so it is NOT part of this trait.
pub trait BuilderCachePushConfigExt {
    /// Convert the builder-delivered cache-push configuration to the server's
    /// local `CacheConfig`, filling in any missing fields from `local_fallback`.
    fn to_cache_config(&self, local_fallback: &CacheConfig) -> CacheConfig;
}

impl BuilderCachePushConfigExt for BuilderCachePushConfig {
    fn to_cache_config(&self, local_fallback: &CacheConfig) -> CacheConfig {
        // Map cf-protocol CacheType → server CacheType
        let cache_type = match &self.cache_type {
            cf_protocol::cache::CacheType::S3 => CacheType::S3,
            cf_protocol::cache::CacheType::Attic => CacheType::Attic,
            cf_protocol::cache::CacheType::Http => CacheType::Http,
            cf_protocol::cache::CacheType::Nix => CacheType::Nix,
        };
        CacheConfig {
            cache_type,
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
            attic_public_key: self.attic_public_key.clone(),
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

// ─────────────────────────────────────────────────────────────────────────────
// Server DB row types for build_jobs (sqlx::FromRow — server-only)
// ─────────────────────────────────────────────────────────────────────────────

/// Server-owned build job row used for both DB queries AND the wire protocol.
///
/// This type has `sqlx::FromRow` for efficient DB queries.  When the server
/// returns `NextJobResponse`, it converts a `BuildJob` into
/// `cf_protocol::builder::BuildJob` via `Into::into`.  The `cf-builder` crate
/// (extracted in a later step) will use `cf_protocol::builder::BuildJob`
/// directly and will never need `sqlx::FromRow`.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BuildJob {
    pub id: Uuid,
    pub builder_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(default)]
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

/// Conversion from server's BuildJob to cf-protocol wire BuildJob.
///
/// Used by the API layer when constructing `NextJobResponse`.
impl From<BuildJob> for cf_protocol::builder::BuildJob {
    fn from(job: BuildJob) -> Self {
        Self {
            id: job.id,
            builder_id: job.builder_id,
            builder_session_id: job.builder_session_id,
            derivation_id: job.derivation_id,
            environment_id: job.environment_id,
            status: job.status,
            retry_count: job.retry_count,
            max_retries: job.max_retries,
            priority_weight: job.priority_weight,
            started_at: job.started_at,
            completed_at: job.completed_at,
            logs: job.logs,
            created_at: job.created_at,
            updated_at: job.updated_at,
        }
    }
}

/// Internal alias for backward compatibility with query code.
pub type BuildJobRow = BuildJob;

// ─────────────────────────────────────────────────────────────────────────────
// Builder status enum (server-owned: sqlx::Type for DB column mapping)
// ─────────────────────────────────────────────────────────────────────────────

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
            _ => BuilderStatus::Inactive,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Server DB row types (sqlx::FromRow — server-only)
// ─────────────────────────────────────────────────────────────────────────────

/// A registered builder (server DB row).
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

/// Builder assigned environment info for summary view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderEnvironmentInfo {
    pub name: String,
    pub color_hex: String,
}

/// Summary view of a builder (for list endpoints).
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
    #[serde(default)]
    #[sqlx(default, json)]
    pub assigned_environments: Vec<BuilderEnvironmentInfo>,
    #[serde(default)]
    #[sqlx(default)]
    pub public_key_fingerprint: String,
    pub registered: bool,
    #[serde(default)]
    #[sqlx(default)]
    pub load_avg: Option<f64>,
    #[serde(default)]
    #[sqlx(default)]
    pub completed_24h: i32,
    #[serde(default)]
    #[sqlx(default)]
    pub failed_24h: i32,
}

impl Builder {
    pub fn with_public_key_fingerprint(mut self) -> Self {
        self.public_key_fingerprint = self.public_key.fingerprint();
        self
    }
}

/// Builder with environment assignments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderWithEnvironments {
    #[serde(flatten)]
    pub builder: Builder,
    pub assigned_environment_ids: Vec<Uuid>,
}

/// Builder environment assignment (server DB row).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BuilderEnvironmentAssignment {
    pub id: i32,
    pub builder_id: Uuid,
    pub environment_id: Uuid,
    pub created_at: DateTime<Utc>,
}

/// Request to create a new builder.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateBuilderRequest {
    pub name: String,
    pub host: Option<String>,
    pub arch: String,
    pub public_key: Option<String>,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: Option<i32>,
    pub enabled: Option<bool>,
    pub environment_ids: Vec<Uuid>,
}

/// Request to update a builder.
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

/// Request to update builder public key.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBuilderPublicKeyRequest {
    pub public_key: String,
}

/// Request to update builder environment assignments.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBuilderEnvironmentsRequest {
    pub environment_ids: Vec<Uuid>,
}

/// Builder resource metrics (server DB row).
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

/// Response for builder creation with generated keypair.
///
/// WARNING: private_key is returned ONLY ONCE and never stored.
#[derive(Debug, Serialize, Deserialize)]
pub struct BuilderCreatedResponse {
    pub builder: Builder,
    /// Base64-encoded Ed25519 private key (64 bytes).
    /// This is shown ONLY ONCE at creation time and NEVER stored server-side.
    pub private_key: Option<String>,
    pub assigned_environment_ids: Vec<Uuid>,
}

/// Response for keypair regeneration.
///
/// WARNING: private_key is returned ONLY ONCE and never stored.
#[derive(Debug, Serialize, Deserialize)]
pub struct KeypairRegeneratedResponse {
    /// Base64-encoded Ed25519 public key (32 bytes).
    pub public_key: String,
    /// Base64-encoded Ed25519 private key (64 bytes).
    /// This is shown ONLY ONCE at creation time and NEVER stored server-side.
    pub private_key: String,
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
