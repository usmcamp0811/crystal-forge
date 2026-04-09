//! API data transfer objects for Crystal Forge REST endpoints.
//!
//! These DTOs define the JSON contract between server and clients (web UI, TUI, external).
//! They are intentionally decoupled from database models (`crate::models`) so the API
//! surface can evolve independently of schema migrations.
//!
//! # Naming Conventions
//! - `*Summary` — lightweight aggregate used in list views and dashboards
//! - `*Detail` — full representation used in detail views
//! - `*Count` — numeric breakdown by category

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Common Enums
// ─────────────────────────────────────────────────────────────────────────────

/// Health status derived from heartbeat recency.
///
/// Thresholds match `view_fleet_health_status`:
/// - Healthy: last seen < 15 min ago
/// - Warning: last seen < 1 hour ago
/// - Critical: last seen < 4 hours ago
/// - Offline: last seen >= 4 hours ago (or never)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Offline,
}

/// Deployment status relative to the latest available commit.
///
/// Derived from `view_system_deployment_status`.
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

/// CVE severity level derived from CVSS v3 score thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CveSeverity {
    Critical,
    High,
    Medium,
    Low,
}

/// Pipeline stage for a NixOS system's build/deploy lifecycle.
///
/// Derived from `view_nixos_pipeline_latest_with_deploy.inferred_stage`.
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
    /// Cancel requested; waiting for builder to stop the nix process.
    Cancelling,
    /// Build was cancelled (terminal).
    Cancelled,
    /// Build completed successfully.
    Complete,
    /// Build failed.
    Failed,
}

// ─────────────────────────────────────────────────────────────────────────────
// Dashboard DTOs — GET /api/v1/dashboard/summary
// ─────────────────────────────────────────────────────────────────────────────

/// Top-level dashboard response aggregating fleet-wide metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    /// Breakdown of system count by health status.
    pub fleet_health: FleetHealthSummary,

    /// Breakdown of system count by deployment status.
    pub deployment_status: DeploymentStatusSummary,

    /// Fleet-wide CVE totals by severity.
    pub cve_summary: CveSummary,

    /// Total number of registered systems.
    pub total_systems: i64,

    /// Number of active builds (in-progress derivations).
    pub active_builds: i64,

    /// Summary of builds in progress and queued.
    pub build_queue: Option<BuildQueueSummary>,

    /// Recent deployment events (newest first, capped).
    pub recent_deployments: Vec<RecentDeployment>,

    /// Server timestamp for cache-freshness.
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
    /// Total across all health categories.
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
    /// Total across all deployment categories.
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
    /// Total vulnerabilities across all severities.
    pub fn total(&self) -> i64 {
        self.critical + self.high + self.medium + self.low
    }
}

/// A single recent deployment event for the dashboard timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentDeployment {
    pub hostname: String,
    pub commit_hash: String,
    pub deployed_at: DateTime<Utc>,
    pub status: DeploymentStatus,
}

/// Summary of the build queue for the dashboard widget.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildQueueItem {
    /// Build job UUID (if sourced from build_jobs table).
    #[serde(default)]
    pub job_id: Option<uuid::Uuid>,

    /// System UUID for manual trigger actions.
    #[serde(default)]
    pub system_id: Option<uuid::Uuid>,

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

    /// Assigned builder name (if currently building/assigned).
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

/// Query parameters for the paginated build queue endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildQueueParams {
    /// Page number (1-indexed, default 1).
    #[serde(default = "default_page")]
    pub page: i64,
    /// Items per page (default 50, max 200).
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Filter by status: queued, building, success, failed (comma-separated or repeated).
    #[serde(default)]
    pub status: Option<String>,
    /// Filter by partial commit hash.
    #[serde(default)]
    pub commit_hash: Option<String>,
    /// Filter by flake/repo name (partial match).
    #[serde(default)]
    pub flake_name: Option<String>,
    /// Filter by system hostname or config name (partial match).
    #[serde(default)]
    pub config_name: Option<String>,
    /// Filter jobs queued at or after this ISO-8601 timestamp.
    #[serde(default)]
    pub queued_after: Option<DateTime<Utc>>,
    /// Filter jobs queued at or before this ISO-8601 timestamp.
    #[serde(default)]
    pub queued_before: Option<DateTime<Utc>>,
}

fn default_page() -> i64 {
    1
}
fn default_limit() -> i64 {
    50
}

/// Paginated response for the build queue listing endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildQueuePageResponse {
    /// Total number of matching rows across all pages.
    pub total: i64,
    /// Current page (1-indexed).
    pub page: i64,
    /// Items per page.
    pub limit: i64,
    /// Items on this page.
    pub items: Vec<BuildQueueItem>,
}

/// Summary of the evaluation queue for the evaluations view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQueueSummary {
    pub active_count: i64,
    pub completed_count: i64,
    pub execution_mode: String,
    pub items: Vec<EvalQueueItem>,
    pub timestamp: DateTime<Utc>,
}

/// A single commit item in the evaluation queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Request payload for persisting eval queue order from the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderEvalQueueRequest {
    pub ordered_commit_ids: Vec<i32>,
}

// ─────────────────────────────────────────────────────────────────────────────
// System DTOs — GET /api/v1/systems, GET /api/v1/systems/:id
// ─────────────────────────────────────────────────────────────────────────────

/// Lightweight system representation for list views.
///
/// Contains just enough data to render a table row or card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSummary {
    pub id: Uuid,
    pub hostname: String,
    pub system_configuration_name: Option<String>,
    pub environment: Option<String>,
    pub flake_id: Option<i32>,
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
    /// Core identity.
    pub id: Uuid,
    pub hostname: String,
    pub system_configuration_name: Option<String>,
    pub environment: Option<String>,
    pub is_active: bool,
    pub deployment_policy: String,

    /// Status indicators.
    pub health_status: HealthStatus,
    pub deployment_status: DeploymentStatus,
    pub pipeline_stage: Option<PipelineStage>,

    /// Software versions.
    pub nixos_version: Option<String>,
    pub kernel: Option<String>,
    pub agent_version: Option<String>,
    pub current_store_path: Option<String>,

    /// Hardware information.
    pub hardware: SystemHardwareInfo,

    /// Network information.
    pub network: SystemNetworkInfo,

    /// Security posture.
    pub security: SystemSecurityInfo,

    /// CVE vulnerability counts.
    pub cve_counts: CveSummary,

    /// Flake and commit context.
    pub flake: Option<FlakeSummary>,

    /// Timestamps.
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

/// Flake registry item for flakes management view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeRegistryItem {
    pub id: i32,
    pub name: String,
    pub repo_url: String,
    pub branch: String,
    pub build_scope: String,
    pub system_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeCredentialSummary {
    pub flake_id: i32,
    pub auth_type: String,
    pub username: Option<String>,
    pub ssh_username: Option<String>,
    pub has_secret: bool,
}

/// Environment summary for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSummary {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
    pub is_active: bool,
    /// Number of systems assigned to this environment.
    pub system_count: i64,
}

/// Request payload for creating an environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEnvironmentRequest {
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
    pub is_active: bool,
}

/// Request payload for updating an environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEnvironmentRequest {
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
}

/// Request payload for updating environment required policies only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEnvironmentPoliciesRequest {
    pub required_policy_ids: Vec<uuid::Uuid>,
}

/// A deployment policy available in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentPolicySummary {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
}

/// Environment with its required policies (the baseline).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentWithPolicies {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
    pub is_active: bool,
    pub system_count: i64,
    /// The required policy IDs that form the baseline for this environment.
    pub required_policy_ids: Vec<uuid::Uuid>,
}

/// Bulk mapping of environment IDs to required policies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentPolicyMapEntry {
    pub environment_id: uuid::Uuid,
    pub required_policy_ids: Vec<uuid::Uuid>,
}

/// Request payload for creating a flake registry entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFlakeRequest {
    pub name: String,
    pub repo_url: String,
    pub branch: Option<String>,
    pub build_scope: Option<String>,
}

/// Request payload for updating a flake registry entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateFlakeRequest {
    pub name: String,
    pub repo_url: String,
    pub branch: Option<String>,
    pub build_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFlakeCredentialRequest {
    pub auth_type: String,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub ssh_username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateFlakeCredentialRequest {
    pub auth_type: Option<String>,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub ssh_username: Option<String>,
}

/// A flake with its commit timeline for the dashboard widget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeTimeline {
    pub flake_id: i32,
    pub flake_name: String,
    pub repo_url: String,
    pub commits: Vec<FlakeCommit>,
}

/// A commit in a flake timeline with deployment status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeCommit {
    pub id: i32,
    pub hash: String,
    pub message: String,
    pub author: String,
    pub committed_at: DateTime<Utc>,
    pub system_count: i64,
    pub commits_behind: i64,
    /// nixosConfigurations discovered at this commit.
    ///
    /// Entries may include the suffix " [CF system]" when the configuration
    /// name matches a Crystal Forge system deployed at this commit.
    pub systems: Vec<String>,
    /// Per-configuration path details shown in commit details.
    #[serde(default)]
    pub system_paths: Vec<FlakeCommitSystemPath>,
    pub build_status: Option<BuildStatus>,
    #[serde(default)]
    pub evaluation_status: Option<String>,
    /// Error message if evaluation failed.
    #[serde(default)]
    pub evaluation_error_message: Option<String>,
    /// Cached evaluation metadata (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<CommitMetadata>,
}

/// Path details for a single nixosConfiguration at a commit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeCommitSystemPath {
    /// Configuration name under nixosConfigurations.
    pub config_name: String,
    /// True when this config maps to an active Crystal Forge system.
    #[serde(default)]
    pub is_cf_system: bool,
    /// Hostname of the mapped Crystal Forge system, when available.
    #[serde(default)]
    pub cf_hostname: Option<String>,
    /// Number of active Crystal Forge systems mapped to this config.
    #[serde(default)]
    pub mapped_host_count: i64,
    /// Store path associated with this config for the commit, when available.
    #[serde(default)]
    pub expected_store_path: Option<String>,
    /// Latest agent-reported current store path for mapped system, when available.
    #[serde(default)]
    pub current_store_path: Option<String>,
}

/// Cached evaluation metadata for fast UI rendering
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitMetadata {
    pub total_systems: i32,
    pub systems_passed_policy: i32,
    pub systems_failed_policy_strict: i32,
    pub systems_failed_policy_non_strict: i32,
    pub has_nix_eval_error: bool,
    pub has_policy_failures: bool,
    pub all_systems_passed: bool,
}

/// Response containing the git diff for a specific commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDiffResponse {
    pub commit_hash: String,
    pub diff: String,
}

/// Request payload for creating a new system.
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

/// Request payload for updating a system's public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSystemPublicKeyRequest {
    pub public_key: String,
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
    /// Number of pages given the current `per_page`.
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

/// Request payload for rolling a system back to a specific commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRollbackRequest {
    pub target_commit: String,
}

/// Request payload for deploying a system with a specific commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploySystemRequest {
    pub commit_sha: String,
}

/// Response containing available commits for deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCommitsResponse {
    pub commits: Vec<CommitInfo>,
    pub current_commit: Option<String>,
}

/// Information about a commit available for deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: String,
}

/// Generic response for accepted system mutation actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMutationResponse {
    pub status: String,
    pub message: String,
}

/// Sort direction for list queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Asc,
    Desc,
}

// ─────────────────────────────────────────────────────────────────────────────
// Authentication Context DTOs — GET /api/auth/whoami
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub actor: Option<String>,
    pub action: AuditAction,
    pub target: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcGroupMapping {
    pub id: String,
    pub group_name: String,
    pub role: Option<Role>,
    pub environments: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupWizardStepStatus {
    pub complete: bool,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupWizardDismissRequest {
    pub dismissed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupWizardAcknowledgeAgentRequest {
    pub acknowledged: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Error Response
// ─────────────────────────────────────────────────────────────────────────────

/// Standard error envelope for all API error responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// Machine-readable error code (e.g., "not_found", "validation_error").
    pub error: String,
    /// Human-readable error message.
    pub message: String,
    /// Optional additional context or structured validation errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fleet_health_summary_total() {
        let summary = FleetHealthSummary {
            healthy: 10,
            warning: 3,
            critical: 1,
            offline: 2,
        };
        assert_eq!(summary.total(), 16);
    }

    #[test]
    fn deployment_status_summary_total() {
        let summary = DeploymentStatusSummary {
            up_to_date: 8,
            behind: 4,
            never_deployed: 2,
            unknown: 1,
        };
        assert_eq!(summary.total(), 15);
    }

    #[test]
    fn cve_summary_total() {
        let summary = CveSummary {
            critical: 2,
            high: 5,
            medium: 12,
            low: 30,
        };
        assert_eq!(summary.total(), 49);
    }

    #[test]
    fn paginated_response_total_pages() {
        let resp: PaginatedResponse<()> = PaginatedResponse {
            items: vec![],
            total: 25,
            page: 1,
            per_page: 10,
        };
        assert_eq!(resp.total_pages(), 3);
    }

    #[test]
    fn paginated_response_total_pages_exact() {
        let resp: PaginatedResponse<()> = PaginatedResponse {
            items: vec![],
            total: 20,
            page: 1,
            per_page: 10,
        };
        assert_eq!(resp.total_pages(), 2);
    }

    #[test]
    fn paginated_response_total_pages_zero_per_page() {
        let resp: PaginatedResponse<()> = PaginatedResponse {
            items: vec![],
            total: 20,
            page: 1,
            per_page: 0,
        };
        assert_eq!(resp.total_pages(), 0);
    }

    #[test]
    fn health_status_serializes_snake_case() {
        let json = serde_json::to_string(&HealthStatus::Healthy).unwrap();
        assert_eq!(json, r#""healthy""#);
    }

    #[test]
    fn health_status_deserializes_snake_case() {
        let status: HealthStatus = serde_json::from_str(r#""critical""#).unwrap();
        assert_eq!(status, HealthStatus::Critical);
    }

    #[test]
    fn deployment_status_serializes_snake_case() {
        let json = serde_json::to_string(&DeploymentStatus::UpToDate).unwrap();
        assert_eq!(json, r#""up_to_date""#);
    }

    #[test]
    fn cve_severity_serializes_snake_case() {
        let json = serde_json::to_string(&CveSeverity::Critical).unwrap();
        assert_eq!(json, r#""critical""#);
    }

    #[test]
    fn pipeline_stage_round_trips() {
        let stage = PipelineStage::ReadyForDeploy;
        let json = serde_json::to_string(&stage).unwrap();
        let back: PipelineStage = serde_json::from_str(&json).unwrap();
        assert_eq!(stage, back);
    }

    #[test]
    fn dashboard_summary_serializes() {
        let summary = DashboardSummary {
            fleet_health: FleetHealthSummary {
                healthy: 5,
                warning: 1,
                critical: 0,
                offline: 0,
            },
            deployment_status: DeploymentStatusSummary {
                up_to_date: 4,
                behind: 1,
                never_deployed: 1,
                unknown: 0,
            },
            cve_summary: CveSummary {
                critical: 0,
                high: 2,
                medium: 8,
                low: 15,
            },
            total_systems: 6,
            active_builds: 1,
            build_queue: Some(BuildQueueSummary {
                building_count: 1,
                queued_count: 0,
                items: vec![],
                timestamp: Utc::now(),
            }),
            recent_deployments: vec![],
            timestamp: Utc::now(),
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["total_systems"], 6);
        assert_eq!(json["fleet_health"]["healthy"], 5);
        assert_eq!(json["cve_summary"]["total"], serde_json::Value::Null);
        // total() is a method, not serialized — verify the method works
        assert_eq!(summary.cve_summary.total(), 25);
    }

    #[test]
    fn api_error_omits_none_details() {
        let err = ApiError {
            error: "not_found".into(),
            message: "System not found".into(),
            details: None,
        };
        let json = serde_json::to_value(&err).unwrap();
        assert!(!json.as_object().unwrap().contains_key("details"));
    }

    #[test]
    fn api_error_includes_details_when_present() {
        let err = ApiError {
            error: "validation_error".into(),
            message: "Invalid parameters".into(),
            details: Some(serde_json::json!({"field": "hostname"})),
        };
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["details"]["field"], "hostname");
    }

    #[test]
    fn systems_list_params_default() {
        let params = SystemsListParams::default();
        assert!(params.page.is_none());
        assert!(params.search.is_none());
        assert!(params.health_status.is_none());
    }

    #[test]
    fn sort_order_serializes() {
        assert_eq!(serde_json::to_string(&SortOrder::Asc).unwrap(), r#""asc""#);
        assert_eq!(
            serde_json::to_string(&SortOrder::Desc).unwrap(),
            r#""desc""#
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Admin Config Health DTOs — GET /api/v1/admin/config-health
// ─────────────────────────────────────────────────────────────────────────────

/// A single pipeline readiness check with its result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigHealthCheck {
    /// Stable identifier for this check (e.g. `"no_flakes"`).
    pub id: String,
    /// Whether this check passed (no issue detected).
    pub passed: bool,
    /// Human-readable description shown to the admin when the check fails.
    pub message: String,
    /// URL path the admin can navigate to in order to resolve the issue.
    pub action_url: String,
}

/// Top-level response for `GET /api/v1/admin/config-health`.
///
/// Aggregates all pipeline readiness checks into a single response so the
/// UI can display a health overview without multiple round-trips.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigHealthResponse {
    /// `true` if at least one flake is configured.
    pub has_flakes: bool,
    /// `true` if at least one environment exists.
    pub has_environments: bool,
    /// `true` if at least one builder is registered.
    pub has_builders: bool,
    /// `true` if at least one cache destination is configured.
    pub has_cache_destinations: bool,
    /// Total number of failing checks (`checks` entries where `passed == false`).
    pub total_issues: u32,
    /// Per-check details for all pipeline readiness checks.
    pub checks: Vec<ConfigHealthCheck>,
}
