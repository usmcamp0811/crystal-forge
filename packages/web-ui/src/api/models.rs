//! Client-side API data transfer objects.
//!
//! These mirror the server-side DTOs in `packages/default/src/api/models.rs`
//! and define the JSON contract between the Crystal Forge server and web UI.
//!
//! Kept as a separate copy (rather than shared crate) because:
//! - The server crate has native dependencies (OpenSSL, SQLx) incompatible with wasm32
//! - Client-side DTOs may diverge (e.g. adding UI-only computed fields)

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
pub use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// FieldUpdate (PATCH semantics helper)
// ─────────────────────────────────────────────────────────────────────────────

/// Tri-state field update semantics for HTTP PATCH requests.
///
/// Distinguishes:
/// - field omitted          → [`FieldUpdate::Unset`]   (preserve stored value)
/// - field present as null  → [`FieldUpdate::Clear`]   (set to NULL)
/// - field present + value  → [`FieldUpdate::Set`]     (write the value)
///
/// `#[serde(default)]` on the containing field maps an omitted key to `Unset`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldUpdate<T> {
    /// Field was not present in the payload; leave the stored value unchanged.
    Unset,
    /// Field was present and explicitly null; clear the stored value.
    Clear,
    /// Field was present with a value; write it.
    Set(T),
}

impl<T> Default for FieldUpdate<T> {
    fn default() -> Self {
        FieldUpdate::Unset
    }
}

impl<T> FieldUpdate<T> {
    /// Returns true when the payload omitted this field entirely.
    pub fn is_unset(&self) -> bool {
        matches!(self, FieldUpdate::Unset)
    }
}

impl<'de, T> Deserialize<'de> for FieldUpdate<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // A present key (even `null`) reaches this deserializer; an omitted key
        // is handled by `#[serde(default)]` on the field, which yields `Unset`.
        let value = Option::<T>::deserialize(deserializer)?;
        Ok(match value {
            Some(inner) => FieldUpdate::Set(inner),
            None => FieldUpdate::Clear,
        })
    }
}

impl<T> Serialize for FieldUpdate<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize transparently as `null` / value. `Unset` serializes as
        // `null`; callers that must omit the key should skip it explicitly.
        match self {
            FieldUpdate::Set(value) => serializer.serialize_some(value),
            FieldUpdate::Unset | FieldUpdate::Clear => serializer.serialize_none(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Common Enums
// ─────────────────────────────────────────────────────────────────────────────

/// Health status derived from heartbeat recency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Offline,
}

impl HealthStatus {
    /// CSS text color class from the design system.
    pub fn color_class(&self) -> &'static str {
        use crate::theme::health;
        match self {
            Self::Healthy => health::HEALTHY_TEXT,
            Self::Warning => health::WARNING_TEXT,
            Self::Critical => health::CRITICAL_TEXT,
            Self::Offline => health::OFFLINE_TEXT,
        }
    }

    /// CSS background color class from the design system.
    pub fn bg_class(&self) -> &'static str {
        use crate::theme::health;
        match self {
            Self::Healthy => health::HEALTHY_BG,
            Self::Warning => health::WARNING_BG,
            Self::Critical => health::CRITICAL_BG,
            Self::Offline => health::OFFLINE_BG,
        }
    }

    /// CSS dot (filled circle) color class from the design system.
    pub fn dot_class(&self) -> &'static str {
        use crate::theme::health;
        match self {
            Self::Healthy => health::HEALTHY_DOT,
            Self::Warning => health::WARNING_DOT,
            Self::Critical => health::CRITICAL_DOT,
            Self::Offline => health::OFFLINE_DOT,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Healthy => "Healthy",
            Self::Warning => "Warning",
            Self::Critical => "Critical",
            Self::Offline => "Offline",
        }
    }
}

/// Deployment status relative to the latest available commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    UpToDate,
    Behind,
    Ahead,
    NeverDeployed,
    NoCommitsAvailable,
    Unknown,
}

impl DeploymentStatus {
    /// CSS text color class from the design system.
    pub fn color_class(&self) -> &'static str {
        use crate::theme::deployment;
        match self {
            Self::UpToDate => deployment::UP_TO_DATE_TEXT,
            Self::Behind => deployment::BEHIND_TEXT,
            Self::Ahead => deployment::AHEAD_TEXT,
            Self::NeverDeployed => deployment::NEVER_DEPLOYED_TEXT,
            Self::NoCommitsAvailable => deployment::NO_COMMITS_TEXT,
            Self::Unknown => deployment::UNKNOWN_TEXT,
        }
    }

    /// CSS background color class from the design system.
    pub fn bg_class(&self) -> &'static str {
        use crate::theme::deployment;
        match self {
            Self::UpToDate => deployment::UP_TO_DATE_BG,
            Self::Behind => deployment::BEHIND_BG,
            Self::Ahead => deployment::AHEAD_BG,
            Self::NeverDeployed => deployment::NEVER_DEPLOYED_BG,
            Self::NoCommitsAvailable => deployment::NO_COMMITS_BG,
            Self::Unknown => deployment::UNKNOWN_BG,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::UpToDate => "Up to Date",
            Self::Behind => "Behind",
            Self::Ahead => "Ahead",
            Self::NeverDeployed => "Never Deployed",
            Self::NoCommitsAvailable => "No Commits",
            Self::Unknown => "Unknown",
        }
    }
}

/// CVE severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CveSeverity {
    Critical,
    High,
    Medium,
    Low,
}

impl CveSeverity {
    /// CSS text color class from the design system.
    pub fn color_class(&self) -> &'static str {
        use crate::theme::cve;
        match self {
            Self::Critical => cve::CRITICAL_TEXT,
            Self::High => cve::HIGH_TEXT,
            Self::Medium => cve::MEDIUM_TEXT,
            Self::Low => cve::LOW_TEXT,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }
}

/// Pipeline stage for a NixOS system's build/deploy lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    DryRun,
    ReadyForBuild,
    Building,
    BuildComplete,
    ReadyForDeploy,
    Unknown,
}

impl PipelineStage {
    /// CSS text color class from the design system.
    pub fn color_class(&self) -> &'static str {
        use crate::theme::pipeline;
        match self {
            Self::DryRun => pipeline::DRY_RUN_TEXT,
            Self::ReadyForBuild => pipeline::READY_FOR_BUILD_TEXT,
            Self::Building => pipeline::BUILDING_TEXT,
            Self::BuildComplete => pipeline::BUILD_COMPLETE_TEXT,
            Self::ReadyForDeploy => pipeline::READY_FOR_DEPLOY_TEXT,
            Self::Unknown => pipeline::UNKNOWN_TEXT,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::DryRun => "Dry Run",
            Self::ReadyForBuild => "Ready for Build",
            Self::Building => "Building",
            Self::BuildComplete => "Build Complete",
            Self::ReadyForDeploy => "Ready for Deploy",
            Self::Unknown => "Unknown",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Dashboard DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Top-level dashboard response aggregating fleet-wide metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub fleet_health: FleetHealthSummary,
    pub deployment_status: DeploymentStatusSummary,
    pub cve_summary: CveSummary,
    pub total_systems: i64,
    pub active_builds: i64,
    pub build_queue: Option<BuildQueueSummary>,
    #[serde(default)]
    pub cache_health: Option<CacheHealthSummary>,
    pub recent_deployments: Vec<RecentDeployment>,
    pub timestamp: DateTime<Utc>,
}

/// System counts grouped by health status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetHealthSummary {
    pub healthy: i64,
    pub warning: i64,
    pub critical: i64,
    pub offline: i64,
}

impl FleetHealthSummary {
    pub fn total(&self) -> i64 {
        self.healthy + self.warning + self.critical + self.offline
    }
}

/// System counts grouped by deployment status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentStatusSummary {
    pub up_to_date: i64,
    pub behind: i64,
    pub never_deployed: i64,
    pub unknown: i64,
}

impl DeploymentStatusSummary {
    pub fn total(&self) -> i64 {
        self.up_to_date + self.behind + self.never_deployed + self.unknown
    }
}

/// Fleet-wide CVE vulnerability counts by severity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveSummary {
    pub critical: i64,
    pub high: i64,
    pub medium: i64,
    pub low: i64,
}

impl CveSummary {
    pub fn total(&self) -> i64 {
        self.critical + self.high + self.medium + self.low
    }
}

/// Admin-only CVE dashboard fleet summary payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveDashboardSummary {
    pub total_open: i64,
    pub severity: CveSummary,
    pub affected_systems: i64,
    pub new_cves_last_7_days: i64,
    pub oldest_cve_age_days: Option<i64>,
}

/// Filters for CVE dashboard drill-down requests.
#[derive(Debug, Clone, Default)]
pub struct CveDashboardVulnerabilityParams {
    pub severity: Option<String>,
    pub status: Option<String>,
    pub system: Option<String>,
    pub environment: Option<String>,
    pub package: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<i64>,
}

/// A CVE row for dashboard drill-down views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveDashboardVulnerability {
    pub system_id: Uuid,
    pub hostname: String,
    pub cve_id: String,
    pub severity: CveSeverity,
    pub cvss_score: Option<f64>,
    pub package_name: String,
    pub installed_version: String,
    pub fixed_version: Option<String>,
    pub first_seen: Option<DateTime<Utc>>,
    pub status: String,
}

/// Top-affected system row for CVE dashboard visualization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveDashboardTopSystem {
    pub system_id: Uuid,
    pub hostname: String,
    pub total_cves: i64,
    pub critical_cves: i64,
    pub high_cves: i64,
    pub medium_cves: i64,
    pub low_cves: i64,
    pub days_since_scan: Option<i64>,
    pub last_cve_scan: Option<DateTime<Utc>>,
}

/// Scan freshness/coverage per system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveScanFreshnessRow {
    pub system_id: Uuid,
    pub hostname: String,
    pub days_since_scan: Option<i64>,
    pub last_cve_scan: Option<DateTime<Utc>>,
    pub total_cves: i64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Advanced CVE Dashboard DTOs (TASK-322)
// ─────────────────────────────────────────────────────────────────────────────

/// Filter parameters for CVE list queries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CveFilters {
    pub severity: Option<String>,
    pub fix_status: Option<String>,
    pub triage_status: Option<String>,
    pub package: Option<String>,
    pub search: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<i64>,
}

/// CVE list item for table views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveListItem {
    pub cve_id: String,
    pub cvss_v3_score: Option<f32>,
    pub severity: String,
    pub title: String,
    pub cvss_vector: Option<String>,
    pub published_date: Option<chrono::NaiveDate>,
    pub exploited: bool,
    pub package_name: Option<String>,
    pub installed_version: Option<String>,
    pub fixed_version: Option<String>,
    pub fix_status: String,
    pub affected_count: i64,
    pub affected_environments: Option<Vec<String>>,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    pub age_days: i32,
    pub triage_status: String,
}

/// CVE package group with aggregated statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CvePackageGroup {
    pub package_name: String,
    pub cve_count: i64,
    pub critical_count: i64,
    pub high_count: i64,
    pub medium_count: i64,
    pub low_count: i64,
    pub environments_count: i64,
    pub total_affected_systems: i64,
    pub fixable_count: i64,
    pub outstanding_count: i64,
    pub exploited_count: i64,
    pub max_cvss: Option<f32>,
    pub severity_score: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cves: Option<Vec<CveListItem>>,
}

/// Detailed CVE information for the drawer view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveDetail {
    pub cve_id: String,
    pub cvss_v3_score: Option<f32>,
    pub severity: String,
    pub title: String,
    pub cvss_vector: Option<String>,
    pub cwe_id: Option<String>,
    pub published_date: Option<chrono::NaiveDate>,
    pub modified_date: Option<chrono::NaiveDate>,
    pub exploited: bool,
    pub package_name: Option<String>,
    pub installed_version: Option<String>,
    pub fixed_version: Option<String>,
    pub detection_method: Option<String>,
    pub fix_status: String,
}

/// System affected by a CVE (for drawer detail view).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveAffectedSystemDetail {
    pub system_id: Uuid,
    pub hostname: String,
    pub environment: Option<String>,
    pub primary_ip_address: Option<String>,
    pub flake_name: Option<String>,
    pub flake_id: Option<i32>,
    pub commit_hash: Option<String>,
    pub deployment_policy: String,
    pub current_package_version: Option<String>,
}

/// CVE justification (triage) record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveJustification {
    pub system_id: Option<Uuid>,
    pub cve_id: String,
    pub category: String,
    pub reason: String,
    pub updated_by: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_by_username: Option<String>,
}

/// Input for creating/updating a CVE justification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveJustificationInput {
    pub system_id: Option<Uuid>,
    pub category: String,
    pub reason: String,
}

/// Fleet-wide CVE statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveFleetStats {
    pub total_cves: i64,
    pub critical: i64,
    pub high: i64,
    pub medium: i64,
    pub low: i64,
    pub exploited: i64,
    pub fixable: i64,
    pub environments_affected: i64,
    pub systems_affected: i64,
    pub outstanding: i64,
    pub accepted: i64,
    pub scheduled: i64,
}

/// Response from fleet rescan trigger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetRescanResponse {
    pub enqueued_count: i64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanSchedulePolicyResponse {
    pub on_build: bool,
    pub deployed_interval: String,
    pub recent_interval: String,
    pub archived_interval: String,
    pub archived_enabled: bool,
    pub rebuild_to_scan: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateScanSchedulePolicyRequest {
    pub on_build: bool,
    pub deployed_interval: String,
    pub recent_interval: String,
    pub archived_interval: String,
    pub archived_enabled: bool,
    pub rebuild_to_scan: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanningStatsResponse {
    pub scanning: i64,
    pub queued: i64,
    pub stale: i64,
    pub never_scanned: i64,
    pub failed: i64,
    pub coverage_percent: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanningQueueItemResponse {
    /// `None` when the system is deployed but has never been scanned.
    pub scan_id: Option<Uuid>,
    pub hostname: String,
    pub flake_name: Option<String>,
    pub commit_hash: Option<String>,
    pub status: String,
    pub completed_at: Option<DateTime<Utc>>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub critical_count: i32,
    pub high_count: i32,
    pub medium_count: i32,
    /// Freshness class: `deployed`, `recent`, or `archived`.
    #[serde(default)]
    pub freshness: String,
    /// True when this is the latest scan row for its derivation.
    #[serde(default)]
    pub is_current: bool,
    /// True when this derivation's commit is the latest known commit for its flake.
    #[serde(default)]
    pub is_latest_per_flake: bool,
    /// Scan trigger source (not yet tracked server-side).
    #[serde(default)]
    pub trigger: Option<String>,
}

/// Paginated deployed configurations response (P2#6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanningDeployedResponse {
    pub items: Vec<ScanningQueueItemResponse>,
    pub total: i64,
    pub has_more: bool,
    /// Opaque cursor for the next page. `None` when this is the last page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanningSystemsItemResponse {
    pub system_id: Uuid,
    pub hostname: String,
    pub environment: Option<String>,
    pub total_configs: i64,
    pub scanned: i64,
    pub stale: i64,
    pub needs_build: i64,
    pub unscanned: i64,
    pub current_crit: i64,
    pub current_high: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanningActivityItemResponse {
    pub at: Option<DateTime<Utc>>,
    pub name: String,
    pub event: String,
    pub detail: String,
    pub status: String,
}

/// A single recent deployment event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentDeployment {
    pub hostname: String,
    pub commit_hash: String,
    pub commit_message: Option<String>,
    pub deployed_at: DateTime<Utc>,
    pub status: DeploymentStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheHealthSummary {
    pub status: CacheHealthStatus,
    pub destination_count: i64,
    pub enabled_destination_count: i64,
    pub successful_pushes_24h: i64,
    pub failed_pushes_24h: i64,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub used_bytes: Option<i64>,
    pub capacity_bytes: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheHealthStatus {
    Healthy,
    Degraded,
    Unknown,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardActivity {
    #[serde(default)]
    pub id: String,
    pub kind: DashboardActivityKind,
    pub status: DashboardActivityStatus,
    pub occurred_at: DateTime<Utc>,
    pub title: String,
    pub system_id: Option<Uuid>,
    pub flake_id: Option<i32>,
    pub commit_id: Option<i32>,
    pub commit_hash: Option<String>,
    pub build_job_id: Option<Uuid>,
    pub deployment_id: Option<Uuid>,
    #[serde(default)]
    pub evaluation_attempt_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardActivityKind {
    Deployment,
    Build,
    Evaluation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardActivityStatus {
    DeploymentStarted,
    DeploymentSucceeded,
    DeploymentFailed,
    BuildQueued,
    BuildBuilding,
    BuildCancelling,
    BuildSucceeded,
    BuildFailed,
    BuildCancelled,
    EvaluationPending,
    EvaluationInProgress,
    EvaluationCancelling,
    EvaluationSucceeded,
    EvaluationFailed,
    EvaluationCancelled,
}

// ─────────────────────────────────────────────────────────────────────────────
// System DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Lightweight system representation for list views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemSummary {
    pub id: Uuid,
    pub hostname: String,
    #[serde(default)]
    pub system_configuration_name: Option<String>,
    pub environment: Option<String>,
    #[serde(default)]
    pub flake_id: Option<i32>,
    /// Primary IP from agent heartbeat. Not included in server list responses
    /// (use [`SystemNetworkInfo`] in [`SystemDetail`] for full network info).
    /// Defaults to `None` for compatibility with the backend DTO.
    #[serde(default)]
    pub primary_ip: Option<String>,
    pub health_status: HealthStatus,
    pub deployment_status: DeploymentStatus,
    pub pipeline_stage: Option<PipelineStage>,
    pub cve_counts: CveSummary,
    pub nixos_version: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub deployment_policy: String,
    #[serde(default)]
    pub fqdn: Option<String>,
    /// Per-system heartbeat interval in seconds. None means the agent uses the server default (600s).
    #[serde(default)]
    pub heartbeat_interval_secs: Option<i32>,
    /// Effective heartbeat interval in seconds: per-system override if set,
    /// otherwise the server-config default. Always present; use this for spinners.
    #[serde(default = "default_effective_heartbeat_interval_secs")]
    pub effective_heartbeat_interval_secs: i32,
    /// Linux kernel boot UUID from /proc/sys/kernel/random/boot_id.
    /// Used to distinguish system reboots from agent restarts.
    #[serde(default)]
    pub boot_id: Option<String>,
}

/// Full system representation for the detail view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemDetail {
    pub id: Uuid,
    pub hostname: String,
    #[serde(default)]
    pub fqdn: Option<String>,
    #[serde(default)]
    pub system_configuration_name: Option<String>,
    pub environment: Option<String>,
    pub is_active: bool,
    pub deployment_policy: String,
    pub health_status: HealthStatus,
    pub deployment_status: DeploymentStatus,
    pub pipeline_stage: Option<PipelineStage>,
    pub nixos_version: Option<String>,
    pub kernel: Option<String>,
    pub agent_version: Option<String>,
    pub current_store_path: Option<String>,
    #[serde(default)]
    pub generation: Option<i32>,
    #[serde(default)]
    pub generation_matches_current_store_path: Option<bool>,
    pub hardware: SystemHardwareInfo,
    pub network: SystemNetworkInfo,
    pub security: SystemSecurityInfo,
    pub cve_counts: CveSummary,
    pub flake: Option<FlakeSummary>,
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Per-system heartbeat interval in seconds. None means the agent uses the server default (600s).
    #[serde(default)]
    pub heartbeat_interval_secs: Option<i32>,
    /// Effective heartbeat interval in seconds: per-system override if set,
    /// otherwise the server-config default. Always present; use this for spinners.
    #[serde(default = "default_effective_heartbeat_interval_secs")]
    pub effective_heartbeat_interval_secs: i32,
    /// Linux kernel boot UUID from /proc/sys/kernel/random/boot_id.
    /// Used to distinguish system reboots from agent restarts.
    #[serde(default)]
    pub boot_id: Option<String>,
    /// Authoritative restart classification: "system_reboot", "agent_restart", "unknown", or None.
    #[serde(default)]
    pub restart_type: Option<String>,
    /// Timestamp of the heartbeat that triggered the last restart classification.
    #[serde(default)]
    pub last_restart_at: Option<DateTime<Utc>>,
}

/// Hardware information subset for system detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemHardwareInfo {
    pub cpu_brand: Option<String>,
    pub cpu_cores: Option<i32>,
    pub memory_gb: Option<f64>,
    pub uptime_secs: Option<i64>,
    pub board_serial: Option<String>,
    pub bios_version: Option<String>,
}

/// Network information subset for system detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemNetworkInfo {
    pub primary_ip: Option<String>,
    pub primary_mac: Option<String>,
    pub gateway_ip: Option<String>,
    #[serde(default = "default_system_reachability")]
    pub reachability: String,
}

fn default_system_reachability() -> String {
    "direct".to_string()
}

/// Fallback used by `#[serde(default)]` when the server omits
/// `effective_heartbeat_interval_secs` (older server version).
fn default_effective_heartbeat_interval_secs() -> i32 {
    600
}

/// Security posture subset for system detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemSecurityInfo {
    pub tpm_present: Option<bool>,
    pub secure_boot_enabled: Option<bool>,
    pub fips_mode: Option<bool>,
    pub selinux_status: Option<String>,
}

/// Flake context for a system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeSummary {
    pub id: i32,
    pub name: String,
    pub repo_url: String,
    pub latest_commit: Option<String>,
}

/// Flake registry item used by flakes management.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeRegistryItem {
    pub id: i32,
    pub name: String,
    pub repo_url: String,
    pub branch: String,
    #[serde(default = "default_flake_build_scope")]
    pub build_scope: String,
    pub system_count: i64,
    /// Current sync state: "unknown" | "synced" | "syncing" | "error"
    #[serde(default = "default_sync_status")]
    pub sync_status: String,
    /// Timestamp of the most recent sync attempt (success or failure).
    #[serde(default)]
    pub last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The error text from the most recent failed sync, if any.
    #[serde(default)]
    pub last_sync_error: Option<String>,

    // ----- Enriched fields (TASK-397) -----
    #[serde(default)]
    pub latest_commit_hash: Option<String>,
    #[serde(default)]
    pub latest_commit_message: Option<String>,
    #[serde(default)]
    pub latest_commit_author: Option<String>,
    #[serde(default)]
    pub latest_commit_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub build_status: Option<String>,
    #[serde(default)]
    pub evaluation_status: Option<String>,
    #[serde(default)]
    pub environments: Vec<String>,
    #[serde(default)]
    pub total_commit_count: i64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Evaluation and flake-output snapshot DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Identifies the durable lifecycle of a cached snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotLifecycle {
    /// Evaluation is waiting for a worker.
    Queued,
    /// A worker is extracting the snapshot.
    Running,
    /// Evaluation ended with a safe diagnostic.
    Failed,
    /// The snapshot is available for database-only reads.
    Available,
    /// No reusable snapshot exists for the revision.
    Unavailable,
}

/// Selects the baseline semantics for an evaluated-options request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotRevisionMode {
    /// Compares with the selected commit's Git first parent.
    #[default]
    Commit,
    /// Compares with the preceding retained generation snapshot.
    Generation,
}

impl SnapshotRevisionMode {
    /// Returns the server query value.
    pub fn as_query_value(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Generation => "generation",
        }
    }
}

/// Selects the server-side evaluated-option subset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluatedOptionFilter {
    /// Returns every matching option.
    #[default]
    All,
    /// Returns options with proven overridden definitions.
    Overridden,
    /// Returns options that differ from a valid baseline.
    Changed,
}

impl EvaluatedOptionFilter {
    /// Returns the server query value.
    pub fn as_query_value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Overridden => "overridden",
            Self::Changed => "changed",
        }
    }
}

/// Defines one bounded evaluated-options request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedOptionsRequest {
    /// Full immutable revision SHA.
    pub revision: String,
    /// Retained generation in generation mode.
    pub generation: Option<i32>,
    /// Comparison mode.
    pub mode: SnapshotRevisionMode,
    /// Debounced server-side search text.
    pub search: String,
    /// Active subset.
    pub filter: EvaluatedOptionFilter,
    /// Requested bounded page size.
    pub limit: i64,
    /// Requested bounded zero-based offset.
    pub offset: i64,
    /// Opaque token that binds the request to one selected artifact and baseline.
    pub snapshot_token: Option<String>,
}

/// Reports revision-global option counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluatedOptionCounts {
    /// Number of options in the selected snapshot.
    pub all: i64,
    /// Number with proven overridden definitions.
    pub overridden: i64,
    /// Number changed from a valid baseline, or no count without a baseline.
    pub changed: Option<i64>,
}

/// Returns one bounded page of evaluated options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluatedOptionsPage {
    /// Selected snapshot lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Full selected revision SHA.
    pub revision: String,
    /// Selected local generation identity in generation mode.
    #[serde(default)]
    pub generation: Option<i32>,
    /// Durable retained-generation snapshot identity.
    #[serde(default)]
    pub generation_snapshot_id: Option<Uuid>,
    /// Opaque token for the exact selected artifact and comparison baseline.
    #[serde(default)]
    pub snapshot_token: Option<String>,
    /// Full baseline SHA when comparison is available.
    pub baseline_revision: Option<String>,
    /// Preceding retained generation used as the generation-mode baseline.
    #[serde(default)]
    pub baseline_generation: Option<i32>,
    /// Whether Changed has a valid baseline.
    pub comparison_available: bool,
    /// Safe evaluation diagnostic for a failed snapshot.
    pub error: Option<String>,
    /// Number of distinct `(source_input, source_revision, source_path)` tuples.
    pub module_count: i64,
    /// End-to-end evaluator duration in milliseconds.
    pub evaluation_duration_ms: Option<i64>,
    /// Revision-global counts independent of search and active filter.
    pub counts: EvaluatedOptionCounts,
    /// Number of rows matching the active search and filter.
    pub total: i64,
    /// Bounded zero-based offset.
    pub offset: i64,
    /// Bounded page size.
    pub limit: i64,
    /// Rows in this page.
    pub options: Vec<EvaluatedOptionRow>,
}

/// Identifies an active registered flake and one exact active revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedFlakeIdentity {
    /// Registered flake database identity.
    pub flake_id: i32,
    /// Registered flake display name.
    pub flake_name: String,
    /// Registered repository URL after credential sanitization.
    pub repo_url: String,
    /// Full immutable revision.
    pub revision: String,
}

/// Aggregates one exact module source across the complete selected snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationModuleSummary {
    /// Evaluator-provided flake input name.
    pub source_input: Option<String>,
    /// Evaluator-provided full source revision.
    pub source_revision: Option<String>,
    /// Exact Nix module source path.
    pub source_path: String,
    /// Number of definitions emitted by this source.
    pub defined_count: i64,
    /// Number of definitions that won the module merge.
    pub won_count: i64,
    /// Server-issued navigation identity after visibility checks.
    pub tracked_flake: Option<TrackedFlakeIdentity>,
}

/// Classifies selected-versus-running configuration drift by exact store identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationDrift {
    /// Selected and running store paths are exactly equal.
    Matches,
    /// Both exact paths are known and differ.
    Differs,
    /// One or both exact paths are unavailable.
    Unavailable,
}

/// Classifies the selected configuration against the latest agent observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFingerprintStatus {
    /// The selected and latest agent-reported store paths are equal.
    Matches,
    /// Both exact store paths are available and differ.
    Differs,
    /// Either exact store path is unavailable.
    Unavailable,
}

/// Classifies exact running-store observations during the trailing seven days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SevenDayDriftStatus {
    /// Complete coverage contains only the selected store path.
    NoObservedDrift,
    /// Complete coverage contains another exact store path.
    ObservedDrift,
    /// Observation coverage is absent or contains an excessive gap.
    InsufficientCoverage,
}

/// Returns complete selected-revision Config summary metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedEvaluationSummary {
    /// Selected snapshot lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Full selected revision SHA.
    pub revision: String,
    /// Selected retained generation in generation mode.
    pub generation: Option<i32>,
    /// Safe lifecycle or integrity diagnostic.
    pub error: Option<String>,
    /// Opaque token for the exact selected artifact and comparison baseline.
    #[serde(default)]
    pub snapshot_token: Option<String>,
    /// Preceding retained generation used as the generation-mode baseline.
    #[serde(default)]
    pub baseline_generation: Option<i32>,
    /// Authoritative number of module sources in the complete snapshot.
    pub module_source_total: i64,
    /// Snapshot completion timestamp.
    pub completed_at: Option<DateTime<Utc>>,
    /// End-to-end evaluator duration in milliseconds.
    pub evaluation_duration_ms: Option<i64>,
    /// Authoritative option count for the complete snapshot.
    pub option_total: i64,
    /// Exact selected NixOS toplevel store path.
    pub selected_store_path: Option<String>,
    /// Existing closure package count, when calculated.
    pub closure_package_count: Option<i32>,
    /// Recursive Nix closure size in bytes from a complete local measurement.
    pub closure_size_bytes: Option<i64>,
    /// Exact latest running store path.
    pub running_store_path: Option<String>,
    /// Agent-reported profile match for the latest running state.
    pub running_profile_matches: Option<bool>,
    /// Number of selected option states that differ from the same-commit mode.
    pub host_delta_count: Option<i64>,
    /// Exact selected-versus-agent store identity status.
    pub agent_fingerprint: AgentFingerprintStatus,
    /// Exact running-store drift during the trailing seven days.
    pub seven_day_drift: SevenDayDriftStatus,
    /// Exact-store-identity drift classification.
    pub drift: EvaluationDrift,
}

/// Returns one bounded page of module sources for a selected evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationModuleSourcesPage {
    /// Selected snapshot lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Full selected revision SHA.
    pub revision: String,
    /// Selected retained generation in generation mode.
    pub generation: Option<i32>,
    /// Safe lifecycle or integrity diagnostic.
    pub error: Option<String>,
    /// Opaque persisted snapshot token that binds continuation pages.
    pub snapshot_token: Option<String>,
    /// Authoritative number of sources in the complete snapshot.
    pub total: i64,
    /// Applied zero-based offset.
    pub offset: i64,
    /// Applied bounded page size.
    pub limit: i64,
    /// Sources in deterministic server order.
    pub sources: Vec<EvaluationModuleSummary>,
}

/// Adds baseline comparison data to one evaluated option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluatedOptionRow {
    /// Selected revision value and provenance, or no value when removed.
    pub option: Option<EvaluatedOption>,
    /// Baseline value when the option existed there.
    pub before: Option<EvaluatedOption>,
    /// Whether selected and baseline payloads differ.
    pub changed: Option<bool>,
    /// Type-aware change summary when a comparison is available.
    pub diff: Option<TypedOptionDiff>,
}

/// Classifies an option comparison without treating removal as missing data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionChangeKind {
    /// The option exists only in the selected snapshot.
    Added,
    /// The option exists only in the baseline snapshot.
    Removed,
    /// The option exists in both snapshots with different typed content.
    Modified,
    /// The option content is unchanged.
    Unchanged,
}

/// Describes typed additions and removals for an option comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypedOptionDiff {
    /// Option-level change classification.
    pub kind: OptionChangeKind,
    /// Safe value kind used for presentation.
    pub value_kind: String,
    /// Added values, package identities, elements, or attributes.
    pub added: Vec<Value>,
    /// Removed values, package identities, elements, or attributes.
    pub removed: Vec<Value>,
}

/// Represents an evaluated option without fabricating unsupported data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SafeOptionValue {
    /// A JSON scalar.
    Scalar(Value),
    /// Package identity and output metadata.
    Package(SafePackageValue),
    /// A bounded list of tagged values.
    List(Vec<SafeOptionValue>),
    /// A bounded attribute set.
    AttributeSet(serde_json::Map<String, Value>),
    /// A bounded submodule value.
    Submodule(serde_json::Map<String, Value>),
    /// A function or another value that cannot be serialized safely.
    Opaque { type_name: String },
    /// Evaluation did not produce a value.
    Failed(SafeEvaluationError),
}

/// Describes a package without depending on a live store path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafePackageValue {
    /// Package display name.
    pub name: Option<String>,
    /// Package pname.
    pub pname: Option<String>,
    /// Package version.
    pub version: Option<String>,
    /// Evaluated output path.
    pub output_path: Option<String>,
}

/// Describes a failed or deliberately unsupported evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeEvaluationError {
    /// Stable machine-readable failure category.
    pub code: String,
    /// Redacted diagnostic suitable for display.
    pub message: String,
}

/// Identifies one option definition and its source provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionDefinitionProvenance {
    /// Source path reported by the Nix module system.
    pub source_path: String,
    /// Source input when tracked metadata resolved it.
    pub source_input: Option<String>,
    /// Full source revision when resolved.
    pub source_revision: Option<String>,
    /// Safe definition value.
    pub value: Option<Value>,
    /// Whether evaluator metadata identifies this definition as winning.
    pub winning: bool,
    /// Module-system priority when available.
    #[serde(default)]
    pub priority: Option<i64>,
    /// Stable evaluator-provided definition status.
    #[serde(default)]
    pub status: Option<String>,
    /// Safe explanation of why this definition won or lost.
    #[serde(default)]
    pub winner_note: Option<String>,
    /// Server-issued navigation identity after visibility checks.
    #[serde(default)]
    pub tracked_flake: Option<TrackedFlakeIdentity>,
}

/// Contains the safe representation of one NixOS option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluatedOption {
    /// Full option path.
    pub path: String,
    /// Declared NixOS option type.
    pub declared_type: String,
    /// Tagged evaluated value or explicit failure.
    pub value: SafeOptionValue,
    /// Complete provenance emitted by the evaluator.
    pub definitions: Vec<OptionDefinitionProvenance>,
    /// Whether lower-priority definitions are proven to exist.
    pub overridden: bool,
}

/// Reports the result of an explicit evaluation action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueEvaluationResponse {
    /// Full requested revision SHA.
    pub revision: String,
    /// Lifecycle after the idempotent action.
    pub lifecycle: SnapshotLifecycle,
    /// Whether this request changed the revision to queued.
    pub queued: bool,
}

/// Classifies a declared-to-managed flake system relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciledFlakeSystemState {
    /// The selected revision declares a managed configuration.
    Managed,
    /// The selected revision declares an unmanaged configuration.
    DeclaredUnmanaged,
    /// A managed system is absent from the selected revision.
    ManagedUndeclared,
}

/// Represents one system in authoritative flake reconciliation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciledFlakeSystem {
    /// Declared or managed configuration name.
    pub configuration_name: String,
    /// Managed Crystal Forge system.
    pub system_id: Option<Uuid>,
    /// Managed hostname.
    pub hostname: Option<String>,
    /// Visible managed environment name.
    pub environment_name: Option<String>,
    /// Visible managed environment color.
    pub environment_color: Option<String>,
    /// Reconciled relationship state.
    pub state: ReconciledFlakeSystemState,
    /// Full deployed revision when it differs from the selected revision.
    pub deployed_revision: Option<String>,
    /// Whether multiple managed hosts collapse onto this output name.
    pub output_collapsed: bool,
}

/// Describes bounded top-level flake output collection paging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlakeOutputPagination {
    /// Applied zero-based offset.
    pub offset: usize,
    /// Applied per-collection limit.
    pub limit: usize,
    /// Number of visible reconciliation rows for the active systems filter.
    #[serde(default)]
    pub system_total: i64,
    /// Whether another reconciliation row exists after this page.
    #[serde(default)]
    pub systems_has_more: bool,
}

/// Describes one resolved input revision change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlakeInputRevisionBump {
    /// Stable lock node identity.
    pub node: String,
    /// Previous full locked revision.
    pub before: Option<String>,
    /// Selected full locked revision.
    pub after: Option<String>,
}

/// Summarizes selected flake outputs against the Git first parent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlakeOutputDelta {
    /// Exact added-system count before the bounded sample was truncated.
    pub systems_added_total: usize,
    /// Exact removed-system count before the bounded sample was truncated.
    pub systems_removed_total: usize,
    /// Exact added-module count before the bounded sample was truncated.
    pub modules_added_total: usize,
    /// Exact removed-module count before the bounded sample was truncated.
    pub modules_removed_total: usize,
    /// Exact added-input count before the bounded sample was truncated.
    pub inputs_added_total: usize,
    /// Exact removed-input count before the bounded sample was truncated.
    pub inputs_removed_total: usize,
    /// Exact input-revision-change count before the bounded sample was truncated.
    pub input_revision_bumps_total: usize,
    /// Declared systems added at the selected revision.
    pub systems_added: Vec<String>,
    /// Declared systems removed at the selected revision.
    pub systems_removed: Vec<String>,
    /// Exported modules added at the selected revision.
    pub modules_added: Vec<String>,
    /// Exported modules removed at the selected revision.
    pub modules_removed: Vec<String>,
    /// Lock nodes added at the selected revision.
    pub inputs_added: Vec<String>,
    /// Lock nodes removed at the selected revision.
    pub inputs_removed: Vec<String>,
    /// Lock inputs whose resolved revision changed.
    pub input_revision_bumps: Vec<FlakeInputRevisionBump>,
}

/// One safe option declaration exported by a flake module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeModuleDeclaration {
    /// Declared option path.
    pub path: String,
    /// Declared Nix option type.
    pub declared_type: String,
    /// Whether a safe default is present.
    pub has_default: bool,
    /// Safe default value when present.
    pub default: Option<Value>,
    /// Complete declaration source paths.
    pub source_paths: Vec<String>,
}

/// One exported `nixosModules` output and its cached analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeOutputModule {
    /// Exported module name.
    pub name: String,
    /// Module description when emitted.
    pub description: Option<String>,
    /// Flake input that owns the exported module attribute.
    pub source_input: Option<String>,
    /// Full source revision when available.
    pub source_revision: Option<String>,
    /// Source path relative to the owning input root.
    pub source_path: Option<String>,
    /// Bounded declaration details.
    pub declarations: Vec<FlakeModuleDeclaration>,
    /// Whether `declarations` contains the authoritative complete declaration set.
    #[serde(default)]
    pub declarations_complete: bool,
    /// Managed configurations that consume the module.
    pub consumers: Vec<String>,
    /// Authoritative declaration count.
    pub declaration_count: i64,
    /// Authoritative consumer count.
    pub consumer_count: i64,
    /// Safe module-analysis diagnostic.
    pub error: Option<String>,
}

/// Returns one stable page of declarations for an exported module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeModuleDeclarationsPage {
    /// Selected flake-output snapshot lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Full selected revision SHA.
    pub revision: String,
    /// Exact exported module name.
    pub module_name: String,
    /// Safe lifecycle or integrity diagnostic.
    pub error: Option<String>,
    /// Content digest that binds all continuation pages to one snapshot.
    pub snapshot_token: Option<String>,
    /// Authoritative declaration count.
    pub total: i64,
    /// Applied zero-based offset.
    pub offset: usize,
    /// Applied page limit, at most 100.
    pub limit: usize,
    /// Declarations in deterministic stable order.
    pub declarations: Vec<FlakeModuleDeclaration>,
}

/// One resolved flake lock input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeOutputInput {
    /// Stable lock node identity.
    pub node: String,
    /// Root input names that resolve to this node.
    pub names: Vec<String>,
    /// Whether the root references this node directly.
    pub direct: bool,
    /// Whether this node is reachable only transitively.
    pub transitive: bool,
    /// Raw follows paths for root aliases.
    pub follows: Vec<Value>,
    /// Original lock metadata after server redaction.
    pub original: Value,
    /// Locked metadata after server redaction.
    pub locked: Value,
    /// Lock source type.
    pub source_type: String,
    /// Safe source URL when emitted.
    pub source: Option<String>,
    /// Full locked revision when emitted.
    pub locked_revision: Option<String>,
    /// Source update timestamp.
    pub last_modified: Option<i64>,
    /// Whether the input uses an indirect channel reference.
    pub channel: bool,
    /// Whether the lock source is revision-tracked.
    pub tracked: bool,
    /// Number of immediate children for a direct root input.
    pub direct_descendant_count: Option<i64>,
    /// Number of unique transitive descendants for a direct root input.
    pub transitive_descendant_count: Option<i64>,
}

/// Reports whether exported-module evaluation metadata was available.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeModuleEvaluation {
    /// Whether a nixpkgs library was available for module evaluation.
    pub available: bool,
    /// Library source used for evaluation.
    pub source: Option<String>,
    /// Safe diagnostic when evaluation was unavailable.
    pub error: Option<String>,
}

/// Typed configuration-independent flake output payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeOutputPayload {
    /// Declared `nixosConfigurations` names.
    pub declared_systems: Vec<String>,
    /// Exported module analysis.
    pub exported_modules: Vec<FlakeOutputModule>,
    /// Resolved lock inputs.
    pub inputs: Vec<FlakeOutputInput>,
    /// Authoritative direct input count.
    pub direct_input_count: i64,
    /// Authoritative resolved input count.
    pub resolved_input_count: i64,
    /// Safe lock-read diagnostic.
    pub lock_error: Option<String>,
    /// Exported-module evaluation state.
    pub module_evaluation: FlakeModuleEvaluation,
    /// Distinct full nixpkgs revisions in the lock graph.
    #[serde(rename = "nixpkgsRevisions")]
    pub nixpkgs_revisions: Vec<String>,
    /// Whether multiple nixpkgs revisions are resolved.
    pub multiple_nixpkgs_revisions: bool,
}

/// Returns cached revision-scoped flake outputs and reconciliation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeOutputSnapshotResponse {
    /// Snapshot lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Full selected revision SHA.
    pub revision: String,
    /// Full Git first-parent revision when known.
    pub first_parent_revision: Option<String>,
    /// Whether Git authoritatively resolved parent data.
    pub first_parent_resolved: bool,
    /// Whether the first-parent output snapshot is available.
    pub comparison_available: bool,
    /// Safe failure diagnostic.
    pub error: Option<String>,
    /// Digest that binds continuation pages to the selected snapshot version.
    pub snapshot_token: Option<String>,
    /// Selected-revision output payload.
    pub outputs: Option<FlakeOutputPayload>,
    /// First-parent output payload when available.
    pub previous_outputs: Option<FlakeOutputPayload>,
    /// Typed first-parent delta when comparison is available.
    pub delta: Option<FlakeOutputDelta>,
    /// Authoritative system reconciliation.
    pub systems: Vec<ReconciledFlakeSystem>,
    /// Number of managed systems for the flake.
    pub managed_system_count: i64,
    /// Number of declared configurations at the revision.
    pub declared_system_count: i64,
    /// Number of declared configurations in the usable Git first parent.
    pub previous_declared_system_count: Option<i64>,
    /// Revision-global visible declared-but-unmanaged count.
    pub declared_unmanaged_count: i64,
    /// Revision-global visible managed-but-undeclared count.
    pub managed_undeclared_count: i64,
    /// Revision-global count of visible managed rows sharing an output name.
    #[serde(default)]
    pub output_collapsed_count: i64,
    /// Revision-global count of visible systems pinned away from this revision.
    #[serde(default)]
    pub pinned_revision_count: i64,
    /// Revision-global count of direct inputs older than 90 days.
    #[serde(default)]
    pub stale_direct_input_count: i64,
    /// Number of exported modules at the revision before pagination.
    pub exported_module_count: i64,
    /// Applied collection pagination.
    pub pagination: FlakeOutputPagination,
}

/// Selects one flake-system reconciliation subset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlakeSystemFilter {
    /// Returns every visible reconciliation row.
    #[default]
    All,
    /// Returns declarations without a visible managed system.
    DeclaredUnmanaged,
    /// Returns visible managed systems absent from the revision.
    ManagedUndeclared,
}

impl FlakeSystemFilter {
    /// Returns the server query representation.
    pub const fn as_query_value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::DeclaredUnmanaged => "declared_unmanaged",
            Self::ManagedUndeclared => "managed_undeclared",
        }
    }
}

fn default_sync_status() -> String {
    "unknown".to_string()
}

/// Navigation badge aggregate returned by GET /api/v1/navigation/badges.
/// Polled by the sidebar every 30 seconds. Counts are computed server-side
/// relative to the requesting user's last acknowledgment of each category
/// (persisted, survives refresh/re-login) — not raw totals. See
/// `alerts::acknowledge` for how the frontend records acknowledgment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NavigationBadges {
    /// Server-side `NOW()` captured when this response was computed. Must be
    /// echoed back as `observed_at` in POST /navigation/acknowledge so the
    /// server anchors `last_seen_at` to exactly the data the user saw, not to
    /// the (later) POST receive time.
    #[serde(default)]
    pub observed_at: Option<String>,
    #[serde(default)]
    pub systems_attention: i64,
    #[serde(default)]
    pub systems_total: i64,
    /// MD5 of the sorted alerting system IDs. Echo back in acknowledge body.
    #[serde(default)]
    pub systems_fingerprint: Option<String>,
    #[serde(default)]
    pub flakes_errored: i64,
    #[serde(default)]
    pub flakes_total: i64,
    #[serde(default)]
    pub environments_attention: i64,
    #[serde(default)]
    pub environments_total: i64,
    /// MD5 of the sorted alerting environment IDs. Echo back in acknowledge body.
    #[serde(default)]
    pub environments_fingerprint: Option<String>,
    #[serde(default)]
    pub builds_failed_new: i64,
    #[serde(default)]
    pub evals_failed_new: i64,
    #[serde(default)]
    pub cves_critical_new: i64,
    /// Server canonical occurrence IDs the user can dismiss per category.
    #[serde(default)]
    pub builds_occurrence_ids: Vec<String>,
    #[serde(default)]
    pub evals_occurrence_ids: Vec<String>,
    #[serde(default)]
    pub flakes_occurrence_ids: Vec<String>,
    #[serde(default)]
    pub systems_occurrence_ids: Vec<String>,
    #[serde(default)]
    pub environments_occurrence_ids: Vec<String>,
    #[serde(default)]
    pub cves_occurrence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeCredentialSummary {
    pub flake_id: i32,
    pub auth_type: String,
    pub username: Option<String>,
    pub ssh_username: Option<String>,
    pub has_secret: bool,
}

/// Request payload for creating a flake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateFlakeRequest {
    pub name: String,
    pub repo_url: String,
    pub branch: Option<String>,
    pub build_scope: Option<String>,
}

/// Request payload for updating a flake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateFlakeRequest {
    pub name: String,
    pub repo_url: String,
    pub branch: Option<String>,
    pub build_scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateFlakeCredentialRequest {
    pub auth_type: String,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub ssh_username: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateFlakeCredentialRequest {
    pub auth_type: Option<String>,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub ssh_username: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestFlakeCredentialRequest {
    pub repo_url: Option<String>,
    pub branch: Option<String>,
    pub auth_type: String,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub ssh_username: Option<String>,
    pub use_stored_secret_if_empty: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestFlakeCredentialResponse {
    pub ok: bool,
    pub message: String,
    pub branch: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Environment DTOs — GET /api/v1/environments, GET /api/v1/environments/:id
// ─────────────────────────────────────────────────────────────────────────────

/// Per-environment system health + risk rollup returned by the environments API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EnvironmentRollup {
    pub active_system_count: i64,
    pub healthy: i64,
    pub warning: i64,
    pub critical: i64,
    pub offline: i64,
    pub cve_critical_high: i64,
    pub flakes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentCacheSummary {
    pub name: String,
    pub url: String,
    pub cache_type: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentComplianceSummary {
    pub id: Uuid,
    pub name: String,
    pub framework: String,
}

/// Lightweight environment summary returned by the environments API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentSummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
    pub is_active: bool,
    /// Number of systems assigned to this environment.
    pub system_count: i64,
    #[serde(default)]
    pub rollup: EnvironmentRollup,
    #[serde(default)]
    pub default_policy: Option<String>,
    #[serde(default)]
    pub auto_sync: Option<bool>,
    #[serde(default)]
    pub requires_approval: Option<bool>,
    #[serde(default)]
    pub is_production: Option<bool>,
    #[serde(default)]
    pub role_assignment_count: Option<i64>,
    #[serde(default)]
    pub cache: Option<EnvironmentCacheSummary>,
    #[serde(default)]
    pub compliance_bundle: Option<EnvironmentComplianceSummary>,
    #[serde(default)]
    pub compliance_assignments: Vec<AssignmentResponse>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentWithPolicies {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
    pub is_active: bool,
    pub system_count: i64,
    pub required_policy_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentPolicyMapEntry {
    pub environment_id: Uuid,
    pub required_policy_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceBundleSummary {
    pub id: Uuid,
    pub name: String,
    pub framework: String,
    pub version: String,
    pub description: Option<String>,
    pub layer: String,
    pub owner: String,
    pub last_review: Option<DateTime<Utc>>,
    pub policy_ids: Vec<Uuid>,
    pub required_envs: Vec<ComplianceEnvironmentRef>,
    pub control_count: i64,
    pub environment_count: i64,
    /// Active versioned bundle assignments across environment and system scopes.
    #[serde(default)]
    pub active_assignment_count: i64,
    #[serde(default)]
    pub current_draft_version_id: Option<Uuid>,
    #[serde(default)]
    pub current_published_version_id: Option<Uuid>,
    #[serde(default)]
    pub current_draft_version: Option<String>,
    #[serde(default)]
    pub current_published_version: Option<String>,
    #[serde(default)]
    pub versions: Vec<ComplianceBundleVersionSummary>,
    #[serde(default)]
    pub policy_count: i64,
    #[serde(default)]
    pub requirement_count: i64,
    #[serde(default)]
    pub applicable_system_count: i64,
    #[serde(default)]
    pub aggregate_score: Option<i64>,
}

/// A server-backed arrangement of Security-domain policy controls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceGroupingScheme {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub groups: Vec<ComplianceGroupingSchemeGroup>,
}

/// One named group in a custom grouping scheme. Policy IDs are lineages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceGroupingSchemeGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub pinned_policy_ids: Vec<Uuid>,
    #[serde(default)]
    pub excluded_policy_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceGroupingSchemeRequest {
    pub name: String,
    pub description: Option<String>,
    pub groups: Vec<ComplianceGroupingSchemeGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceBundleVersionSummary {
    pub id: Uuid,
    pub bundle_id: Uuid,
    pub version: String,
    pub publication_state: String,
    #[serde(default)]
    pub trust_state: String,
    pub semantic_digest: String,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub derived_from_version_id: Option<Uuid>,
    pub control_count: i64,
    #[serde(default)]
    pub is_current_published: bool,
    #[serde(default)]
    pub is_current_draft: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceEnvironmentRef {
    pub id: Uuid,
    pub name: String,
    pub color_hex: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceBundleSystemsResponse {
    pub bundle_id: Uuid,
    #[serde(default)]
    pub bundle_version_id: Option<Uuid>,
    pub systems: Vec<ComplianceSystemRollup>,
    pub totals: ComplianceRollupTotals,
}

/// Response for GET /api/v1/systems/:system_id/compliance
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemComplianceBundlesResponse {
    pub system_id: Uuid,
    pub bundles: Vec<SystemComplianceBundle>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemComplianceBundle {
    pub bundle: ComplianceBundleSummary,
    pub rollup: ComplianceSystemRollup,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ComplianceRollupTotals {
    pub system_count: i64,
    pub fully_compliant_count: i64,
    pub pass: i64,
    pub warn: i64,
    pub fail: i64,
    pub waiver: i64,
    pub total_controls: i64,
    #[serde(default)]
    pub evaluated_controls: i64,
    #[serde(default)]
    pub not_checked: i64,
    #[serde(default)]
    pub not_applicable: i64,
    #[serde(default)]
    pub error: i64,
    pub overall_score: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceSystemRollup {
    pub system_id: Uuid,
    pub hostname: String,
    pub environment: Option<String>,
    pub applies: bool,
    pub total: i64,
    #[serde(default)]
    pub evaluated_total: i64,
    pub pass: i64,
    pub warn: i64,
    pub fail: i64,
    pub waiver: i64,
    #[serde(default)]
    pub not_checked: i64,
    #[serde(default)]
    pub not_applicable: i64,
    #[serde(default)]
    pub error: i64,
    #[serde(default)]
    pub report_only: i64,
    pub score: i64,
    #[serde(default)]
    pub resolution_state: Option<String>,
    #[serde(default)]
    pub assignment_status: Option<String>,
    #[serde(default)]
    pub assignment_reason: Option<String>,
    #[serde(default)]
    pub assignment_approved_by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceControlStatus {
    Pass,
    Warn,
    Fail,
    Waiver,
    NotChecked,
    NotApplicable,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceEvidenceResponse {
    pub bundle_id: Uuid,
    #[serde(default)]
    pub bundle_version_id: Option<Uuid>,
    /// Framework from the exact bundle version used to produce this evidence.
    #[serde(default)]
    pub framework: Option<String>,
    pub system_id: Uuid,
    pub hostname: String,
    pub controls: Vec<ComplianceControlEvidence>,
    #[serde(default)]
    pub resolution_state: Option<String>,
}

/// Describes one normalized requirement mapping attached to compliance evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComplianceRequirementIdentity {
    /// Identifies the immutable requirement version.
    pub requirement_version_id: Uuid,
    /// Contains the framework-published requirement or control identifier.
    pub external_id: String,
    /// Contains the optional human-readable requirement title.
    pub title: Option<String>,
    /// Identifies the authoritative framework lineage.
    pub framework_id: Uuid,
    /// Contains the human-readable framework name.
    pub framework_name: String,
    /// Identifies the immutable framework release.
    pub framework_version_id: Uuid,
    /// Contains the human-readable framework release version.
    pub framework_version: String,
    /// Contains the optional human-readable framework release title.
    pub framework_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceControlEvidence {
    pub policy_id: Uuid,
    pub policy_name: String,
    pub status: ComplianceControlStatus,
    pub severity: String,
    pub summary: String,
    pub evidence_items: Vec<ComplianceEvidenceItem>,
    pub framework_mapping: String,
    /// Contains normalized mapping identity when supplied by the server.
    #[serde(default)]
    pub requirements: Vec<ComplianceRequirementIdentity>,
    /// Contains the authoritative composite assessment when one was recorded.
    #[serde(default)]
    pub composite_result: Option<CompositeAssessmentResult>,
    /// Identifies the stable server finding used for remediation actions.
    #[serde(default)]
    pub finding_id: Option<Uuid>,
    /// Binds a legacy finding to the exact authoritative observation.
    #[serde(default)]
    pub finding_observation: Option<FindingObservationReference>,
    /// True when this control is composite even if no exact assessment exists yet.
    #[serde(default)]
    pub composite_expected: bool,
    /// Grouping metadata from the exact policy version used for this control.
    #[serde(default)]
    pub control_family: Option<String>,
    #[serde(default)]
    pub cmmc_level: Option<i32>,
    #[serde(default)]
    pub cis_section: Option<String>,
}

/// Identifies the authoritative evidence source behind a stable finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingObservationSource {
    /// Uses a deployed derivation's persisted Nix policy result.
    NixPolicyResult,
    /// Uses a completed CVE scan for the deployed derivation.
    CveScan,
}

/// References one exact observation that the server can recompute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingObservationReference {
    /// Selects the authoritative evidence resolver.
    pub source: FindingObservationSource,
    /// Identifies the source derivation or scan.
    pub source_id: String,
    /// Identifies the effective immutable policy version.
    pub policy_version_id: Uuid,
    /// Binds the source values and effective policy semantics.
    pub token: String,
}

/// Describes the server-computed result of one composite policy assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeAssessmentResult {
    /// Identifies the persisted assessment when the server stored one.
    #[serde(default)]
    pub assessment_id: Option<Uuid>,
    /// Identifies the evaluation attempt that produced the assessment.
    #[serde(default)]
    pub evaluation_attempt_id: Option<Uuid>,
    /// Identifies the immutable policy semantics used for evaluation.
    pub policy_version_id: Uuid,
    /// Identifies the evaluated derivation when the assessment targeted one.
    #[serde(default)]
    pub target_store_path: Option<String>,
    /// Contains the digest of the effective policy set.
    #[serde(default)]
    pub effective_set_digest: Option<String>,
    /// Contains the digest of the effective policy configuration.
    #[serde(default)]
    pub effective_config_digest: Option<String>,
    /// Contains the server-derived aggregate assessment status.
    pub overall_status: String,
    /// Contains the ordered results for each evaluated policy rule.
    pub rule_results: Vec<CompositeAssessmentRuleResult>,
}

/// Describes one server-evaluated rule in a composite assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeAssessmentRuleResult {
    /// Identifies the rule within its immutable policy version.
    pub rule_id: Uuid,
    /// Contains the rule evaluator kind.
    pub kind: String,
    /// Contains the evaluation phase in which the rule ran.
    pub phase: String,
    /// Contains the server-derived rule status.
    pub status: String,
    /// Contains the human-readable evaluation explanation.
    pub detail: String,
    /// Contains evaluator-specific evidence without changing its JSON shape.
    pub evidence: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceEvidenceItem {
    pub kind: String,
    pub label: String,
    pub body: String,
    pub artifact: Option<ComplianceEvidenceArtifact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceEvidenceArtifact {
    pub artifact_type: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateComplianceBundleRequest {
    pub name: String,
    pub framework: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub layer: Option<String>,
    pub required_envs: Vec<Uuid>,
    pub policy_ids: Vec<Uuid>,
    #[serde(default)]
    pub requirement_version_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateComplianceBundleRequest {
    pub name: String,
    pub framework: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub required_envs: Vec<Uuid>,
    pub policy_ids: Vec<Uuid>,
    #[serde(default)]
    pub requirement_version_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateEnvironmentRequest {
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
    pub is_active: bool,
    #[serde(default)]
    pub default_policy: Option<String>,
    #[serde(default)]
    pub auto_sync: Option<bool>,
    #[serde(default)]
    pub requires_approval: Option<bool>,
    #[serde(default)]
    pub is_production: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateEnvironmentRequest {
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
    #[serde(default)]
    pub default_policy: Option<String>,
    #[serde(default)]
    pub auto_sync: Option<bool>,
    #[serde(default)]
    pub requires_approval: Option<bool>,
    #[serde(default)]
    pub is_production: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEnvironmentPoliciesRequest {
    pub required_policy_ids: Vec<uuid::Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPolicySummary {
    pub id: Uuid,
    #[serde(default)]
    pub version_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    /// Policy category from current version's compliance_metadata
    #[serde(default)]
    pub category: Option<String>,
    /// Framework from current version's compliance_metadata
    #[serde(default)]
    pub framework: Option<String>,
    /// Severity from current version's compliance_metadata
    #[serde(default)]
    pub severity: Option<String>,
    /// NIST 800-53 control family
    #[serde(default)]
    pub control_family: Option<String>,
    /// CMMC 2.0 level
    #[serde(default)]
    pub cmmc_level: Option<i32>,
    /// CIS Benchmark section
    #[serde(default)]
    pub cis_section: Option<String>,
    /// Human-readable rationale
    #[serde(default)]
    pub rationale: Option<String>,
}

/// Search metadata for one NixOS option. The server derives these fields from
/// the pinned NixOS module option set; `value_type` controls the policy value
/// editor while unknown types deliberately fall back to a semantic string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NixosOptionValueType {
    /// Uses a boolean editor and JSON boolean value.
    Boolean,
    /// Uses the server-supplied set of allowed values.
    Enum,
    /// Uses a signed integer editor and JSON number value.
    Integer,
    /// Uses a single-line semantic string value.
    String,
    /// Uses a multiline semantic string value.
    Lines,
    /// Preserves an unrecognized type as a semantic string.
    Unknown,
}

impl NixosOptionValueType {
    /// Returns the stable editor type name used by policy authoring state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Enum => "enum",
            Self::Integer => "integer",
            Self::String => "string",
            Self::Lines => "lines",
            Self::Unknown => "unknown",
        }
    }
}

/// Describes one option from the server's pinned NixOS metadata index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NixosOptionMetadata {
    /// Contains the canonical NixOS option path.
    pub path: String,
    /// Selects the value editor without overriding target evaluation semantics.
    pub value_type: NixosOptionValueType,
    /// Contains allowed enum values when the option has a closed value set.
    #[serde(default)]
    pub enum_values: Vec<serde_json::Value>,
    /// Contains the option documentation supplied by the metadata index.
    #[serde(default)]
    pub description: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Deployment Policies CRUD DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Full deployment policy record with timestamps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPolicyRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Exact draft version shown by the policy-management API, when available.
    #[serde(default)]
    pub current_version_id: Option<Uuid>,
    #[serde(default)]
    pub versions: Vec<DeploymentPolicyVersionSummary>,
    /// Number of trusted/eligible policy_requirement_mappings for this policy version
    #[serde(default)]
    pub mapped_requirement_count: i64,
    /// Number of distinct bundle lineages using this policy version
    #[serde(default)]
    pub bundle_usage_count: i64,
}

/// Evidence collection specification for a control or policy.
/// Describes the authoritative evidence needed to satisfy an ATO requirement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "details")]
pub enum EvidenceKind {
    /// Command execution proof: cmd output must match expect pattern
    Command { cmd: String, expect: String },
    /// System journal or event log: unit/source with match_text filter
    Log {
        source: String,
        unit: String,
        match_text: String,
    },
    /// File presence/state: path with optional annotation
    File { path: String, note: Option<String> },
    /// systemd/systemctl unit state: requires exact state value
    UnitState { unit: String, state: String },
    /// NixOS eval attribute: attr path to be evaluated
    EvalAttr { attr: String },
    /// Human attestation: reviewer assertion with optional note
    Attestation { note: String },
}

/// Single versioned evidence spec within a policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceSpec {
    /// Spec type and parameters
    #[serde(flatten)]
    pub kind: EvidenceKind,
    /// Optional required-fields map (validation dictionary)
    #[serde(default)]
    pub required_fields: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPolicyVersionSummary {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub version: String,
    pub publication_state: String,
    #[serde(default)]
    pub trust_state: String,
    pub semantic_digest: String,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub derived_from_version_id: Option<Uuid>,
    #[serde(default)]
    pub is_current_published: bool,
    #[serde(default)]
    pub is_current_draft: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub policy_type: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub enabled: bool,
    /// SRG IDs for this exact revision (from compliance_metadata).
    #[serde(default)]
    pub srg_ids: Vec<String>,
    /// CCI IDs for this exact revision (from compliance_metadata).
    #[serde(default)]
    pub cci_ids: Vec<String>,
    /// Policy category: "deployment", "pipeline", "rollout", "security"
    #[serde(default)]
    pub category: Option<String>,
    /// Framework string, e.g. "DISA STIG", "NIST 800-53", "CMMC 2.0", "CIS Benchmark", or custom
    #[serde(default)]
    pub framework: Option<String>,
    /// Severity: "hard", "medium", "low" — None means unrated
    #[serde(default)]
    pub severity: Option<String>,
    /// NIST 800-53 control family, e.g. "AC", "AU", "CM", "IA", "SC", "SI", "MP"
    #[serde(default)]
    pub control_family: Option<String>,
    /// CMMC 2.0 maturity level: 1, 2, or 3
    #[serde(default)]
    pub cmmc_level: Option<i32>,
    /// CIS Benchmark section, e.g. "5.2.3"
    #[serde(default)]
    pub cis_section: Option<String>,
    /// Human-readable rationale for this control
    #[serde(default)]
    pub rationale: Option<String>,
    /// User UUID who created this version (if available)
    #[serde(default)]
    pub created_by: Option<Uuid>,
    /// Human-readable display name of the user who created this version (username or email)
    #[serde(default)]
    pub created_by_display: Option<String>,
    /// Evidence collection specifications for ATO audits
    #[serde(default)]
    pub evidence_specs: Vec<EvidenceSpec>,
    /// Authoritative imported-origin provenance recorded at import time.
    /// Empty for policies authored in Crystal Forge. Read-only: the editor
    /// renders it and never submits it.
    #[serde(default)]
    pub provenance: Vec<PolicyOriginProvenance>,
}

/// One authoritative imported-origin record for a policy version, mirroring the
/// server DTO. Populated from the immutable source artifact and its recorded
/// source-object mapping, including origins inherited from the version a draft
/// was derived from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyOriginProvenance {
    /// Identifies the immutable imported source artifact.
    pub source_artifact_id: Uuid,
    /// Contains the original source filename.
    pub filename: String,
    /// Contains the source artifact media type.
    pub media_type: String,
    /// Contains the artifact's lowercase SHA-256 digest.
    pub sha256: String,
    /// Contains the importer version that parsed the source.
    pub parser_version: String,
    /// Contains the detected XCCDF version when the artifact declared one.
    #[serde(default)]
    pub detected_xccdf_version: Option<String>,
    /// Contains the imported source object kind when available.
    #[serde(default)]
    pub object_kind: Option<String>,
    /// Contains the source object's stable external identity when available.
    #[serde(default)]
    pub source_identity: Option<String>,
    /// Describes the recorded import fidelity when available.
    #[serde(default)]
    pub fidelity: Option<String>,
    /// Identifies the user who imported the source when retained by the server.
    #[serde(default)]
    pub imported_by: Option<Uuid>,
    /// Contains the server-resolved importer display name when available.
    #[serde(default)]
    pub imported_by_display: Option<String>,
    /// Records when the source artifact was imported.
    pub imported_at: DateTime<Utc>,
    /// Identifies the first policy version created from this source object.
    pub origin_policy_version_id: Uuid,
    /// Counts derivation edges from the original imported policy version.
    #[serde(default)]
    pub lineage_depth: i32,
    /// Indicates that the current version inherited rather than created this origin.
    #[serde(default)]
    pub inherited: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleVersionPolicyMembership {
    pub policy_version_id: Uuid,
    pub policy_lineage_id: Uuid,
    pub policy_order: i32,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyVersionUsageResponse {
    pub policy_version_id: Uuid,
    pub bundle_versions: Vec<PolicyVersionBundleUsage>,
    pub systems: Vec<PolicyVersionSystemUsage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyVersionBundleUsage {
    pub bundle_id: Uuid,
    pub bundle_name: String,
    pub bundle_version_id: Uuid,
    pub bundle_version: String,
    pub publication_state: String,
    pub policy_order: i32,
    pub is_current_published: bool,
    pub is_current_draft: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyVersionSystemUsage {
    pub system_id: Uuid,
    pub hostname: String,
    pub environment: Option<String>,
    pub bundle_id: Uuid,
    pub bundle_name: String,
    pub bundle_version_id: Uuid,
    pub bundle_version: String,
    pub source: String,
    pub enforcement_mode: String,
}

/// Response for listing deployment policies with pagination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPoliciesListResponse {
    pub policies: Vec<DeploymentPolicyRecord>,
    pub total: usize,
    pub limit: i64,
    pub offset: i64,
    /// Per-policy count of distinct active systems inheriting the policy.
    #[serde(default)]
    pub system_counts: HashMap<Uuid, i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyInterchangePreviewPolicy {
    pub lineage_id: Uuid,
    pub version_id: Uuid,
    pub version: String,
    pub name: String,
    pub policy_type: String,
    pub implementation_state: String,
    pub semantic_digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyInterchangePreviewResponse {
    pub source_sha256: String,
    pub filename: Option<String>,
    pub policy_count: usize,
    pub policies: Vec<PolicyInterchangePreviewPolicy>,
    pub publication_state: String,
    pub enabled: bool,
    pub trusted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyInterchangeImportResponse {
    pub created_policy_count: u32,
    pub reused_policy_count: u32,
    pub publication_state: String,
    pub enabled: bool,
    pub trusted: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// XCCDF Preview / Import DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// A single diagnostic (error or warning) from the XCCDF parser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XccdfDiagnostic {
    pub code: String,
    pub summary: String,
    pub blocking: bool,
}

/// Parsed benchmark identity returned by the XCCDF preview endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XccdfBenchmarkInfo {
    pub id: String,
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub version: Option<String>,
    pub status: Option<String>,
    pub platforms: Vec<String>,
}

/// Parsed profile returned by the XCCDF preview endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XccdfProfileInfo {
    pub id: String,
    pub title: Option<String>,
    pub rule_count: usize,
    #[serde(default)]
    pub rule_ids: Vec<String>,
}

/// Parsed rule summary returned by the XCCDF preview endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XccdfRuleInfo {
    pub id: String,
    pub title: Option<String>,
    /// Cleaned VulnDiscussion text (XML sub-element tags stripped by the server).
    #[serde(default)]
    pub description: Option<String>,
    pub severity: Option<String>,
    pub is_native: bool,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub identifiers: Vec<serde_json::Value>,
    #[serde(default)]
    pub checks: Vec<serde_json::Value>,
    /// Fix/remediation.  Contains `"content"` (full text) and `"preview"` (same
    /// as content, retained for backward compatibility), plus fix metadata.
    #[serde(default)]
    pub fix: Option<serde_json::Value>,
    #[serde(default)]
    pub inferred_assertions: Vec<serde_json::Value>,
    #[serde(default)]
    pub references: Vec<serde_json::Value>,
    #[serde(default)]
    pub has_opaque_xml: bool,
}

/// Response body from `POST /api/v1/compliance/xccdf/preview`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XccdfPreviewResponse {
    pub sha256: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub document_class: Option<String>,
    #[serde(default)]
    pub fidelity: Option<String>,
    #[serde(default)]
    pub fidelity_losses: Vec<String>,
    #[serde(default)]
    pub xccdf_version: Option<String>,
    pub benchmark: Option<XccdfBenchmarkInfo>,
    #[serde(default)]
    pub profiles: Vec<XccdfProfileInfo>,
    #[serde(default)]
    pub rules: Vec<XccdfRuleInfo>,
    pub rule_count: usize,
    pub profile_count: usize,
    #[serde(default)]
    pub errors: Vec<XccdfDiagnostic>,
    #[serde(default)]
    pub warnings: Vec<XccdfDiagnostic>,
    /// CF-native reconciliation data (present only for CfNativeExact documents)
    #[serde(default)]
    pub cf_native_reconciliation: Option<CfNativeReconciliationPreview>,
    /// Requirement-aware reconciliation for a foreign DISA STIG.  This is
    /// server-computed from normalized framework/requirement/mapping data.
    #[serde(default)]
    pub foreign_stig_reconciliation: Option<ForeignStigReconciliationPreview>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignStigReconciliationPreview {
    pub framework: ForeignStigFrameworkReconciliation,
    #[serde(default)]
    pub requirements: Vec<ForeignStigRequirementReconciliation>,
    #[serde(default)]
    pub shared_implementation_groups: Vec<ForeignStigSharedImplementationGroup>,
    /// Requirements present in the previous release but absent from the upload.
    #[serde(default)]
    pub removed_requirements: Vec<ForeignStigRemovedRequirement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignStigSharedImplementationGroup {
    pub group_id: String,
    #[serde(default)]
    pub requirement_keys: Vec<String>,
    pub recommended_action: String,
    pub has_existing_candidate: bool,
    #[serde(default)]
    pub existing_candidate: Option<ForeignStigSharedCandidate>,
    #[serde(default)]
    pub member_proofs: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignStigSharedCandidate {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    pub policy_name: String,
    pub confidence: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignStigRemovedRequirement {
    pub external_id: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignStigFrameworkReconciliation {
    pub canonical_source_key: String,
    pub canonical_release_key: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignStigRequirementReconciliation {
    pub rule_id: String,
    pub external_id: String,
    pub title: Option<String>,
    pub state: String,
    pub auto_resolvable: bool,
    pub inferred_enforcement: bool,
    #[serde(default)]
    pub candidates: Vec<ForeignStigPolicyCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignStigPolicyCandidate {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    pub policy_name: String,
    pub match_type: String,
    pub confidence: u8,
    #[serde(default)]
    pub match_reasons: Vec<String>,
    #[serde(default)]
    pub related_evidence: Option<ForeignStigRelatedEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignStigRelatedEvidence {
    #[serde(default)]
    pub shared_cci_ids: Vec<String>,
    #[serde(default)]
    pub shared_srg_ids: Vec<String>,
    pub related_requirement_version_id: Uuid,
    pub related_framework_id: Uuid,
    pub related_framework_name: String,
    pub related_external_id: String,
}

/// CF-native reconciliation preview for import
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CfNativeReconciliationPreview {
    pub bundle: CfNativeBundleReconciliation,
    #[serde(default)]
    pub policies: Vec<CfNativePolicyReconciliation>,
    pub has_blocking_conflicts: bool,
    #[serde(default)]
    pub blocking_conflicts: Vec<CfNativeConflict>,
    pub signature_status: String,
    pub import_trust_state: String,
}

/// Bundle reconciliation state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CfNativeBundleReconciliation {
    pub lineage_id: String,
    pub version_id: String,
    pub name: String,
    pub version: String,
    pub semantic_digest: String,
    pub source_publication_state: String,
    pub reconciliation_state: String,
    #[serde(default)]
    pub local_lineage_id: Option<String>,
    #[serde(default)]
    pub local_version_id: Option<String>,
    #[serde(default)]
    pub local_semantic_digest: Option<String>,
    #[serde(default)]
    pub local_publication_state: Option<String>,
    #[serde(default)]
    pub local_trust_state: Option<String>,
    pub name_collision: bool,
    #[serde(default)]
    pub blocking_conflicts: Vec<CfNativeConflict>,
}

/// Individual policy reconciliation state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CfNativePolicyReconciliation {
    pub lineage_id: String,
    pub version_id: String,
    pub name: String,
    pub version: String,
    pub policy_type: String,
    pub implementation_state: String,
    pub semantic_digest: String,
    pub enabled_by_default: bool,
    pub reconciliation_state: String,
    #[serde(default)]
    pub local_lineage_id: Option<String>,
    #[serde(default)]
    pub local_version_id: Option<String>,
    #[serde(default)]
    pub local_semantic_digest: Option<String>,
    #[serde(default)]
    pub local_publication_state: Option<String>,
    #[serde(default)]
    pub local_trust_state: Option<String>,
    #[serde(default)]
    pub local_enabled: Option<bool>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub has_opaque_content: bool,
    pub name_collision: bool,
    #[serde(default)]
    pub blocking_conflicts: Vec<CfNativeConflict>,
}

/// Individual conflict during reconciliation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CfNativeConflict {
    pub code: String,
    pub summary: String,
    pub blocking: bool,
    #[serde(default)]
    pub details: serde_json::Value,
}

/// Single rule action in an XCCDF import plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum XccdfRuleImportAction {
    CreateNativeCustom {
        rule_id: String,
        customization: ImportedPolicyCustomization,
        custom_check: ImportedCustomCheck,
        evidence_requirements: Vec<ImportedEvidenceRequirement>,
    },
    CreateManual {
        rule_id: String,
        #[serde(default)]
        customization: ImportedPolicyCustomization,
        #[serde(default)]
        evidence_requirements: Vec<ImportedEvidenceRequirement>,
    },
    CreateUnbound {
        rule_id: String,
        #[serde(default)]
        customization: ImportedPolicyCustomization,
    },
    PreserveOpaque {
        rule_id: String,
        #[serde(default)]
        customization: ImportedPolicyCustomization,
    },
    MapExisting {
        rule_id: String,
        policy_version_id: Uuid,
        #[serde(default)]
        proof: Option<MapExistingProof>,
    },
    Exclude {
        rule_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapExistingProof {
    InheritedMapping,
    ExactTechnicalMatch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ImportedPolicyCustomization {
    pub policy_name: Option<String>,
    pub policy_description: Option<String>,
    pub implementation_note: Option<String>,
    #[serde(default)]
    pub policy_severity: Option<String>,
    #[serde(default)]
    pub policy_rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedCustomCheck {
    #[serde(default)]
    pub mode: String,
    pub rules: Vec<ImportedCustomCheckRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedCustomCheckRule {
    pub field_name: String,
    pub expression: String,
    pub description: String,
    #[serde(default = "default_true")]
    pub strict: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportedEvidenceRequirement {
    Command {
        command: String,
        expected_output: String,
    },
    File {
        path: String,
        expected_content: String,
    },
    UnitState {
        unit: String,
        state: String,
    },
    Log {
        source: String,
        unit: Option<String>,
        pattern: String,
    },
    Attestation {
        description: String,
    },
}

/// Bundle metadata included in the XCCDF import plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedBundlePlan {
    pub name: String,
    pub framework: String,
    pub version: String,
    pub layer: Option<String>,
    pub owner: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub environment_ids: Vec<Uuid>,
}

/// Import plan submitted to `POST /api/v1/compliance/xccdf/import`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XccdfImportPlan {
    pub expected_sha256: String,
    pub selected_profile_id: Option<String>,
    pub selected_rule_ids: Vec<String>,
    pub rule_actions: Vec<XccdfRuleImportAction>,
    #[serde(default)]
    pub mapping_semantics: std::collections::HashMap<String, ImportedMappingSemantics>,
    pub bundle: ImportedBundlePlan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ImportedMappingSemantics {
    pub relationship: Option<String>,
    pub coverage: Option<String>,
    pub rationale: Option<String>,
    #[serde(default)]
    pub reviewed_related_candidate: Option<ReviewedRelatedCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewedRelatedCandidate {
    pub policy_version_id: Uuid,
    pub related_requirement_version_id: Uuid,
    #[serde(default)]
    pub shared_cci_ids: Vec<String>,
    #[serde(default)]
    pub shared_srg_ids: Vec<String>,
}

#[cfg(test)]
mod xccdf_mapping_contract_tests {
    use super::*;

    #[test]
    fn map_existing_proofs_use_server_wire_names() {
        let action = XccdfRuleImportAction::MapExisting {
            rule_id: "rule-1".into(),
            policy_version_id: Uuid::nil(),
            proof: Some(MapExistingProof::ExactTechnicalMatch),
        };
        let value = serde_json::to_value(action).expect("serialize map action");
        assert_eq!(value["proof"], "exact_technical_match");
    }

    #[test]
    fn reviewed_related_candidate_preserves_evidence() {
        let semantics = ImportedMappingSemantics {
            relationship: Some("supports".into()),
            coverage: Some("partial".into()),
            rationale: Some("reviewed shared CCI".into()),
            reviewed_related_candidate: Some(ReviewedRelatedCandidate {
                policy_version_id: Uuid::nil(),
                related_requirement_version_id: Uuid::from_u128(1),
                shared_cci_ids: vec!["CCI-000770".into()],
                shared_srg_ids: vec!["SRG-OS-000109-GPOS-00051".into()],
            }),
        };
        let value = serde_json::to_value(semantics).expect("serialize reviewed semantics");
        assert_eq!(
            value["reviewed_related_candidate"]["shared_cci_ids"][0],
            "CCI-000770"
        );
        assert_eq!(value["coverage"], "partial");
    }
}

/// Response body from `POST /api/v1/compliance/xccdf/import`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XccdfImportResponse {
    #[serde(default)]
    pub bundle_version_id: Option<Uuid>,
    pub created_policy_count: u32,
    /// Server field name for reused exact versions
    /// (`XccdfCommittedImportResult.reused_policy_versions`).
    #[serde(default)]
    pub reused_policy_versions: u32,
    #[serde(default)]
    pub errors: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Trust / Publication DTOs
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustPolicyVersionRequest {
    pub trusted: bool,
    pub review_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustPolicyVersionResponse {
    pub version_id: Uuid,
    pub publication_state: String,
    pub trust_state: String,
    pub trusted_by: Option<Uuid>,
    pub trusted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustBundleVersionRequest {
    pub trusted: bool,
    pub review_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustBundleVersionResponse {
    pub version_id: Uuid,
    pub publication_state: String,
    pub trust_state: String,
    pub trusted_by: Option<Uuid>,
    pub trusted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishBundleVersionRequest {
    pub auto_publish_draft_policies: Option<bool>,
    pub expected_semantic_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishBundleVersionResponse {
    pub version_id: Uuid,
    pub publication_state: String,
    pub published_at: DateTime<Utc>,
    pub semantic_digest: String,
    pub published_policy_count: i32,
    pub auto_published_policy_count: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateBundleDraftRequest {
    pub new_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateBundleDraftResponse {
    pub version_id: Uuid,
    pub version: String,
    pub publication_state: String,
    pub derived_from_version_id: Uuid,
}

// ─────────────────────────────────────────────────────────────────────────────
// Assignment DTOs
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyValueOverride {
    pub policy_version_id: Uuid,
    pub value_path: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateAssignmentRequest {
    pub bundle_version_id: Uuid,
    /// "environment" or "system"
    pub scope_type: String,
    pub scope_id: Uuid,
    pub enforcement_mode: Option<String>,
    pub exclusions: Option<Vec<Uuid>>,
    pub additions: Option<Vec<Uuid>>,
    pub value_overrides: Option<Vec<PolicyValueOverride>>,
    /// User-provided reason/justification for the assignment.
    pub reason: Option<String>,
}

/// Request to replace an assignment's mutable overlay and enforcement mode.
///
/// The server creates a new immutable assignment version and compares
/// `expected_version_id` with the current version before doing so. Bundle
/// version rebinding is deliberately not part of this request.
///
/// Reason updates use tri-state semantics:
/// - omitted: preserve reason from current immutable version
/// - null: explicitly clear the reason
/// - value: replace reason
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateAssignmentRequest {
    pub expected_version_id: Uuid,
    pub enforcement_mode: Option<String>,
    pub exclusions: Option<Vec<Uuid>>,
    pub additions: Option<Vec<Uuid>>,
    pub value_overrides: Option<Vec<PolicyValueOverride>>,
    /// Tri-state reason/justification for the assignment update.
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unset")]
    pub reason: FieldUpdate<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentResponse {
    pub id: Uuid,
    pub current_version_id: Uuid,
    pub bundle_id: Uuid,
    pub bundle_version_id: Uuid,
    pub scope_type: String,
    pub scope_id: Uuid,
    pub enforcement_mode: String,
    #[serde(default)]
    pub exclusions: Vec<Uuid>,
    #[serde(default)]
    pub additions: Vec<Uuid>,
    #[serde(default)]
    pub value_overrides: Vec<PolicyValueOverride>,
    pub assignment_overlay_digest: String,
    #[serde(default = "default_assignment_active")]
    pub active: bool,
    /// Reason/justification from the current immutable assignment version.
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_assignment_active() -> bool {
    true
}

/// Server returns `{ "assignments": [...] }` — use this to deserialize then
/// extract the inner Vec.
/// A retained record that prevents permanent deletion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeletionBlocker {
    /// Machine-readable blocker kind, e.g. "immutable_history", "draft_bundle_membership".
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
    /// Number of references, when applicable.
    #[serde(default)]
    pub count: Option<i64>,
    /// Specific immutable version UUIDs that are blocking.
    #[serde(default)]
    pub version_ids: Vec<Uuid>,
    /// Classification supplied by the server. A false value means normal
    /// lifecycle actions cannot remove the blocker.
    #[serde(default)]
    pub removable: bool,
}

/// Whether a policy or bundle lineage can be permanently deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeletionEligibility {
    pub eligible: bool,
    #[serde(default)]
    pub blockers: Vec<DeletionBlocker>,
}

impl DeletionEligibility {
    pub fn permanently_blocked(&self) -> bool {
        self.blockers.iter().any(|blocker| !blocker.removable)
    }
}

/// Request body for bulk-deleting policies from the catalog's multi-select
/// toolbar (TASK-433 Phase 1 — policy catalog scaling).
#[derive(Debug, Clone, Serialize)]
pub struct BulkDeletePoliciesRequest {
    /// Identifies the policy lineages to delete independently.
    pub policy_ids: Vec<Uuid>,
}

/// One policy that a bulk-delete request could not remove, with the same
/// authoritative eligibility payload the single-policy delete endpoint
/// returns on conflict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BulkDeleteSkippedPolicy {
    /// Identifies the requested policy lineage that was not deleted.
    pub policy_id: Uuid,
    /// Stable machine-readable reason: "not_found" or "deletion_blocked".
    pub reason: String,
    /// Contains authoritative deletion blockers for a blocked policy.
    #[serde(default)]
    pub eligibility: Option<DeletionEligibility>,
}

/// Response for a bulk-delete request. Every requested id resolves into
/// exactly one of `deleted` or `skipped`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BulkDeletePoliciesResponse {
    /// Contains policy lineage IDs that the server deleted.
    pub deleted: Vec<Uuid>,
    /// Contains requested policies the server did not delete and the reason.
    #[serde(default)]
    pub skipped: Vec<BulkDeleteSkippedPolicy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentListResponse {
    pub assignments: Vec<AssignmentResponse>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectivePolicyDto {
    pub policy_version_id: Uuid,
    pub policy_lineage_id: Uuid,
    pub policy_type: String,
    pub source: String,
    pub baseline_order: Option<i32>,
    pub addition_order: Option<i32>,
    #[serde(default)]
    pub overrides: Vec<PolicyValueOverride>,
    pub effective_config: serde_json::Value,
    pub enforcement_mode: String,
    #[serde(default)]
    pub provenance: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectivePolicySetResponse {
    pub bundle_version_id: Uuid,
    pub assignment_id: Option<Uuid>,
    pub scope_type: String,
    pub scope_id: Uuid,
    pub policies: Vec<EffectivePolicyDto>,
    pub effective_set_digest: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Request to create a new deployment policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateDeploymentPolicyRequest {
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub config: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// SRG IDs this policy satisfies. Normalised server-side.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub srg_ids: Vec<String>,
    /// CCI mappings. Normalised server-side.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cci_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmmc_level: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cis_section: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Evidence collection specifications for ATO audits
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_specs: Vec<EvidenceSpec>,
    /// Creates normalized requirement mappings with the new policy version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirement_mappings: Vec<CreatePolicyMappingRequest>,
}

/// Request to update an existing deployment policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateDeploymentPolicyRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// When `Some`, replace the curated SRG mapping; `Some([])` clears it.
    /// `None` (omitted) preserves the existing value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub srg_ids: Option<Vec<String>>,
    /// When `Some`, replace the curated CCI mapping; `Some([])` clears it.
    /// `None` (omitted) preserves the existing value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cci_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Replaces, clears, or preserves the framework for `Some(Some(_))`,
    /// `Some(None)`, or `None`, respectively.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<Option<String>>,
    /// Replaces, clears, or preserves the severity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<Option<String>>,
    /// Replaces, clears, or preserves the control family.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_family: Option<Option<String>>,
    /// Replaces, clears, or preserves the CMMC level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmmc_level: Option<Option<i32>>,
    /// Replaces, clears, or preserves the CIS section.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cis_section: Option<Option<String>>,
    /// Replaces, clears, or preserves the rationale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<Option<String>>,
    /// When `Some`, replace evidence specs; `Some([])` clears them.
    /// `None` (omitted) preserves the existing value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_specs: Option<Vec<EvidenceSpec>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Flake Commit Timeline DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// A flake with its commit timeline for the dashboard widget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeTimeline {
    pub flake_id: i32,
    pub flake_name: String,
    pub repo_url: String,
    pub commits: Vec<FlakeCommit>,
}

/// Build status for a commit/derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildStatus {
    /// No build in progress or queued.
    Idle,
    /// Build is queued and waiting for a worker.
    Queued,
    /// Build is currently in progress.
    Building,
    /// Build is being cancelled (waiting for builder to stop).
    Cancelling,
    /// Build completed successfully.
    Complete,
    /// Build failed.
    Failed,
    /// Build was cancelled by user.
    Cancelled,
}

impl BuildStatus {
    /// Returns true if this status represents an active build (queued or building).
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Queued | Self::Building | Self::Cancelling)
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Queued => "Queued",
            Self::Building => "Building",
            Self::Cancelling => "Stopping",
            Self::Complete => "Complete",
            Self::Cancelled => "Cancelled",
            Self::Failed => "Failed",
        }
    }

    /// CSS color class for the status.
    pub fn color_class(&self) -> &'static str {
        match self {
            Self::Idle => "text-gray-400",
            Self::Queued => "text-blue-400",
            Self::Building => "text-cyan-400",
            Self::Cancelling => "text-amber-400",
            Self::Complete => "text-emerald-400",
            Self::Cancelled => "text-slate-400",
            Self::Failed => "text-red-400",
        }
    }
}

/// Response containing the git diff for a specific commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDiffResponse {
    pub commit_hash: String,
    pub diff: String,
}

/// A single commit in a flake's history with deployment info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FlakeCommit {
    /// Database commit ID.
    pub id: i32,
    /// Full commit hash.
    pub hash: String,
    /// Short commit message (first line).
    pub message: String,
    /// Commit author name.
    pub author: String,
    /// When the commit was made.
    pub committed_at: DateTime<Utc>,
    /// Number of systems currently deployed at this commit.
    pub system_count: i64,
    /// How many commits behind the latest this is (0 = latest).
    pub commits_behind: i64,
    /// nixosConfigurations discovered at this commit.
    ///
    /// Entries may include the suffix " [CF system]" when the configuration
    /// name matches a Crystal Forge system deployed at this commit.
    pub systems: Vec<String>,
    /// Per-configuration path details for commit details UI.
    #[serde(default)]
    pub system_paths: Vec<FlakeCommitSystemPath>,
    /// Current build status for this commit (if any build is in progress).
    #[serde(default)]
    pub build_status: Option<BuildStatus>,
    /// Dry-run/evaluation status for this commit.
    #[serde(default)]
    pub evaluation_status: Option<String>,
    /// Error message if evaluation failed.
    #[serde(default)]
    pub evaluation_error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeCommitSystemPath {
    pub config_name: String,
    #[serde(default)]
    pub is_cf_system: bool,
    #[serde(default)]
    pub cf_hostname: Option<String>,
    #[serde(default)]
    pub mapped_host_count: i64,
    #[serde(default)]
    pub expected_store_path: Option<String>,
    #[serde(default)]
    pub current_store_path: Option<String>,
    #[serde(default)]
    pub cve_scan_eligible: bool,
    #[serde(default)]
    pub cve_scan_blocked_reason: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Build Queue DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Query parameters for the paginated build queue endpoint.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BuildQueueParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    /// Comma-separated statuses: queued, building, success, failed
    pub status: Option<String>,
    pub commit_hash: Option<String>,
    pub flake_name: Option<String>,
    pub config_name: Option<String>,
    pub queued_after: Option<DateTime<Utc>>,
    pub queued_before: Option<DateTime<Utc>>,
    pub search: Option<String>,
    pub latest_only: bool,
}

/// Paginated response for the build queue listing endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildQueuePageResponse {
    pub total: i64,
    #[serde(default)]
    pub domain_total: i64,
    pub page: i64,
    pub limit: i64,
    pub items: Vec<BuildQueueItem>,
}

/// Summary of the build queue for the dashboard widget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildQueueSummary {
    /// Number of builds currently in progress.
    pub building_count: i64,
    /// Number of builds waiting in the queue.
    pub queued_count: i64,
    #[serde(default)]
    pub failed_24h_count: i64,
    #[serde(default)]
    pub active_workers: i64,
    #[serde(default)]
    pub total_workers: i64,
    #[serde(default)]
    pub used_slots: i64,
    #[serde(default)]
    pub total_slots: i64,
    /// List of active build items (building + queued, limited).
    pub items: Vec<BuildQueueItem>,
    /// Server timestamp for freshness.
    pub timestamp: DateTime<Utc>,
}

/// A single item in the build queue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildQueueItem {
    #[serde(default)]
    pub job_id: Option<Uuid>,
    #[serde(default)]
    pub system_id: Option<Uuid>,
    #[serde(default)]
    pub flake_id: Option<i32>,
    #[serde(default)]
    pub is_latest_per_flake: bool,
    /// The hostname/system being built.
    pub hostname: String,
    /// The flake name this build belongs to.
    pub flake_name: String,
    /// Short commit hash being built.
    pub commit_hash: String,
    /// Commit message (first line).
    pub commit_message: Option<String>,
    /// Current status (Queued or Building).
    pub status: BuildStatus,
    #[serde(default)]
    pub builder_name: Option<String>,
    /// 1-indexed attempt number within this job's retry lineage.
    #[serde(default = "default_attempt_number")]
    pub attempt_number: i32,
    /// When the build was queued.
    pub queued_at: DateTime<Utc>,
    /// When the build started (None if still queued).
    pub started_at: Option<DateTime<Utc>>,
    /// Elapsed time in seconds since started (for display).
    #[serde(default)]
    pub elapsed_secs: Option<i64>,
    /// Build logs (if available).
    #[serde(default)]
    pub logs: Option<String>,
    /// Environment name (if system has an environment).
    #[serde(default)]
    pub environment: Option<String>,
    /// Total derivations for this system config at this commit.
    #[serde(default)]
    pub total_derivs: i64,
    /// Derivations that have completed a build.
    #[serde(default)]
    pub built_derivs: i64,
    /// Derivations pushed to cache.
    #[serde(default)]
    pub cached_derivs: i64,
}

fn default_attempt_number() -> i32 {
    1
}

/// Summary of the evaluation queue for the evaluations page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalQueueSummary {
    pub active_count: i64,
    pub completed_count: i64,
    #[serde(default)]
    pub successful_count: i64,
    pub failed_count: i64,
    #[serde(default)]
    pub domain_total: i64,
    #[serde(default)]
    pub filtered_total: i64,
    pub execution_mode: String,
    pub items: Vec<EvalQueueItem>,
    pub timestamp: DateTime<Utc>,
}

/// A single commit item in the evaluation queue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalQueueItem {
    pub commit_id: i32,
    pub flake_id: i32,
    pub flake_name: String,
    pub branch: String,
    pub commit_hash: String,
    pub commit_message: Option<String>,
    pub author: Option<String>,
    pub committed_at: DateTime<Utc>,
    pub enqueued_at: DateTime<Utc>,
    #[serde(default)]
    pub is_latest_per_flake: bool,
    pub evaluation_status: String,
    pub queue_position: i64,
    pub systems: Vec<String>,
    pub system_count: i64,
    pub passed_count: i64,
    pub policy_failed_count: i64,
    pub eval_failed_count: i64,
    /// 1-indexed attempt number within this evaluation's retry lineage.
    #[serde(default = "default_attempt_number")]
    pub attempt_number: i32,
}

/// Request payload for persisting evaluation queue ordering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReorderEvalQueueRequest {
    pub ordered_commit_ids: Vec<i32>,
}

/// A single row in the eval history list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalHistoryItem {
    pub commit_id: i32,
    pub flake_id: i32,
    pub flake_name: String,
    pub branch: String,
    pub commit_hash: String,
    pub commit_message: Option<String>,
    pub author: Option<String>,
    pub committed_at: DateTime<Utc>,
    pub enqueued_at: DateTime<Utc>,
    #[serde(default)]
    pub is_latest_per_flake: bool,
    pub evaluation_status: String,
    pub evaluation_completed_at: Option<DateTime<Utc>>,
    pub evaluation_duration_ms: Option<i64>,
    pub evaluation_error_message: Option<String>,
    pub system_count: i64,
    pub passed_count: i64,
    pub policy_failed_count: i64,
    pub eval_failed_count: i64,
    pub alert_occurrence_id: String,
    /// 1-indexed attempt number within this evaluation's retry lineage.
    #[serde(default = "default_attempt_number")]
    pub attempt_number: i32,
}

/// Paginated response for GET /api/v1/commits/eval-history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalHistoryPage {
    pub total_count: i64,
    #[serde(default)]
    pub domain_total: i64,
    pub page: i64,
    pub limit: i64,
    pub items: Vec<EvalHistoryItem>,
}

/// A single evaluation log entry (persisted to database).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalLogEntry {
    pub timestamp: DateTime<Utc>,
    pub sequence: i32,
    pub level: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalPolicyMatrixResponse {
    pub commit_id: i32,
    pub policies: Vec<String>,
    pub systems: Vec<EvalPolicySystemRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalPolicySystemRow {
    pub system_name: String,
    pub results: Vec<String>,
    #[serde(default)]
    pub details: Vec<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalDependencyGraphResponse {
    pub commit_id: i32,
    pub total_packages: i64,
    pub packages: Vec<EvalDependencyPackageRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalDependencyPackageRow {
    pub package_name: String,
    #[serde(default)]
    pub closure_counted: bool,
    /// Built (store_path present / BuildComplete).
    pub ready_count: i64,
    /// Evaluated but not yet built.
    pub pending_count: i64,
    /// Eval or build failed.
    pub failed_count: i64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Pagination
// ─────────────────────────────────────────────────────────────────────────────

/// Wrapper for paginated list responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

impl<T> PaginatedResponse<T> {
    pub fn total_pages(&self) -> i64 {
        if self.per_page == 0 {
            return 0;
        }
        (self.total + self.per_page - 1) / self.per_page
    }
}

/// Query parameters for the systems list endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemsListParams {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub search: Option<String>,
    pub health_status: Option<HealthStatus>,
    pub deployment_status: Option<DeploymentStatus>,
    pub environment: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<SortOrder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRollbackRequest {
    pub target_commit: String,
}

/// Requests rollback to an exact retained generation artifact.
///
/// Either the retained artifact identity or the system-local generation
/// authorizes target resolution. [`Self::store_path`] can only narrow that
/// server-side identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRollbackGenerationRequest {
    /// Identifies the durable retained generation artifact.
    pub generation_snapshot_id: Option<Uuid>,
    /// Identifies the retained generation within its system.
    pub generation: Option<i32>,
    /// Narrows artifact resolution without granting rollback authority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_path: Option<String>,
}

/// Selects how a manual deployment request treats an `auto_latest` policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualDeploymentAction {
    /// Deploy under the current manual or pinned policy.
    #[default]
    Deploy,
    /// Deploy once and preserve the persisted `auto_latest` policy.
    ContinueAutoLatest,
    /// Persist the manual policy before attempting deployment.
    ConvertToManual,
}

/// Requests deployment of a specific commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploySystemRequest {
    /// Full 40- or 64-character hexadecimal commit identity to deploy.
    pub commit_sha: String,
    /// Specifies how the request handles an `auto_latest` system.
    #[serde(default)]
    pub action: ManualDeploymentAction,
    /// Stable identity reused while retrying the same deployment intent.
    pub request_id: Option<Uuid>,
}

/// Persisted system policy after a manual deployment request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualDeploymentPolicyState {
    /// Automatic latest-commit deployment remains persisted.
    AutoLatest,
    /// Manual deployment is persisted.
    Manual,
    /// The pinned deployment policy remains persisted.
    Pinned,
}

/// Policy conversion result for a manual deployment request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualDeploymentConversionState {
    /// The request did not ask to change policy.
    NotRequested,
    /// This request persisted the manual policy.
    Converted,
    /// An earlier request already persisted the manual policy.
    AlreadyManual,
}

/// Pending deployment result for a manual deployment request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualDeploymentRequestState {
    /// This request created pending deployment work.
    Queued,
    /// Active pending work already exists for this target.
    AlreadyQueued,
    /// No deployment work was queued by this attempt.
    Failed,
}

/// Reports persisted policy and deployment state independently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualDeploymentResponse {
    /// Persisted policy after the request.
    pub policy: ManualDeploymentPolicyState,
    /// Policy conversion result.
    pub conversion: ManualDeploymentConversionState,
    /// Pending deployment result.
    pub deployment: ManualDeploymentRequestState,
    /// New or reused pending deployment identity.
    pub deployment_id: Option<Uuid>,
    /// Human-readable result including partial success.
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCommitsResponse {
    pub commits: Vec<CommitInfo>,
    pub current_commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemGenerationsResponse {
    pub generations: Vec<SystemGeneration>,
    pub current_generation: Option<i32>,
}

/// Describes one system-local generation retained by the server.
///
/// Rollback eligibility depends on retained snapshot identity. A store path is
/// optional metadata and cannot authorize rollback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemGeneration {
    /// Identifies the generation within its system.
    pub generation: i32,
    /// Provides an optional store-path narrowing hint.
    pub store_path: Option<String>,
    /// Identifies the full commit associated with the generation, when known.
    pub commit_hash: Option<String>,
    /// Records when the server first observed the generation.
    pub timestamp: DateTime<Utc>,
    /// Indicates whether this generation is currently active.
    pub is_current: bool,
    /// Identifies the durable retained artifact that authorizes exact rollback.
    #[serde(default)]
    pub generation_snapshot_id: Option<Uuid>,
    /// Indicates whether the server can resolve exact retained rollback lineage.
    #[serde(default)]
    pub rollback_eligible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyGenerationClosureRequest {
    pub store_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyGenerationClosureResponse {
    pub available: bool,
    pub message: String,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemDeploymentProgress {
    pub id: Uuid,
    pub stage: String,
    pub kind: String,
    pub target_store_path: String,
    #[serde(default)]
    pub target_commit: Option<String>,
    #[serde(default)]
    pub target_generation: Option<i64>,
    pub source: String,
    pub issued_at: DateTime<Utc>,
    #[serde(default)]
    pub delivered_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub applying_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub failed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub failure_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemHistoryEntry {
    #[serde(default)]
    pub id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub occurred_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub observed_at: Option<DateTime<Utc>>,
    pub store_path: Option<String>,
    pub system_configuration_name: Option<String>,
    pub change_reason: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub event_rank: Option<i16>,
    #[serde(default)]
    pub title: Option<String>,
    pub commit_hash: Option<String>,
    pub flake_name: Option<String>,
    pub flake_repo_url: Option<String>,
    pub actor: String,
    pub outcome: String,
    /// Authoritative event classification derived from `change_reason`:
    /// `cf_deployment`, `local_rebuild`, `restart`, `agent_restart`, or `state_change`.
    #[serde(default)]
    pub event_kind: String,
    /// Recorded generation number at this transition.
    #[serde(default)]
    pub generation: Option<i32>,
    #[serde(default)]
    pub previous_generation: Option<i64>,
    #[serde(default)]
    pub new_generation: Option<i64>,
    #[serde(default)]
    pub previous_store_path: Option<String>,
    #[serde(default)]
    pub new_store_path: Option<String>,
    #[serde(default)]
    pub previous_boot_id: Option<String>,
    #[serde(default)]
    pub new_boot_id: Option<String>,
    #[serde(default)]
    pub deployment_id: Option<Uuid>,
    #[serde(default)]
    pub desired_target_id: Option<Uuid>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub correlation_id: Option<Uuid>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Whether the running store path maps to a tracked flake commit.
    #[serde(default)]
    pub reconciled: bool,
    /// Whether this recorded generation matched the current store path.
    #[serde(default)]
    pub generation_matches_current_store_path: Option<bool>,
    /// Per-event restart classification: "system_reboot", "agent_restart", "unknown", or None.
    #[serde(default)]
    pub restart_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemAgentEvent {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub event_type: String,
    pub message: String,
    pub deployment_related: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMutationResponse {
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveScanEligibilityResponse {
    pub eligible: bool,
    pub reason: Option<String>,
    pub derivation_id: Option<i32>,
    pub config_name: Option<String>,
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveScanTriggerResponse {
    pub scan_id: uuid::Uuid,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveScanStatusResponse {
    pub scan_id: uuid::Uuid,
    pub derivation_id: i32,
    pub status: String,
    pub scanner_name: String,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub attempts: i32,
    pub total_vulnerabilities: i32,
    pub critical_count: i32,
    pub high_count: i32,
    pub medium_count: i32,
    pub low_count: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardeningFleetSummaryResponse {
    pub total_systems_scanned: i64,
    pub avg_fleet_score: Option<f64>,
    pub total_well_hardened_services: i64,
    pub total_moderately_hardened_services: i64,
    pub total_poorly_hardened_services: i64,
    pub total_vulnerable_services: i64,
    pub total_services_scanned: i64,
    pub last_scan_completed: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardeningTopServiceResponse {
    pub service_name: String,
    pub affected_systems_count: i64,
    pub avg_score: f64,
    pub min_score: i32,
    pub max_score: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardeningSystemPostureResponse {
    pub system_id: Option<Uuid>,
    pub derivation_id: i32,
    pub config_name: String,
    pub hostname: Option<String>,
    pub environment_name: Option<String>,
    pub latest_scan_id: Option<Uuid>,
    pub overall_score: Option<i32>,
    pub risk_level: Option<String>,
    pub total_services: Option<i32>,
    pub well_hardened_count: Option<i32>,
    pub moderately_hardened_count: Option<i32>,
    pub poorly_hardened_count: Option<i32>,
    pub vulnerable_count: Option<i32>,
    pub last_scan_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardeningServiceResultResponse {
    pub id: Uuid,
    pub scan_id: Uuid,
    pub service_name: String,
    pub service_type: Option<String>,
    pub hardening_score: i32,
    pub risk_level: String,
    pub directives_detail: serde_json::Value,
    pub enabled_directives_count: i32,
    pub disabled_directives_count: i32,
    pub missing_directives_count: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardeningJustificationResponse {
    pub id: Uuid,
    pub system_id: Uuid,
    pub service_name: String,
    pub directive_name: Option<String>,
    pub category: Option<String>,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveHardeningJustificationRequest {
    pub directive_name: Option<String>,
    pub category: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardeningScanEligibilityResponse {
    pub eligible: bool,
    pub reason: Option<String>,
    pub derivation_id: Option<i32>,
    pub config_name: Option<String>,
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardeningScanTriggerResponse {
    pub scan_id: uuid::Uuid,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardeningScanStatusResponse {
    pub scan_id: uuid::Uuid,
    pub derivation_id: i32,
    pub status: String,
    pub error_message: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub attempts: i32,
    pub total_services: i32,
    pub overall_score: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSystemRequest {
    pub hostname: String,
    pub system_configuration_name: Option<String>,
    pub public_key: String,
    pub environment: Option<String>,
    pub flake_name: Option<String>,
    pub deployment_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSystemRequest {
    pub hostname: String,
    #[serde(default)]
    pub fqdn: Option<String>,
    pub system_configuration_name: Option<String>,
    pub environment: Option<String>,
    pub flake_name: Option<String>,
    pub deployment_policy: String,
    /// Tri-state heartbeat interval in seconds. Omitting the key preserves the persisted value;
    /// sending `null` clears it (falls back to server default of 600s); sending a value sets it.
    /// Valid range: 15-900 seconds.
    ///
    /// `skip_serializing_if` is required: without it, `Unset` serializes as `null`,
    /// which the server interprets as `Clear` and wipes the stored override.
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unset")]
    pub heartbeat_interval_secs: FieldUpdate<i32>,
}

fn default_flake_build_scope() -> String {
    "cf_systems_only".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSystemPublicKeyRequest {
    pub public_key: String,
}

/// Sort direction for list queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Asc,
    Desc,
}

// ─────────────────────────────────────────────────────────────────────────────
// System History / Events DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// A commit in the system's flake history with deployment status for this specific system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemCommitHistory {
    /// Full commit hash.
    pub hash: String,
    /// Short commit message (first line).
    pub message: String,
    /// Commit author name.
    pub author: String,
    /// When the commit was made.
    pub committed_at: DateTime<Utc>,
    /// Whether this commit was deployed to this system.
    pub was_deployed: bool,
    /// When this commit was deployed to this system (if it was).
    pub deployed_at: Option<DateTime<Utc>>,
    /// Whether this is the currently running config on the system.
    pub is_current: bool,
    /// Whether this build is ready to deploy but not yet deployed.
    #[serde(default)]
    pub is_ready_to_deploy: bool,
    /// Build status for this commit (queued/building/etc).
    #[serde(default)]
    pub build_status: Option<BuildStatus>,
    /// Config diff summary (e.g., "+5 -3 lines" or "nginx, redis changed").
    pub diff_summary: Option<String>,
    /// Optional external flake repository URL for this timeline entry.
    #[serde(default)]
    pub flake_repo_url: Option<String>,
    /// Optional config identity shown on timeline cards.
    #[serde(default)]
    pub config_identity: Option<String>,
}

/// Deployment log entry for the Logs tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentLogEntry {
    /// Log line content.
    pub message: String,
    /// Timestamp of the log entry.
    pub timestamp: DateTime<Utc>,
    /// Log level (info, warn, error).
    pub level: LogLevel,
    /// Optional phase/stage this log belongs to.
    pub phase: Option<String>,
}

/// Log severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl LogLevel {
    pub fn color_class(&self) -> &'static str {
        match self {
            Self::Info => "text-gray-400",
            Self::Warn => "text-yellow-400",
            Self::Error => "text-red-400",
            Self::Debug => "text-gray-500",
        }
    }
}

/// A CVE vulnerability detail for the CVE drilldown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemVulnerability {
    /// CVE ID (e.g., CVE-2024-1234).
    pub cve_id: String,
    /// Severity level.
    pub severity: CveSeverity,
    /// CVSS score if available.
    pub cvss_score: Option<f32>,
    /// Short description.
    pub description: String,
    /// Affected package name.
    pub package_name: String,
    /// Installed version.
    pub installed_version: String,
    /// Fixed version (if known).
    pub fixed_version: Option<String>,
    /// First observed in fleet scans.
    #[serde(default)]
    pub first_seen: Option<DateTime<Utc>>,
    /// When this CVE was published.
    #[serde(default)]
    pub published_at: Option<DateTime<Utc>>,
    /// Current status (open/fixed/ignored).
    #[serde(default)]
    pub status: Option<String>,
    /// Optional saved justification category for this system/CVE.
    #[serde(default)]
    pub justification_category: Option<String>,
    /// Optional saved justification reason for this system/CVE.
    #[serde(default)]
    pub justification_reason: Option<String>,
    /// Last update timestamp for saved justification.
    #[serde(default)]
    pub justification_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveSystemCveJustificationRequest {
    pub category: Option<String>,
    pub reason: String,
}

impl CveSeverity {
    /// CSS background color class from the design system.
    pub fn bg_class(&self) -> &'static str {
        use crate::theme::cve;
        match self {
            Self::Critical => cve::CRITICAL_BG,
            Self::High => cve::HIGH_BG,
            Self::Medium => cve::MEDIUM_BG,
            Self::Low => cve::LOW_BG,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Authentication Context DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Authentication mode the server is operating under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    /// Development mode with local fixture users.
    Dev,
    /// Local username/password authentication.
    Local,
    /// OIDC-based authentication.
    Oidc,
}

/// User role in the RBAC system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Role {
    Admin,
    Operator,
    Viewer,
}

/// Authenticated user information for the current session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
}

/// Authentication context exposed to the UI.
///
/// This is the single source of truth for auth state in the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// Whether the user is authenticated.
    pub is_authenticated: bool,
    /// Authenticated user information (None if not authenticated).
    pub user: Option<AuthUser>,
    /// Roles assigned to the current user (empty if not authenticated).
    pub roles: Vec<Role>,
    /// Authentication mode the server is using.
    pub auth_mode: AuthMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiThemePreference {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiDensityPreference {
    Comfortable,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SystemsViewPreference {
    Cards,
    Table,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserPreferencesDto {
    pub user_id: Uuid,
    pub theme: String,
    pub density: String,
    pub sidebar_collapsed: bool,
    pub default_systems_view: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserPreferencesResponse {
    pub preferences: Option<UserPreferencesDto>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateUserPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<UiThemePreference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<UiDensityPreference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar_collapsed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_systems_view: Option<SystemsViewPreference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationDeliveryChannel {
    InApp,
    Email,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationPreferencesDto {
    pub deploy_failures: bool,
    pub build_failures: bool,
    pub critical_cves: bool,
    pub policy_violations: bool,
    pub heartbeat_lost: bool,
    pub weekly_digest: bool,
    pub delivery_channel: NotificationDeliveryChannel,
    pub email_available: bool,
    pub delivery_email: Option<String>,
    pub email_unavailable_reason: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateNotificationPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deploy_failures: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_failures: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critical_cves: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_violations: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_lost: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekly_digest: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_channel: Option<NotificationDeliveryChannel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationCategory {
    DeployFailures,
    BuildFailures,
    CriticalCves,
    PolicyViolations,
    HeartbeatLost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserNotificationDto {
    pub id: Uuid,
    pub category: NotificationCategory,
    pub title: String,
    pub summary: String,
    pub route: String,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserNotificationsResponse {
    pub notifications: Vec<UserNotificationDto>,
    pub unread_count: i64,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserSessionDto {
    pub id: Uuid,
    pub current: bool,
    pub device_label: String,
    pub browser: String,
    pub operating_system: String,
    pub device_class: String,
    pub ip_address: Option<String>,
    pub auth_source: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserSessionsResponse {
    pub sessions: Vec<UserSessionDto>,
}

/// Admin users list item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminUserSummary {
    pub id: String,
    pub identifier: String,
    pub identity_source: IdentitySource,
    pub role: Option<Role>,
    pub enabled: bool,
    pub environments: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySource {
    LocalManaged,
    OidcDerived,
}

/// Audit event action classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    UserCreated,
    UserUpdated,
    UserDeleted,
    UserEnabled,
    UserDisabled,
    UserRoleAssigned,
    UserEnvironmentMembershipUpdated,
    OidcMappingChanged,
    SystemSyncRequested,
    SystemDeployRequested,
    SystemRollbackRequested,
    SessionInvalidated,
    CveScanRequested,
    BuilderRotateKey,
    FlakeSync,
    EvalCancel,
    CacheCreate,
    PolicyEdit,
    UserCreate,
    SystemRollback,
    AuthLogin,
    AuthLoginDenied,
    BuildComplete,
    CveAccept,
    SystemDeploy,
    #[serde(other)]
    Unknown,
}

/// Admin audit log entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub actor: Option<String>,
    pub action: AuditAction,
    pub target: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OidcGroupMapping {
    pub id: String,
    pub group_name: String,
    pub role: Option<Role>,
    pub environments: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminUpsertOidcMappingRequest {
    pub group_name: String,
    pub role: Option<Role>,
    pub environments: Vec<String>,
}

/// Query parameters for admin audit event listing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminAuditEventsParams {
    pub actor: Option<String>,
    pub action: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Request payload for creating a local admin-managed user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminCreateUserRequest {
    pub email: String,
    pub display_name: Option<String>,
    pub password: Option<String>,
    pub role: Role,
    pub environments: Vec<String>,
}

/// Request payload for updating a local admin-managed user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminUpdateUserRequest {
    pub role: Option<Role>,
    pub enabled: Option<bool>,
    pub environments: Option<Vec<String>>,
    pub password: Option<String>,
}

/// Reports whether one server-derived setup coach step is complete.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetupWizardStepStatus {
    /// Indicates whether at least one qualifying entity exists.
    pub complete: bool,
    /// Gives the number of qualifying persisted records or lineages for the step.
    pub count: i64,
}

/// Reports administrator setup coach progress derived from persisted state.
///
/// Optional coach fields preserve compatibility with the legacy six-step API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupWizardProgressResponse {
    /// Indicates whether the current administrator dismissed the coach.
    pub dismissed: bool,
    /// Indicates whether the current administrator acknowledged agent setup.
    pub agent_acknowledged: bool,
    /// Reports environment setup progress.
    pub environment: SetupWizardStepStatus,
    /// Reports flake setup progress.
    pub flake: SetupWizardStepStatus,
    /// Reports builder setup progress.
    pub builder: SetupWizardStepStatus,
    /// Reports cache destination setup progress.
    pub cache: SetupWizardStepStatus,
    /// Reports progress for systems linked to an environment and flake.
    pub system: SetupWizardStepStatus,
    /// Reports policy lineage progress from user-attributed policy versions.
    #[serde(default)]
    pub policy: Option<SetupWizardStepStatus>,
    /// Reports compliance bundle lineage progress.
    #[serde(default)]
    pub bundle: Option<SetupWizardStepStatus>,
    /// Reports POA&M progress across all lifecycle states.
    #[serde(default)]
    pub poam: Option<SetupWizardStepStatus>,
    /// Indicates whether the original five infrastructure steps are complete.
    pub all_required_complete: bool,
    /// Indicates whether all nine setup coach steps are complete.
    #[serde(default)]
    pub all_coach_steps_complete: Option<bool>,
}

#[cfg(test)]
mod setup_wizard_tests {
    use super::*;

    #[test]
    fn older_setup_progress_preserves_absent_new_steps() {
        let progress: SetupWizardProgressResponse = serde_json::from_value(serde_json::json!({
            "dismissed": false,
            "agent_acknowledged": true,
            "environment": { "complete": true, "count": 1 },
            "flake": { "complete": true, "count": 1 },
            "builder": { "complete": true, "count": 1 },
            "cache": { "complete": true, "count": 1 },
            "system": { "complete": true, "count": 1 },
            "all_required_complete": true
        }))
        .expect("older setup progress should deserialize");

        assert_eq!(progress.policy, None);
        assert_eq!(progress.bundle, None);
        assert_eq!(progress.poam, None);
        assert_eq!(progress.all_coach_steps_complete, None);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupWizardDismissRequest {
    pub dismissed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupWizardAcknowledgeAgentRequest {
    pub acknowledged: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Development Auth DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Request payload for dev mode login.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevLoginRequest {
    pub email: String,
}

/// Response from dev mode login endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevLoginResponse {
    pub user_id: String,
    pub email: String,
    pub display_name: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Local Auth DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Request payload for local login.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLoginRequest {
    pub username: String,
    pub password: String,
}

/// Response from local login endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLoginResponse {
    pub user_id: String,
    pub username: String,
    pub email: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Error Response
// ─────────────────────────────────────────────────────────────────────────────

/// Standard error envelope for API error responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Builder Management
// ─────────────────────────────────────────────────────────────────────────────

/// Builder status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuilderStatus {
    #[serde(alias = "Active")]
    Active,
    #[serde(alias = "Inactive")]
    Inactive,
    #[serde(alias = "Offline")]
    Offline,
    #[serde(alias = "Draining")]
    Draining,
}

impl BuilderStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "running",
            Self::Inactive => "paused",
            Self::Offline => "offline",
            Self::Draining => "draining",
        }
    }

    pub fn color_class(&self) -> &'static str {
        match self {
            Self::Active => "text-emerald-400",
            Self::Inactive => "text-amber-400",
            Self::Offline => "text-red-400",
            Self::Draining => "text-blue-400",
        }
    }

    pub fn bg_class(&self) -> &'static str {
        match self {
            Self::Active => "bg-emerald-500/10",
            Self::Inactive => "bg-amber-500/10",
            Self::Offline => "bg-red-500/10",
            Self::Draining => "bg-blue-500/10",
        }
    }

    pub fn dot_class(&self) -> &'static str {
        match self {
            Self::Active => "bg-emerald-400",
            Self::Inactive => "bg-amber-400",
            Self::Offline => "bg-red-400",
            Self::Draining => "bg-blue-400",
        }
    }

    /// JSX-compatible chip class matching BuildersView.jsx
    pub fn chip_class(&self) -> &'static str {
        match self {
            Self::Active => "chip-healthy",
            Self::Inactive => "chip-warning",
            Self::Offline => "chip-critical",
            Self::Draining => "chip-info",
        }
    }

    /// JSX-compatible dot color matching BuildersView.jsx
    pub fn dot_color(&self) -> &'static str {
        match self {
            Self::Active => "#34d399",
            Self::Inactive => "#fbbf24",
            Self::Offline => "#f87171",
            Self::Draining => "#60a5fa",
        }
    }
}

/// Builder assigned environment info
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuilderEnvironmentInfo {
    pub name: String,
    pub color_hex: String,
}

/// Builder summary for list view
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuilderSummary {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub arch: String,
    pub status: BuilderStatus,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: i32,
    pub enabled: bool,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub assigned_environment_count: i32,
    #[serde(default)]
    pub active_jobs: i32,
    #[serde(default)]
    pub queued_jobs: i32,
    #[serde(default)]
    pub assigned_environments: Vec<BuilderEnvironmentInfo>,
    pub public_key_fingerprint: String,
    pub registered: bool,
    #[serde(default)]
    pub load_avg: Option<f64>,
    #[serde(default)]
    pub completed_24h: i32,
    #[serde(default)]
    pub failed_24h: i32,
}

/// Full builder details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderDetail {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub arch: String,
    pub public_key: String,
    #[serde(default)]
    pub public_key_fingerprint: String,
    pub status: BuilderStatus,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: i32,
    pub enabled: bool,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub assigned_environment_ids: Vec<Uuid>,
}

/// Response returned when creating a builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderCreatedResponse {
    pub builder: BuilderDetail,
    #[serde(default)]
    pub private_key: Option<String>,
    #[serde(default)]
    pub assigned_environment_ids: Vec<Uuid>,
}

/// Create builder request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBuilderRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default = "default_arch")]
    pub arch: String,
    pub public_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cpu_cores: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_jobs: Option<i32>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub environment_ids: Vec<Uuid>,
}

fn default_arch() -> String {
    "x86_64-linux".to_string()
}

fn default_enabled() -> bool {
    true
}

/// Update builder request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBuilderRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BuilderStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cpu_cores: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_jobs: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Update builder environments request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBuilderEnvironmentsRequest {
    pub environment_ids: Vec<Uuid>,
}

/// Update builder public key request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBuilderPublicKeyRequest {
    pub public_key: String,
}

/// Builder metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ─────────────────────────────────────────────────────────────────────────────
// Cache Management
// ─────────────────────────────────────────────────────────────────────────────

/// Cache destination configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheDestination {
    pub id: i32,
    pub name: String,
    pub cache_type: String,
    pub push_to: Option<String>,
    pub enabled: bool,
    pub signing_key_path: Option<String>,
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
    pub attic_ignore_upstream_cache_filter: Option<bool>,
    pub attic_jobs: Option<i32>,
    pub parallel_uploads: Option<i32>,
    pub max_retries: Option<i32>,
    pub retry_delay_seconds: Option<i64>,
    pub push_timeout_seconds: Option<i64>,
    pub force_repush: Option<bool>,
    pub require_sigs: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Create cache destination request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateCacheDestination {
    pub name: String,
    pub cache_type: String,
    pub push_to: Option<String>,
    pub enabled: Option<bool>,
    pub signing_key_path: Option<String>,
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
    pub attic_ignore_upstream_cache_filter: Option<bool>,
    pub attic_jobs: Option<i32>,
    pub parallel_uploads: Option<i32>,
    pub max_retries: Option<i32>,
    pub retry_delay_seconds: Option<i64>,
    pub push_timeout_seconds: Option<i64>,
    pub force_repush: Option<bool>,
    pub require_sigs: Option<bool>,
    pub environment_ids: Option<Vec<Uuid>>,
}

/// Update cache destination request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateCacheDestination {
    pub name: Option<String>,
    pub cache_type: Option<String>,
    pub push_to: Option<String>,
    pub enabled: Option<bool>,
    pub signing_key_path: Option<String>,
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
    pub attic_ignore_upstream_cache_filter: Option<bool>,
    pub attic_jobs: Option<i32>,
    pub parallel_uploads: Option<i32>,
    pub max_retries: Option<i32>,
    pub retry_delay_seconds: Option<i64>,
    pub push_timeout_seconds: Option<i64>,
    pub force_repush: Option<bool>,
    pub require_sigs: Option<bool>,
    pub environment_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheCredentialTestResult {
    pub ok: bool,
    pub status_code: Option<u16>,
    pub message: String,
    pub tested_url: Option<String>,
}

/// Cache push job status and details
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachePushJob {
    pub id: i32,
    pub derivation_id: i32,
    pub status: String,
    pub store_path: Option<String>,
    pub scheduled_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub attempts: i32,
    pub error_message: Option<String>,
    pub push_size_bytes: Option<i64>,
    pub push_duration_ms: Option<i32>,
    pub cache_destination: Option<String>,
}

/// Bulk job action request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkJobAction {
    pub job_ids: Vec<i32>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Admin Config Health DTOs — GET /api/v1/admin/config-health
// ─────────────────────────────────────────────────────────────────────────────

/// A single pipeline readiness check result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigHealthCheck {
    /// Stable identifier for this check (e.g. `"no_flakes"`).
    pub id: String,
    /// Whether this check passed (no issue detected).
    pub passed: bool,
    /// Human-readable description shown when the check fails.
    pub message: String,
    /// URL path the admin can navigate to in order to resolve the issue.
    pub action_url: String,
}

/// Top-level response for `GET /api/v1/admin/config-health`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigHealthResponse {
    pub has_flakes: bool,
    pub has_environments: bool,
    pub has_builders: bool,
    pub has_cache_destinations: bool,
    /// Total number of failing checks.
    pub total_issues: u32,
    /// Per-check details for all pipeline readiness checks.
    pub checks: Vec<ConfigHealthCheck>,
}

/// Runtime database information displayed in Admin → Server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseRuntimeInfo {
    pub status: String,
    pub name: String,
    pub size: String,
    pub server_version: String,
}

/// Runtime server/build information displayed in Admin → Server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerRuntimeInfoResponse {
    pub version: String,
    pub commit: Option<String>,
    pub uptime_seconds: u64,
    pub database: DatabaseRuntimeInfo,
    pub active_sessions: i64,
    pub oidc_issuer_url: Option<String>,
    pub tls_status: String,
    pub tls_detail: String,
}

/// Persisted classification banner configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationBannerConfig {
    pub enabled: bool,
    pub level: String,
    pub custom_text: String,
}

/// Request payload for updating classification banner configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateClassificationBannerRequest {
    pub enabled: bool,
    pub level: String,
    pub custom_text: String,
}

/// Persisted server-wide automatic retry policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomaticRetryPolicy {
    pub max_build_retries: i16,
    pub max_evaluation_retries: i16,
    pub backoff_seconds: i32,
    pub transient_only: bool,
}

impl Default for AutomaticRetryPolicy {
    fn default() -> Self {
        Self {
            max_build_retries: 2,
            max_evaluation_retries: 1,
            backoff_seconds: 30,
            transient_only: true,
        }
    }
}

/// Complete replacement payload for the automatic retry policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateAutomaticRetryPolicyRequest {
    pub max_build_retries: i16,
    pub max_evaluation_retries: i16,
    pub backoff_seconds: i32,
    pub transient_only: bool,
}

#[cfg(test)]
mod reconciliation_tests {
    use super::*;

    #[test]
    fn field_update_default_is_unset() {
        let value: FieldUpdate<i32> = FieldUpdate::default();
        assert_eq!(value, FieldUpdate::Unset);
    }

    #[test]
    fn update_system_request_omits_unset_heartbeat_interval() {
        // Unset must be omitted entirely: serializing as `null` would be
        // interpreted as Clear by the server, wiping the stored override
        // during unrelated edits (the original P1-3 bug).
        let request = UpdateSystemRequest {
            hostname: "web01".into(),
            fqdn: None,
            system_configuration_name: None,
            environment: None,
            flake_name: None,
            deployment_policy: "manual".into(),
            heartbeat_interval_secs: FieldUpdate::Unset,
        };

        let value = serde_json::to_value(request).expect("request should serialize");
        assert!(
            !value
                .as_object()
                .expect("request serializes as object")
                .contains_key("heartbeat_interval_secs"),
            "Unset heartbeat_interval_secs must be omitted from the payload"
        );
    }

    #[test]
    fn update_system_request_serializes_clear_heartbeat_interval_as_null() {
        let request = UpdateSystemRequest {
            hostname: "web01".into(),
            fqdn: None,
            system_configuration_name: None,
            environment: None,
            flake_name: None,
            deployment_policy: "manual".into(),
            heartbeat_interval_secs: FieldUpdate::Clear,
        };

        let value = serde_json::to_value(request).expect("request should serialize");
        assert_eq!(
            value.get("heartbeat_interval_secs"),
            Some(&serde_json::Value::Null),
            "Clear must serialize as explicit null"
        );
    }

    #[test]
    fn update_system_request_serializes_set_heartbeat_interval_as_value() {
        let request = UpdateSystemRequest {
            hostname: "web01".into(),
            fqdn: None,
            system_configuration_name: None,
            environment: None,
            flake_name: None,
            deployment_policy: "manual".into(),
            heartbeat_interval_secs: FieldUpdate::Set(120),
        };

        let value = serde_json::to_value(request).expect("request should serialize");
        assert_eq!(
            value.get("heartbeat_interval_secs"),
            Some(&serde_json::json!(120)),
            "Set(120) must serialize as 120"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TASK-418: Compliance Frameworks, Requirements, Mappings, and Coverage
// ─────────────────────────────────────────────────────────────────────────────

/// Summary of a compliance framework lineage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceFrameworkSummary {
    pub id: Uuid,
    pub name: String,
    pub publisher: Option<String>,
    pub canonical_source_key: String,
    pub description: Option<String>,
    pub version_count: i64,
}

/// Summary of a specific framework version (release).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceFrameworkVersionSummary {
    pub id: Uuid,
    pub framework_id: Uuid,
    pub version: String,
    pub canonical_release_key: String,
    pub title: Option<String>,
    pub published_at: Option<String>,
    pub semantic_digest: String,
    #[serde(default = "default_finalized")]
    pub migration_recovery_status: String,
    pub migration_recovery_reason: Option<String>,
    pub requirement_count: i64,
}

fn default_finalized() -> String {
    "finalized".to_string()
}

/// Compact projection used to split a bundle picker into mapped policies and
/// explicit custom additions for a selected normalized framework.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameworkMappedPolicyVersionsResponse {
    #[serde(default)]
    pub policy_version_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleVersionRequirementMembership {
    pub requirement_version_id: Uuid,
    pub requirement_id: Uuid,
    pub framework_id: Uuid,
    pub framework_version_id: Uuid,
    pub framework_name: String,
    pub framework_version: String,
    pub external_id: String,
    pub title: Option<String>,
    pub kind: String,
    pub selected: bool,
    pub requirement_order: i32,
}

/// A single requirement version row from a search or list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequirementVersionSummary {
    pub id: Uuid,
    pub requirement_id: Uuid,
    pub framework_version_id: Uuid,
    pub external_id: String,
    pub title: Option<String>,
    pub kind: String,
    pub severity: Option<String>,
    pub parent_requirement_version_id: Option<Uuid>,
    pub semantic_digest: String,
}

/// A policy-requirement mapping row returned by the list endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyMappingRow {
    pub id: Uuid,
    pub policy_version_id: Uuid,
    pub requirement_version_id: Uuid,
    pub relationship: String,
    pub coverage: String,
    pub rationale: Option<String>,
    pub provenance: String,
    pub trust_state: String,
    // Joined framework/requirement data for display.
    pub framework_id: Uuid,
    pub framework_name: String,
    pub framework_version_id: Uuid,
    pub framework_version: String,
    pub requirement_external_id: String,
    pub requirement_title: Option<String>,
}

/// Coverage classification for a single requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementCoverage {
    Full,
    Partial,
    Unmapped,
    /// The requirement's framework release is pending migration recovery and
    /// cannot supply authoritative evidence, but is still counted in the total.
    RecoveryRequired,
}

/// One row in the bundle requirement coverage report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleCoverageRow {
    pub requirement_version_id: Uuid,
    pub external_id: String,
    pub title: Option<String>,
    pub kind: String,
    pub parent_requirement_version_id: Option<Uuid>,
    pub coverage: RequirementCoverage,
    pub mapped_policy_version_ids: Vec<Uuid>,
    pub mappings: Vec<BundleCoverageMapping>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleCoverageMapping {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    pub policy_name: String,
    pub relationship: String,
    pub coverage: String,
    pub provenance: String,
    pub rationale: Option<String>,
}

/// Aggregated requirement coverage for a bundle version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleCoverageReport {
    pub bundle_version_id: Uuid,
    #[serde(default)]
    pub source_framework: Option<BundleCoverageSourceFramework>,
    #[serde(default)]
    pub frameworks: Vec<BundleCoverageFramework>,
    pub total_requirements: i64,
    pub full: i64,
    pub partial: i64,
    pub unmapped: i64,
    #[serde(default)]
    pub recovery_required: i64,
    pub rows: Vec<BundleCoverageRow>,
}

/// Authoritative source framework identity of a bundle version, independent of
/// requirement membership. A DISA STIG bundle with zero normalized requirements
/// still reports its source framework through this field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleCoverageSourceFramework {
    pub framework_id: Uuid,
    pub framework_name: String,
    pub framework_version_id: Uuid,
    pub framework_version: String,
    #[serde(default)]
    pub framework_publisher: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleCoverageFramework {
    pub framework_id: Uuid,
    pub framework_name: String,
    pub framework_version_id: Uuid,
    pub framework_version: String,
    #[serde(default)]
    pub framework_publisher: Option<String>,
}

/// Request body for creating a requirement mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatePolicyMappingRequest {
    pub requirement_version_id: Uuid,
    pub relationship: String,
    pub coverage: String,
    pub rationale: Option<String>,
    #[serde(default = "default_provenance")]
    pub provenance: String,
}

fn default_provenance() -> String {
    "manual".to_string()
}

/// Request body for updating an existing mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePolicyMappingRequest {
    pub relationship: String,
    pub coverage: String,
    pub rationale: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{ComplianceControlEvidence, XccdfPreviewResponse};

    #[test]
    fn compliance_requirement_identity_is_additive_and_rolling_compatible() {
        let mut value = serde_json::json!({
            "policy_id": "00000000-0000-0000-0000-000000000001",
            "policy_name": "Mapped policy",
            "status": "pass",
            "severity": "high",
            "summary": "Passed",
            "evidence_items": [],
            "framework_mapping": "NIST SP 800-53 Rev. 5 · AC-2"
        });
        let legacy: ComplianceControlEvidence = serde_json::from_value(value.clone()).unwrap();
        assert!(legacy.requirements.is_empty());

        value["requirements"] = serde_json::json!([{
            "requirement_version_id": "00000000-0000-0000-0000-000000000002",
            "external_id": "AC-2",
            "title": "Account Management",
            "framework_id": "00000000-0000-0000-0000-000000000003",
            "framework_name": "NIST SP 800-53",
            "framework_version_id": "00000000-0000-0000-0000-000000000004",
            "framework_version": "Rev. 5",
            "framework_title": null
        }]);
        let enriched: ComplianceControlEvidence = serde_json::from_value(value).unwrap();
        assert_eq!(enriched.requirements[0].external_id, "AC-2");
        assert_eq!(enriched.requirements[0].framework_version, "Rev. 5");
    }

    #[test]
    fn deserializes_20ac_shared_implementation_group() {
        let preview: XccdfPreviewResponse = serde_json::from_value(serde_json::json!({
            "sha256": "fixture-stig-import-sha256",
            "rule_count": 2,
            "profile_count": 0,
            "foreign_stig_reconciliation": {
                "framework": {
                    "canonical_source_key": "disa-anduril-nixos-stig",
                    "canonical_release_key": "v1r2",
                    "state": "exact_release"
                },
                "requirements": [],
                "shared_implementation_groups": [{
                    "group_id": "fixture-shared-group",
                    "requirement_keys": ["V-999001", "V-999002"],
                    "recommended_action": "reuse_existing",
                    "has_existing_candidate": true,
                    "existing_candidate": {
                        "policy_id": "11111111-1111-4111-8111-111111111111",
                        "policy_version_id": "22222222-2222-4222-8222-222222222222",
                        "policy_name": "Fixture authoritative policy",
                        "confidence": 100
                    },
                    "member_proofs": {
                        "V-999001": "exact_technical",
                        "V-999002": "shared_implementation"
                    }
                }],
                "removed_requirements": []
            }
        }))
        .expect("20ac fixture should deserialize");

        let reconciliation = preview
            .foreign_stig_reconciliation
            .expect("foreign reconciliation");
        assert_eq!(reconciliation.shared_implementation_groups.len(), 1);

        let group = &reconciliation.shared_implementation_groups[0];
        assert_eq!(group.group_id, "fixture-shared-group");
        assert_eq!(group.requirement_keys, ["V-999001", "V-999002"]);
        assert_eq!(group.recommended_action, "reuse_existing");
        assert!(group.has_existing_candidate);

        let candidate = group.existing_candidate.as_ref().expect("candidate");
        assert_eq!(candidate.policy_name, "Fixture authoritative policy");
        assert_eq!(candidate.confidence, 100);
        assert_eq!(
            group.member_proofs.get("V-999001").map(String::as_str),
            Some("exact_technical")
        );
        assert_eq!(
            group.member_proofs.get("V-999002").map(String::as_str),
            Some("shared_implementation")
        );
    }
}
