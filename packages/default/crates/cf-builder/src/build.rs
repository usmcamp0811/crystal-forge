//! Build execution types used by the builder binary.

use anyhow::Result;
use cf_protocol::builder::BuildJobDerivation;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// A callback for receiving build log lines.
pub type LogSink = Arc<dyn Fn(String) + Send + Sync>;

/// Error returned when a build job is cancelled during execution.
#[derive(Debug, Clone)]
pub struct BuildCancelledError;

impl std::fmt::Display for BuildCancelledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "build was cancelled")
    }
}

impl std::error::Error for BuildCancelledError {}

// ─────────────────────────────────────────────────────────────────────────────
// Build progress reporting traits (builder-side, no DB dependency)
// ─────────────────────────────────────────────────────────────────────────────

/// Progress snapshot emitted periodically during a streaming build.
#[derive(Debug, Clone)]
pub struct BuildProgress {
    pub derivation_id: i32,
    pub elapsed_seconds: i32,
    pub current_target: Option<String>,
    pub last_activity_seconds: i32,
}

/// Abstraction over build progress reporting and cancellation.
///
/// The API builder implements this using HTTP/WebSocket calls to the server.
/// Server-side workers implement it against the database.
#[async_trait::async_trait]
pub trait BuildReporter: Send + Sync {
    async fn report_progress(&self, progress: &BuildProgress) -> Result<()>;
    async fn is_cancelled(&self, job_id: Option<Uuid>) -> Result<bool>;
}

/// Derivation type for builder-side classification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DerivationType {
    NixOS,
    Package,
}

impl ToString for DerivationType {
    fn to_string(&self) -> String {
        match self {
            DerivationType::NixOS => "nixos".into(),
            DerivationType::Package => "package".into(),
        }
    }
}

/// Builder-side derivation representation.
///
/// This is the in-memory form used by the builder during build execution.
/// No SQLx dependency — the builder never reads from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Derivation {
    pub id: i32,
    pub commit_id: Option<i32>,
    pub derivation_type: DerivationType,
    pub derivation_name: String,
    pub derivation_path: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub attempt_count: i32,
    pub evaluation_duration_ms: Option<i32>,
    pub error_message: Option<String>,
    pub pname: Option<String>,
    pub version: Option<String>,
    pub status_id: i32,
    pub derivation_target: Option<String>,
    #[serde(default)]
    pub build_elapsed_seconds: Option<i32>,
    #[serde(default)]
    pub build_current_target: Option<String>,
    #[serde(default)]
    pub build_last_activity_seconds: Option<i32>,
    #[serde(default)]
    pub build_last_heartbeat: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cf_agent_enabled: Option<bool>,
    pub store_path: Option<String>,
}

impl Derivation {
    /// Construct a builder-side `Derivation` from the API build payload.
    /// Only the fields required to run and report a build are populated.
    pub fn from_build_payload(payload: &BuildJobDerivation) -> Self {
        Derivation {
            id: payload.id,
            commit_id: None,
            derivation_type: match payload.derivation_type.as_str() {
                "package" => DerivationType::Package,
                _ => DerivationType::NixOS,
            },
            derivation_name: payload.derivation_name.clone(),
            derivation_path: payload
                .expected_drv_path
                .clone()
                .or_else(|| payload.derivation_path.clone()),
            scheduled_at: None,
            completed_at: None,
            started_at: None,
            attempt_count: 0,
            evaluation_duration_ms: None,
            error_message: None,
            pname: None,
            version: None,
            status_id: 0,
            derivation_target: None,
            build_elapsed_seconds: None,
            build_current_target: None,
            build_last_activity_seconds: None,
            build_last_heartbeat: None,
            cf_agent_enabled: None,
            store_path: payload.store_path.clone(),
        }
    }
}
