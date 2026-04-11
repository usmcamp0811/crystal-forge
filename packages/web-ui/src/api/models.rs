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
pub use uuid::Uuid;

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

/// A single recent deployment event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentDeployment {
    pub hostname: String,
    pub commit_hash: String,
    pub commit_message: Option<String>,
    pub deployed_at: DateTime<Utc>,
    pub status: DeploymentStatus,
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
}

/// Full system representation for the detail view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemDetail {
    pub id: Uuid,
    pub hostname: String,
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

// ─────────────────────────────────────────────────────────────────────────────
// Environment DTOs — GET /api/v1/environments, GET /api/v1/environments/:id
// ─────────────────────────────────────────────────────────────────────────────

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
pub struct CreateEnvironmentRequest {
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateEnvironmentRequest {
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEnvironmentPoliciesRequest {
    pub required_policy_ids: Vec<uuid::Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPolicySummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
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
}

/// Response for listing deployment policies with pagination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPoliciesListResponse {
    pub policies: Vec<DeploymentPolicyRecord>,
    pub total: usize,
    pub limit: i64,
    pub offset: i64,
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
}

/// Paginated response for the build queue listing endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildQueuePageResponse {
    pub total: i64,
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
}

/// Summary of the evaluation queue for the evaluations page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalQueueSummary {
    pub active_count: i64,
    pub completed_count: i64,
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
    pub evaluation_status: String,
    pub queue_position: i64,
    pub systems: Vec<String>,
    pub system_count: i64,
    pub passed_count: i64,
    pub policy_failed_count: i64,
    pub eval_failed_count: i64,
}

/// Request payload for persisting evaluation queue ordering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReorderEvalQueueRequest {
    pub ordered_commit_ids: Vec<i32>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploySystemRequest {
    pub commit_sha: String,
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
    pub system_configuration_name: Option<String>,
    pub environment: Option<String>,
    pub flake_name: Option<String>,
    pub deployment_policy: String,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupWizardStepStatus {
    pub complete: bool,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupWizardProgressResponse {
    pub dismissed: bool,
    pub agent_acknowledged: bool,
    pub environment: SetupWizardStepStatus,
    pub flake: SetupWizardStepStatus,
    pub builder: SetupWizardStepStatus,
    pub cache: SetupWizardStepStatus,
    pub system: SetupWizardStepStatus,
    pub all_required_complete: bool,
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
}

impl BuilderStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Inactive => "Inactive",
            Self::Offline => "Offline",
        }
    }

    pub fn color_class(&self) -> &'static str {
        match self {
            Self::Active => "text-emerald-400",
            Self::Inactive => "text-slate-400",
            Self::Offline => "text-red-400",
        }
    }

    pub fn bg_class(&self) -> &'static str {
        match self {
            Self::Active => "bg-emerald-500/10",
            Self::Inactive => "bg-slate-500/10",
            Self::Offline => "bg-red-500/10",
        }
    }

    pub fn dot_class(&self) -> &'static str {
        match self {
            Self::Active => "bg-emerald-400",
            Self::Inactive => "bg-slate-400",
            Self::Offline => "bg-red-400",
        }
    }
}

/// Builder summary for list view
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuilderSummary {
    pub id: Uuid,
    pub name: String,
    pub status: BuilderStatus,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: i32,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub assigned_environment_count: i32,
    #[serde(default)]
    pub active_jobs: i32,
    #[serde(default)]
    pub queued_jobs: i32,
}

/// Full builder details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderDetail {
    pub id: Uuid,
    pub name: String,
    pub public_key: String,
    pub status: BuilderStatus,
    pub max_cpu_cores: Option<i32>,
    pub max_memory_mb: Option<i32>,
    pub max_concurrent_jobs: i32,
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
    pub public_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cpu_cores: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_jobs: Option<i32>,
    #[serde(default)]
    pub environment_ids: Vec<Uuid>,
}

/// Update builder request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBuilderRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BuilderStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cpu_cores: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_jobs: Option<i32>,
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
