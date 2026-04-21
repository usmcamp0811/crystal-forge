//! Core types for systemd hardening analysis.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Status of a hardening scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar")]
pub enum ScanStatus {
    #[sqlx(rename = "pending")]
    Pending,
    #[sqlx(rename = "in_progress")]
    InProgress,
    #[sqlx(rename = "completed")]
    Completed,
    #[sqlx(rename = "failed")]
    Failed,
}

impl std::fmt::Display for ScanStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanStatus::Pending => write!(f, "pending"),
            ScanStatus::InProgress => write!(f, "in_progress"),
            ScanStatus::Completed => write!(f, "completed"),
            ScanStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Risk level based on hardening score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar")]
pub enum RiskLevel {
    #[sqlx(rename = "well_hardened")]
    WellHardened,
    #[sqlx(rename = "moderately_hardened")]
    ModeratelyHardened,
    #[sqlx(rename = "poorly_hardened")]
    PoorlyHardened,
    #[sqlx(rename = "vulnerable")]
    Vulnerable,
}

impl RiskLevel {
    /// Calculate risk level from a hardening score (0-100).
    pub fn from_score(score: i32) -> Self {
        match score {
            80..=100 => RiskLevel::WellHardened,
            60..=79 => RiskLevel::ModeratelyHardened,
            40..=59 => RiskLevel::PoorlyHardened,
            _ => RiskLevel::Vulnerable,
        }
    }

    /// Get CSS-friendly color class for this risk level.
    pub fn color_class(&self) -> &'static str {
        match self {
            RiskLevel::WellHardened => "green",
            RiskLevel::ModeratelyHardened => "yellow",
            RiskLevel::PoorlyHardened => "orange",
            RiskLevel::Vulnerable => "red",
        }
    }
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::WellHardened => write!(f, "Well Hardened"),
            RiskLevel::ModeratelyHardened => write!(f, "Moderately Hardened"),
            RiskLevel::PoorlyHardened => write!(f, "Poorly Hardened"),
            RiskLevel::Vulnerable => write!(f, "Vulnerable"),
        }
    }
}

/// A hardening scan record.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct HardeningScan {
    pub id: Uuid,
    pub derivation_id: i32,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: ScanStatus,
    pub attempts: i32,
    pub total_services: i32,
    pub well_hardened_count: i32,
    pub moderately_hardened_count: i32,
    pub poorly_hardened_count: i32,
    pub vulnerable_count: i32,
    pub overall_score: Option<i32>,
    pub scan_duration_ms: Option<i32>,
    pub scan_metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl HardeningScan {
    /// Create a new pending hardening scan.
    pub fn new(derivation_id: i32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            derivation_id,
            scheduled_at: Some(now),
            started_at: None,
            completed_at: None,
            status: ScanStatus::Pending,
            attempts: 0,
            total_services: 0,
            well_hardened_count: 0,
            moderately_hardened_count: 0,
            poorly_hardened_count: 0,
            vulnerable_count: 0,
            overall_score: None,
            scan_duration_ms: None,
            scan_metadata: None,
            created_at: now,
        }
    }

    /// Get the risk level based on overall score.
    pub fn risk_level(&self) -> Option<RiskLevel> {
        self.overall_score.map(RiskLevel::from_score)
    }
}

/// Detail about a single hardening directive for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectiveDetail {
    /// Name of the directive (e.g., "PrivateTmp")
    pub name: String,
    /// Whether this directive is enabled/configured
    pub enabled: bool,
    /// The actual value from the config (for directives with multiple values)
    pub value: serde_json::Value,
    /// Points awarded for this directive's configuration
    pub points: i32,
    /// Maximum possible points for this directive
    pub max_points: i32,
    /// Category of this directive
    pub category: String,
    /// Human-readable description of what this directive does
    pub description: String,
}

/// Per-service hardening result.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ServiceHardeningResult {
    pub id: Uuid,
    pub scan_id: Uuid,
    pub service_name: String,
    pub service_type: Option<String>,
    pub hardening_score: i32,
    pub risk_level: RiskLevel,
    pub directives_detail: serde_json::Value,
    pub enabled_directives_count: i32,
    pub disabled_directives_count: i32,
    pub missing_directives_count: i32,
    pub created_at: DateTime<Utc>,
}

impl ServiceHardeningResult {
    /// Get parsed directive details.
    pub fn get_directives(&self) -> Vec<DirectiveDetail> {
        serde_json::from_value(self.directives_detail.clone()).unwrap_or_default()
    }
}

/// Justification for a service's hardening posture.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct HardeningJustification {
    pub id: Uuid,
    pub system_id: Uuid,
    pub service_name: String,
    pub directive_name: Option<String>,
    pub category: Option<String>,
    pub reason: String,
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Justification categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JustificationCategory {
    /// Service requires elevated capabilities by design
    RequiredCapability,
    /// Legacy service that cannot be easily hardened
    LegacyService,
    /// Hardening is handled externally (container, VM, etc.)
    ExternalHardening,
    /// False positive - directive doesn't apply to this service
    FalsePositive,
    /// Temporary exception with planned remediation
    TemporaryException,
    /// Other documented reason
    Other,
}

impl JustificationCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            JustificationCategory::RequiredCapability => "required_capability",
            JustificationCategory::LegacyService => "legacy_service",
            JustificationCategory::ExternalHardening => "external_hardening",
            JustificationCategory::FalsePositive => "false_positive",
            JustificationCategory::TemporaryException => "temporary_exception",
            JustificationCategory::Other => "other",
        }
    }
}

impl std::fmt::Display for JustificationCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JustificationCategory::RequiredCapability => write!(f, "Required Capability"),
            JustificationCategory::LegacyService => write!(f, "Legacy Service"),
            JustificationCategory::ExternalHardening => write!(f, "External Hardening"),
            JustificationCategory::FalsePositive => write!(f, "False Positive"),
            JustificationCategory::TemporaryException => write!(f, "Temporary Exception"),
            JustificationCategory::Other => write!(f, "Other"),
        }
    }
}

/// Summary of hardening posture for a system (from view_system_hardening_posture).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SystemHardeningPosture {
    pub derivation_id: i32,
    pub config_name: String,
    pub system_id: Option<Uuid>,
    pub hostname: Option<String>,
    pub environment_name: Option<String>,
    pub latest_scan_id: Option<Uuid>,
    pub overall_score: Option<i32>,
    pub risk_level: Option<RiskLevel>,
    pub total_services: Option<i32>,
    pub well_hardened_count: Option<i32>,
    pub moderately_hardened_count: Option<i32>,
    pub poorly_hardened_count: Option<i32>,
    pub vulnerable_count: Option<i32>,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub scan_duration_ms: Option<i32>,
}

/// Fleet-wide hardening summary (from view_hardening_fleet_summary).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetHardeningSummary {
    pub total_systems_scanned: i64,
    pub avg_fleet_score: Option<f64>,
    pub total_well_hardened_services: i64,
    pub total_moderately_hardened_services: i64,
    pub total_poorly_hardened_services: i64,
    pub total_vulnerable_services: i64,
    pub total_services_scanned: i64,
    pub last_scan_completed: Option<DateTime<Utc>>,
}

/// Entry in the top vulnerable services list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopVulnerableService {
    pub service_name: String,
    pub affected_systems_count: i64,
    pub avg_score: f64,
    pub min_score: i32,
    pub max_score: i32,
}
