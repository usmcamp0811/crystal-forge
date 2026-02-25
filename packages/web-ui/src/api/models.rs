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
    pub environment: Option<String>,
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
    pub system_count: i64,
}

/// Request payload for creating a flake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateFlakeRequest {
    pub name: String,
    pub repo_url: String,
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
    /// Build completed successfully.
    Complete,
    /// Build failed.
    Failed,
}

impl BuildStatus {
    /// Returns true if this status represents an active build (queued or building).
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Queued | Self::Building)
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Queued => "Queued",
            Self::Building => "Building",
            Self::Complete => "Complete",
            Self::Failed => "Failed",
        }
    }

    /// CSS color class for the status.
    pub fn color_class(&self) -> &'static str {
        match self {
            Self::Idle => "text-gray-400",
            Self::Queued => "text-blue-400",
            Self::Building => "text-cyan-400",
            Self::Complete => "text-emerald-400",
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
    /// Hostnames of systems at this commit (for tooltip/expansion).
    pub systems: Vec<String>,
    /// Current build status for this commit (if any build is in progress).
    #[serde(default)]
    pub build_status: Option<BuildStatus>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Build Queue DTOs
// ─────────────────────────────────────────────────────────────────────────────

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
    /// When the build was queued.
    pub queued_at: DateTime<Utc>,
    /// When the build started (None if still queued).
    pub started_at: Option<DateTime<Utc>>,
    /// Elapsed time in seconds since started (for display).
    #[serde(default)]
    pub elapsed_secs: Option<i64>,
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
pub struct SystemMutationResponse {
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSystemRequest {
    pub hostname: String,
    pub public_key: String,
    pub environment: Option<String>,
    pub flake_name: Option<String>,
    pub deployment_policy: String,
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
    /// When this CVE was published.
    pub published_at: Option<DateTime<Utc>>,
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
