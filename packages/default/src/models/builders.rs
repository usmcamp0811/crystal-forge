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
}

impl ToString for BuilderStatus {
    fn to_string(&self) -> String {
        match self {
            BuilderStatus::Active => "active".into(),
            BuilderStatus::Inactive => "inactive".into(),
            BuilderStatus::Offline => "offline".into(),
        }
    }
}

impl From<String> for BuilderStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "active" => BuilderStatus::Active,
            "inactive" => BuilderStatus::Inactive,
            "offline" => BuilderStatus::Offline,
            _ => BuilderStatus::Inactive, // default to inactive for unknown values
        }
    }
}

/// A registered builder
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Builder {
    pub id: Uuid,
    pub name: String,
    pub public_key: PublicKey,
    pub status: BuilderStatus,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: i32,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Summary view of a builder (for list endpoints)
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BuilderSummary {
    pub id: Uuid,
    pub name: String,
    pub status: BuilderStatus,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: i32,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub assigned_environment_count: i32,
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
    /// Optional base64-encoded Ed25519 public key
    /// If not provided, server will generate a proper Ed25519 keypair
    pub public_key: Option<String>,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: Option<i32>,
    pub environment_ids: Vec<Uuid>, // Optional environment assignments
}

/// Request to update a builder
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBuilderRequest {
    pub name: Option<String>,
    pub status: Option<BuilderStatus>,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: Option<i32>,
}

/// Request to update builder public key
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBuilderPublicKeyRequest {
    pub public_key: String, // base64-encoded Ed25519 public key
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
    Success,
    Failed,
}

impl std::fmt::Display for BuildJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildJobStatus::Queued => write!(f, "queued"),
            BuildJobStatus::Building => write!(f, "building"),
            BuildJobStatus::Success => write!(f, "success"),
            BuildJobStatus::Failed => write!(f, "failed"),
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

/// Response for builder creation with generated keypair
/// WARNING: private_key is returned ONLY ONCE and never stored
#[derive(Debug, Serialize, Deserialize)]
pub struct BuilderCreatedResponse {
    pub builder: Builder,
    /// Base64-encoded Ed25519 private key (64 bytes)
    /// This is shown ONLY ONCE at creation time and NEVER stored server-side
    pub private_key: String,
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
