use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct SystemEvent {
    pub id: Uuid,
    pub system_id: Uuid,
    pub event_type: String,
    pub event_rank: i16,
    pub dedupe_key: String,
    pub correlation_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub previous_generation: Option<i64>,
    pub new_generation: Option<i64>,
    pub previous_store_path: Option<String>,
    pub new_store_path: Option<String>,
    pub previous_boot_id: Option<String>,
    pub new_boot_id: Option<String>,
    pub deployment_id: Option<Uuid>,
    pub desired_target_id: Option<Uuid>,
    pub source: String,
    pub actor: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEventType {
    SystemReboot,
    AgentRestart,
    CfDeploymentStarted,
    CfDeploymentSucceeded,
    CfDeploymentFailed,
    LocalRebuildDetected,
}

impl SystemEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SystemReboot => "system_reboot",
            Self::AgentRestart => "agent_restart",
            Self::CfDeploymentStarted => "cf_deployment_started",
            Self::CfDeploymentSucceeded => "cf_deployment_succeeded",
            Self::CfDeploymentFailed => "cf_deployment_failed",
            Self::LocalRebuildDetected => "local_rebuild_detected",
        }
    }

    pub fn rank(self) -> i16 {
        match self {
            Self::CfDeploymentStarted
            | Self::CfDeploymentSucceeded
            | Self::CfDeploymentFailed
            | Self::LocalRebuildDetected => 10,
            Self::SystemReboot => 20,
            Self::AgentRestart => 30,
        }
    }
}
