use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum NotificationDeliveryChannel {
    InApp,
    Email,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum UserNotificationCategory {
    DeployFailures,
    BuildFailures,
    CriticalCves,
    PolicyViolations,
    HeartbeatLost,
}

#[derive(Debug, Clone, FromRow)]
pub struct UserNotificationPreferences {
    pub user_id: Uuid,
    pub deploy_failures: bool,
    pub build_failures: bool,
    pub critical_cves: bool,
    pub policy_violations: bool,
    pub heartbeat_lost: bool,
    pub weekly_digest: bool,
    pub delivery_channel: NotificationDeliveryChannel,
    pub deploy_failures_email_enabled_at: Option<DateTime<Utc>>,
    pub build_failures_email_enabled_at: Option<DateTime<Utc>>,
    pub critical_cves_email_enabled_at: Option<DateTime<Utc>>,
    pub policy_violations_email_enabled_at: Option<DateTime<Utc>>,
    pub heartbeat_lost_email_enabled_at: Option<DateTime<Utc>>,
    pub deploy_failures_in_app_enabled_at: Option<DateTime<Utc>>,
    pub build_failures_in_app_enabled_at: Option<DateTime<Utc>>,
    pub critical_cves_in_app_enabled_at: Option<DateTime<Utc>>,
    pub policy_violations_in_app_enabled_at: Option<DateTime<Utc>>,
    pub heartbeat_lost_in_app_enabled_at: Option<DateTime<Utc>>,
    pub weekly_digest_enabled_at: Option<DateTime<Utc>>,
    pub initialized_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct UserNotification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub category: UserNotificationCategory,
    pub source_occurrence_id: Option<Uuid>,
    pub source_type: String,
    pub source_id: String,
    pub title: String,
    pub summary: String,
    pub route: String,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub dismissed_at: Option<DateTime<Utc>>,
}
