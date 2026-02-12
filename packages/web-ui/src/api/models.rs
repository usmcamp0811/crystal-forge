//! Client-side API data transfer objects.
//!
//! These mirror the server-side DTOs in `packages/default/src/api/models.rs`
//! and define the JSON contract between the Crystal Forge server and web UI.
//!
//! Kept as a separate copy (rather than shared crate) because:
//! - The server crate has native dependencies (OpenSSL, SQLx) incompatible with wasm32
//! - Client-side DTOs may diverge (e.g. adding UI-only computed fields)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    /// CSS color class for Tailwind styling.
    pub fn color_class(&self) -> &'static str {
        match self {
            Self::Healthy => "text-emerald-400",
            Self::Warning => "text-amber-400",
            Self::Critical => "text-red-400",
            Self::Offline => "text-gray-500",
        }
    }

    /// Background color class for badges.
    pub fn bg_class(&self) -> &'static str {
        match self {
            Self::Healthy => "bg-emerald-400/10",
            Self::Warning => "bg-amber-400/10",
            Self::Critical => "bg-red-400/10",
            Self::Offline => "bg-gray-500/10",
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
    /// CSS color class for Tailwind styling.
    pub fn color_class(&self) -> &'static str {
        match self {
            Self::UpToDate => "text-emerald-400",
            Self::Behind => "text-amber-400",
            Self::Ahead => "text-blue-400",
            Self::NeverDeployed => "text-gray-500",
            Self::NoCommitsAvailable => "text-gray-500",
            Self::Unknown => "text-gray-500",
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
    /// CSS color class for Tailwind styling.
    pub fn color_class(&self) -> &'static str {
        match self {
            Self::Critical => "text-red-400",
            Self::High => "text-orange-400",
            Self::Medium => "text-amber-400",
            Self::Low => "text-gray-400",
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub fleet_health: FleetHealthSummary,
    pub deployment_status: DeploymentStatusSummary,
    pub cve_summary: CveSummary,
    pub total_systems: i64,
    pub active_builds: i64,
    pub recent_deployments: Vec<RecentDeployment>,
    pub timestamp: DateTime<Utc>,
}

/// System counts grouped by health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// A single recent deployment event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentDeployment {
    pub hostname: String,
    pub commit_hash: String,
    pub deployed_at: DateTime<Utc>,
    pub status: DeploymentStatus,
}

// ─────────────────────────────────────────────────────────────────────────────
// System DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Lightweight system representation for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSummary {
    pub id: Uuid,
    pub hostname: String,
    pub environment: Option<String>,
    pub health_status: HealthStatus,
    pub deployment_status: DeploymentStatus,
    pub pipeline_stage: Option<PipelineStage>,
    pub cve_counts: CveSummary,
    pub nixos_version: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub deployment_policy: String,
}

/// Full system representation for the detail view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemDetail {
    pub id: Uuid,
    pub hostname: String,
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
    pub hardware: SystemHardwareInfo,
    pub network: SystemNetworkInfo,
    pub security: SystemSecurityInfo,
    pub cve_counts: CveSummary,
    pub flake: Option<FlakeSummary>,
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Hardware information subset for system detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHardwareInfo {
    pub cpu_brand: Option<String>,
    pub cpu_cores: Option<i32>,
    pub memory_gb: Option<f64>,
    pub uptime_secs: Option<i64>,
    pub board_serial: Option<String>,
    pub bios_version: Option<String>,
}

/// Network information subset for system detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemNetworkInfo {
    pub primary_ip: Option<String>,
    pub primary_mac: Option<String>,
    pub gateway_ip: Option<String>,
}

/// Security posture subset for system detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSecurityInfo {
    pub tpm_present: Option<bool>,
    pub secure_boot_enabled: Option<bool>,
    pub fips_mode: Option<bool>,
    pub selinux_status: Option<String>,
}

/// Flake context for a system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeSummary {
    pub id: i32,
    pub name: String,
    pub repo_url: String,
    pub latest_commit: Option<String>,
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

/// Sort direction for list queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Asc,
    Desc,
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
