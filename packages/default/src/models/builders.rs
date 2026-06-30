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
}

/// Response returned when a builder public key has been registered/approved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveBuilderIdResponse {
    pub builder_id: Uuid,
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
}

/// Response returned by GET /api/v1/builders/:id/next-job.
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
