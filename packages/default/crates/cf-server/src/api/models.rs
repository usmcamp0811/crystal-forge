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
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Tri-state value for PATCH-style update payloads.
///
/// JSON cannot, with a plain `Option<T>`, distinguish "field omitted" from
/// "field explicitly null". For update endpoints that means an older/partial
/// client omitting a field would be indistinguishable from a request asking to
/// clear it — silently wiping persisted data.
///
/// `FieldUpdate` makes the three cases explicit:
/// - field omitted        → [`FieldUpdate::Unset`]   (preserve existing value)
/// - field present as null → [`FieldUpdate::Clear`]   (set to NULL)
/// - field present + value → [`FieldUpdate::Set`]     (write the value)
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

/// Admin-only CVE dashboard fleet summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveDashboardSummary {
    pub total_open: i64,
    pub severity: CveSummary,
    pub affected_systems: i64,
    pub new_cves_last_7_days: i64,
    pub oldest_cve_age_days: Option<i64>,
}

/// Top-affected system entry for the CVE dashboard visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveDashboardTopSystem {
    pub system_id: Uuid,
    pub hostname: String,
    pub total_cves: i64,
    pub critical_cves: i64,
    pub high_cves: i64,
    pub medium_cves: i64,
    pub low_cves: i64,
    /// Days since the last CVE scan (None if never scanned).
    pub days_since_scan: Option<i64>,
    /// Timestamp of the last completed scan.
    pub last_cve_scan: Option<DateTime<Utc>>,
}

/// Scan freshness/coverage row per system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveScanFreshnessRow {
    pub system_id: Uuid,
    pub hostname: String,
    /// Days since last scan, or None if never scanned.
    pub days_since_scan: Option<i64>,
    pub last_cve_scan: Option<DateTime<Utc>>,
    /// Total CVEs found in the last scan.
    pub total_cves: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSchedulePolicyResponse {
    pub on_build: bool,
    pub deployed_interval: String,
    pub recent_interval: String,
    pub archived_interval: String,
    pub archived_enabled: bool,
    pub rebuild_to_scan: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateScanSchedulePolicyRequest {
    pub on_build: bool,
    pub deployed_interval: String,
    pub recent_interval: String,
    pub archived_interval: String,
    pub archived_enabled: bool,
    pub rebuild_to_scan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanningStatsResponse {
    pub scanning: i64,
    pub queued: i64,
    pub stale: i64,
    pub never_scanned: i64,
    pub failed: i64,
    pub coverage_percent: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub freshness: String,
    /// True when this is the latest scan row for its derivation.
    pub is_current: bool,
    /// True when this derivation's commit is the latest known commit for its flake.
    #[serde(default)]
    pub is_latest_per_flake: bool,
    /// What triggered the scan. Not yet tracked in the schema; always `None`
    /// until a trigger source column is added (tracked as follow-up).
    pub trigger: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanningActivityItemResponse {
    pub at: Option<DateTime<Utc>>,
    pub name: String,
    pub event: String,
    pub detail: String,
    pub status: String,
}

/// A single CVE row for dashboard drill-down views.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// CVE entry for a specific system drill-down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemVulnerability {
    pub cve_id: String,
    pub severity: CveSeverity,
    pub cvss_score: Option<f64>,
    pub description: String,
    pub package_name: String,
    pub installed_version: String,
    pub fixed_version: Option<String>,
    pub first_seen: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub status: String,
    #[serde(default)]
    pub justification_category: Option<String>,
    #[serde(default)]
    pub justification_reason: Option<String>,
    #[serde(default)]
    pub justification_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSystemCveJustificationRequest {
    pub category: Option<String>,
    pub reason: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Advanced CVE Dashboard DTOs (TASK-322)
// ─────────────────────────────────────────────────────────────────────────────

/// Filter parameters for CVE list queries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CveFilters {
    pub severity: Option<String>,      // "critical", "high", "medium", "low"
    pub fix_status: Option<String>,    // "available", "pending", "exploited"
    pub triage_status: Option<String>, // "outstanding", "scheduled", "accepted"
    pub package: Option<String>,       // Package name substring match
    pub search: Option<String>,        // Search across CVE ID, package, title
    pub sort: Option<String>,          // "severity", "cvss", "age", "affected"
    pub limit: Option<i64>,            // Max results (default 500, max 1000)
}

/// CVE list item for table views.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
    #[sqlx(skip)]
    pub cves: Option<Vec<CveListItem>>,
}

/// Detailed CVE information for the drawer view.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveJustificationInput {
    pub system_id: Option<Uuid>, // None = fleet-wide justification
    pub cve_id: String,
    pub category: String, // "mitigated", "false_positive", "accepted_risk", "patch_scheduled"
    pub reason: String,   // 10-2000 characters
    pub updated_by: Uuid, // User ID from auth context
}

/// Fleet-wide CVE statistics.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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

    /// Stable flake identity used for latest-per-flake grouping.
    #[serde(default)]
    pub flake_id: Option<i32>,

    /// True when this is the newest item in its active/history domain for its flake.
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

    /// Assigned builder name (if currently building/assigned).
    #[serde(default)]
    pub builder_name: Option<String>,
    /// When the build was queued.
    pub queued_at: DateTime<Utc>,
    #[serde(default = "default_attempt_number")]
    pub attempt_number: i32,
    #[serde(default)]
    pub parent_job_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub root_job_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub available_at: Option<DateTime<Utc>>,
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
    /// Total derivations for this system config at this commit (for progress bar).
    #[serde(default)]
    pub total_derivs: i64,
    /// Derivations that have completed a build (build-complete or complete status).
    #[serde(default)]
    pub built_derivs: i64,
    /// Derivations that have been pushed to cache (cache-pushed status).
    #[serde(default)]
    pub cached_derivs: i64,
}

fn default_attempt_number() -> i32 {
    1
}

/// Query parameters for the paginated build queue endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildQueueParams {
    /// Page number (1-indexed, default 1).
    #[serde(default = "default_page")]
    pub page: i64,
    /// Items per page (default 50).
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
    /// Search system/config, flake, commit, builder, architecture, or status.
    #[serde(default)]
    pub search: Option<String>,
    /// Return only the authoritative latest item for each stable flake.
    #[serde(default)]
    pub latest_only: bool,
}

/// Hard upper bound for per-request limit parameters.
///
/// Prevents a viewer from requesting an unbounded result set (e.g.
/// `limit=9223372036854775807`) that would cause the server to query and
/// serialize the entire matching dataset, risking memory exhaustion and
/// long-running queries. All paginated and list endpoints that accept a
/// `limit` query parameter clamp to this value.
///
/// Set to 10 000, which is well beyond any realistic on-screen viewport
/// (infinite scroll loads 50–200 per page) while still providing headroom
/// for bulk export or script usage.
pub const LIMIT_MAX: i64 = 10_000;

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
    /// Total rows in the active/history domain before search and filters.
    #[serde(default)]
    pub domain_total: i64,
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
    pub failed_count: i64,
    /// Active evaluation rows before search and filters.
    #[serde(default)]
    pub domain_total: i64,
    /// Active evaluation rows after all search/filter/latest predicates.
    #[serde(default)]
    pub filtered_total: i64,
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
    #[serde(default = "default_attempt_number")]
    pub attempt_number: i32,
    #[serde(default)]
    pub parent_attempt_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub root_attempt_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub available_at: Option<DateTime<Utc>>,
}

/// Request payload for persisting eval queue order from the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderEvalQueueRequest {
    pub ordered_commit_ids: Vec<i32>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Eval cancellation + history DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Outcome returned by the cancel-evaluation endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CancelEvalOutcome {
    /// Commit was `pending` and is now immediately `cancelled`.
    Cancelled,
    /// Commit was `in_progress`; transitioned to `cancelling`.
    /// The eval loop will kill the subprocess and finalise to `cancelled`.
    CancellingInProgress,
    /// No commit with that ID was found, or it was already terminal.
    NotFound,
    /// Commit was already in a terminal state (`complete`, `failed`, `cancelled`).
    AlreadyTerminal,
}

/// A single row in the eval history list.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// When the evaluation finished (complete, failed, or cancelled).
    pub evaluation_completed_at: Option<DateTime<Utc>>,
    /// Elapsed milliseconds from started_at to completed_at.
    pub evaluation_duration_ms: Option<i64>,
    pub evaluation_error_message: Option<String>,
    pub system_count: i64,
    pub passed_count: i64,
    pub policy_failed_count: i64,
    pub eval_failed_count: i64,
    /// Unique identifier for this evaluation occurrence, including both commit
    /// and completion timestamp. Used for alert acknowledgement to distinguish
    /// between separate evaluation attempts of the same commit (review finding).
    pub alert_occurrence_id: String,
    #[serde(default = "default_attempt_number")]
    pub attempt_number: i32,
    #[serde(default)]
    pub parent_attempt_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub root_attempt_id: Option<uuid::Uuid>,
}

/// Paginated response for GET /api/v1/commits/eval-history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalHistoryPage {
    pub total_count: i64,
    /// Total terminal evaluations before search and filters.
    #[serde(default)]
    pub domain_total: i64,
    pub page: i64,
    pub limit: i64,
    pub items: Vec<EvalHistoryItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalQueueParams {
    #[serde(default = "default_eval_queue_limit")]
    pub limit: i64,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub flake: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub latest_only: bool,
}

fn default_eval_queue_limit() -> i64 {
    200
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalHistoryParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub flake: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub latest_only: bool,
}

/// Per-system policy matrix for a single commit evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPolicyMatrixResponse {
    pub commit_id: i32,
    pub policies: Vec<String>,
    pub systems: Vec<EvalPolicySystemRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPolicySystemRow {
    pub system_name: String,
    /// One entry per policy in `EvalPolicyMatrixResponse.policies`.
    pub results: Vec<String>,
    /// Parallel to `results` — per-policy detail/evidence. Empty vec or `None`
    /// entries mean no detail is available for that result.
    #[serde(default)]
    pub details: Vec<Option<String>>,
}

/// Dependency/derivation breakdown for a single commit evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalDependencyGraphResponse {
    pub commit_id: i32,
    pub total_packages: i64,
    pub packages: Vec<EvalDependencyPackageRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalDependencyPackageRow {
    pub package_name: String,
    /// True when ready/pending counts represent real closure package counts.
    /// False means counts are only a temporary system-status fallback.
    pub closure_counted: bool,
    /// Systems with a completed build (store_path present / BuildComplete).
    pub ready_count: i64,
    /// Systems evaluated but not yet built (DryRunComplete / pending build).
    pub pending_count: i64,
    /// Systems whose eval or build failed.
    pub failed_count: i64,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemDetail {
    /// Core identity.
    pub id: Uuid,
    pub hostname: String,
    #[serde(default)]
    pub fqdn: Option<String>,
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
    #[serde(default)]
    pub generation: Option<i32>,
    #[serde(default)]
    pub generation_matches_current_store_path: Option<bool>,

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
    /// Authoritative restart classification written by the server on each startup heartbeat.
    /// Values: "system_reboot", "agent_restart", "unknown". None = no startup event processed yet.
    #[serde(default)]
    pub restart_type: Option<String>,
    /// Timestamp of the heartbeat that triggered the last restart classification.
    #[serde(default)]
    pub last_restart_at: Option<DateTime<Utc>>,
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
    #[serde(default = "default_system_reachability")]
    pub reachability: String,
}

fn default_system_reachability() -> String {
    "direct".to_string()
}

fn default_effective_heartbeat_interval_secs() -> i32 {
    600
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
///
/// Enriched with latest commit summary and environment names so the initial
/// Flakes view can render from a single registry response without fetching
/// timelines or the full systems list (TASK-397).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeRegistryItem {
    pub id: i32,
    pub name: String,
    pub repo_url: String,
    pub branch: String,
    pub build_scope: String,
    pub system_count: i64,
    /// Current sync state: "unknown" | "synced" | "syncing" | "error"
    pub sync_status: String,
    /// Timestamp of the most recent sync attempt (success or failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<DateTime<Utc>>,
    /// The error text from the most recent failed sync, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_error: Option<String>,

    // ----- Enriched fields (TASK-397) -----
    /// Hash of the latest commit visible on the tracked branch (from snapshot
    /// or commit table). `None` if the flake has no commits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_commit_hash: Option<String>,
    /// Commit message of the latest visible commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_commit_message: Option<String>,
    /// Author of the latest visible commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_commit_author: Option<String>,
    /// Timestamp of the latest visible commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_commit_timestamp: Option<DateTime<Utc>>,
    /// Aggregate build status of the latest visible commit.
    /// One of: "building", "queued", "failed", "complete", or `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_status: Option<String>,
    /// Evaluation status of the latest visible commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation_status: Option<String>,
    /// Sorted, deduplicated environment names associated with this flake's
    /// active systems. Empty array if no systems exist.
    #[serde(default)]
    pub environments: Vec<String>,
    /// Total number of commits currently visible on the tracked branch.
    /// When a branch snapshot exists, this is the snapshot row count.
    /// Otherwise it is the count of commits in the commits table.
    pub total_commit_count: i64,
}

/// Navigation badge aggregate — counts of items needing attention per view,
/// computed relative to the requesting user's last acknowledgment of each
/// category (see `queries::navigation`), not raw totals. Returned by
/// GET /api/v1/navigation/badges; polled by the sidebar every 30s.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NavigationBadges {
    /// Server-side timestamp captured with `NOW()` at the moment this response
    /// was computed. Clients MUST echo this value back as `observed_at` in the
    /// POST /navigation/acknowledge body so the acknowledgment baseline is
    /// anchored to exactly what the client was shown, not to the (later) time
    /// the POST was received. This prevents a failure that arrives after the
    /// badge response but before the user clicks acknowledge from being silently
    /// consumed.
    #[serde(default)]
    pub observed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Systems whose health is "critical" or "offline" — current total shown
    /// only if the count or alerting-ID set changed since last acknowledgment.
    pub systems_attention: i64,
    pub systems_total: i64,
    /// MD5 of the sorted set of alerting system IDs at query time. Clients
    /// must echo this in the acknowledge body so replacement failures (same
    /// count, different alerting IDs) resurface after acknowledgment.
    #[serde(default)]
    pub systems_fingerprint: Option<String>,
    /// Flakes whose sync_status is "error" and last_sync_at is newer than the
    /// user's last acknowledgment of the flakes category.
    pub flakes_errored: i64,
    pub flakes_total: i64,
    /// Environments containing ≥1 attention system — current total shown only
    /// if the count or alerting-ID set changed since last acknowledgment.
    pub environments_attention: i64,
    pub environments_total: i64,
    /// MD5 of the sorted set of alerting environment IDs at query time.
    #[serde(default)]
    pub environments_fingerprint: Option<String>,
    /// Build jobs that failed and completed after the user's last
    /// acknowledgment of the builds category.
    pub builds_failed_new: i64,
    /// Commit evaluations that failed and completed after the user's last
    /// acknowledgment of the evals category.
    pub evals_failed_new: i64,
    /// Critical CVEs first detected after the user's last acknowledgment of
    /// the cves category.
    pub cves_critical_new: i64,
    /// Server canonical occurrence IDs that the current user can dismiss for
    /// each category. Empty when no eligible occurrences are present.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeCredentialSummary {
    pub flake_id: i32,
    pub auth_type: String,
    pub username: Option<String>,
    pub ssh_username: Option<String>,
    pub has_secret: bool,
}

/// Per-environment system health + risk rollup.
///
/// Derived from the `view_environment_rollups` view (TASK-358). All counts are
/// authoritative and computed from the active systems assigned to the
/// environment, mirroring the Systems surface health thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvironmentRollup {
    /// Active systems included in this health rollup.
    pub active_system_count: i64,
    /// Systems in the `healthy` state.
    pub healthy: i64,
    /// Systems in the `warning` state.
    pub warning: i64,
    /// Systems in the `critical` state.
    pub critical: i64,
    /// Systems in the `offline` state.
    pub offline: i64,
    /// Distinct CRITICAL+HIGH CVEs across this environment's active systems.
    pub cve_critical_high: i64,
    /// Names of the flakes spanning this environment's systems.
    pub flakes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentCacheSummary {
    pub name: String,
    pub url: String,
    pub cache_type: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentComplianceSummary {
    pub id: uuid::Uuid,
    pub name: String,
    pub framework: String,
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
    /// Per-environment health breakdown, CVE totals, and flakes.
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
}

/// Request payload for creating an environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Request payload for updating an environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ─────────────────────────────────────────────────────────────────────────────
// Compliance DTOs — /api/v1/compliance/*
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceBundleSummary {
    pub id: uuid::Uuid,
    pub name: String,
    pub framework: String,
    pub version: String,
    pub description: Option<String>,
    pub layer: String,
    pub owner: String,
    pub last_review: Option<DateTime<Utc>>,
    pub policy_ids: Vec<uuid::Uuid>,
    pub required_envs: Vec<ComplianceEnvironmentRef>,
    pub control_count: i64,
    pub environment_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceEnvironmentRef {
    pub id: uuid::Uuid,
    pub name: String,
    pub color_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceBundleSystemsResponse {
    pub bundle_id: uuid::Uuid,
    pub systems: Vec<ComplianceSystemRollup>,
    pub totals: ComplianceRollupTotals,
}

/// Response for GET /api/v1/systems/:system_id/compliance
/// Returns bundles applicable to the system with their rollups.
///
/// This endpoint is all-or-nothing: infrastructure failures (database errors,
/// missing policies) fail the entire request. Individual bundle computation
/// uses deterministic logic with no fallible operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemComplianceBundlesResponse {
    pub system_id: uuid::Uuid,
    pub bundles: Vec<SystemComplianceBundle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemComplianceBundle {
    pub bundle: ComplianceBundleSummary,
    pub rollup: ComplianceSystemRollup,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComplianceRollupTotals {
    pub system_count: i64,
    pub fully_compliant_count: i64,
    pub pass: i64,
    pub warn: i64,
    pub fail: i64,
    pub waiver: i64,
    /// Total policy slots across all systems (includes disabled/unsupported).
    pub total_controls: i64,
    /// Controls that were actually evaluated (excludes disabled/unsupported).
    /// Used as the denominator for overall_score.
    #[serde(default)]
    pub evaluated_controls: i64,
    pub overall_score: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceSystemRollup {
    pub system_id: uuid::Uuid,
    pub hostname: String,
    pub environment: Option<String>,
    pub applies: bool,
    /// Total policy slots in the bundle (includes disabled/unsupported).
    pub total: i64,
    /// Controls that were actually evaluated for this system.
    /// Use this as the score denominator; total includes non-evaluated policies.
    #[serde(default)]
    pub evaluated_total: i64,
    pub pass: i64,
    pub warn: i64,
    pub fail: i64,
    pub waiver: i64,
    pub score: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceControlStatus {
    Pass,
    Warn,
    Fail,
    Waiver,
    /// Control is selected but no applicable evidence was found.
    NotChecked,
    /// Control does not apply to this target.
    NotApplicable,
    /// Evaluator encountered an error attempting to assess the control.
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceEvidenceResponse {
    pub bundle_id: uuid::Uuid,
    pub system_id: uuid::Uuid,
    pub hostname: String,
    pub controls: Vec<ComplianceControlEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceControlEvidence {
    pub policy_id: uuid::Uuid,
    pub policy_name: String,
    pub status: ComplianceControlStatus,
    pub severity: String,
    pub summary: String,
    pub evidence_items: Vec<ComplianceEvidenceItem>,
    pub framework_mapping: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceEvidenceItem {
    pub kind: String,
    pub label: String,
    pub body: String,
    pub artifact: Option<ComplianceEvidenceArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceEvidenceArtifact {
    pub artifact_type: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateComplianceBundleRequest {
    pub name: String,
    pub framework: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub layer: Option<String>,
    pub required_envs: Vec<uuid::Uuid>,
    pub policy_ids: Vec<uuid::Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateComplianceBundleRequest {
    pub name: String,
    pub framework: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub required_envs: Vec<uuid::Uuid>,
    pub policy_ids: Vec<uuid::Uuid>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFlakeCredentialRequest {
    pub repo_url: Option<String>,
    pub branch: Option<String>,
    pub auth_type: String,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub ssh_username: Option<String>,
    pub use_stored_secret_if_empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFlakeCredentialResponse {
    pub ok: bool,
    pub message: String,
    pub branch: String,
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
    /// Whether this config can trigger an immediate CVE scan.
    #[serde(default)]
    pub cve_scan_eligible: bool,
    /// Why immediate CVE scan is blocked (when ineligible).
    #[serde(default)]
    pub cve_scan_blocked_reason: Option<String>,
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
    /// Tri-state FQDN update. Omitting the key preserves the persisted FQDN;
    /// sending `null` (or an empty string, normalized server-side) clears it.
    #[serde(default)]
    pub fqdn: FieldUpdate<String>,
    pub system_configuration_name: Option<String>,
    pub environment: Option<String>,
    pub flake_name: Option<String>,
    pub deployment_policy: String,
    /// Tri-state heartbeat interval in seconds. Omitting the key preserves the persisted value;
    /// sending `null` clears it (falls back to server default of 600s); sending a value sets it.
    /// Valid range: 15-900 seconds.
    ///
    /// `skip_serializing_if` is required: without it, `Unset` serializes as `null`,
    /// which a receiving server interprets as `Clear` and wipes the stored override.
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unset")]
    pub heartbeat_interval_secs: FieldUpdate<i32>,
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

/// Request payload for rolling a system back to a specific deployed store path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRollbackGenerationRequest {
    pub store_path: String,
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

/// Response containing available generations for rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemGenerationsResponse {
    pub generations: Vec<SystemGeneration>,
    pub current_generation: Option<i32>,
}

/// Information about a NixOS generation available for rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemGeneration {
    pub generation: i32,
    pub store_path: Option<String>,
    pub commit_hash: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyGenerationClosureRequest {
    pub store_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyGenerationClosureResponse {
    pub available: bool,
    pub message: String,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemDeploymentProgress {
    pub id: uuid::Uuid,
    pub stage: String,
    pub kind: String,
    pub target_store_path: String,
    pub target_commit: Option<String>,
    pub target_generation: Option<i64>,
    pub source: String,
    pub issued_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub applying_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub failed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub failure_message: Option<String>,
}

/// A single system state transition for timeline/history views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHistoryEntry {
    #[serde(default)]
    pub id: Option<uuid::Uuid>,
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
    pub event_kind: String,
    /// Recorded generation number at this transition (from `system_states.generation`).
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
    pub deployment_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub desired_target_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub correlation_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Whether the running store path at this transition maps to a tracked flake commit.
    /// For out-of-band local rebuilds this distinguishes "reconciled" from
    /// "untracked / capture-to-flake".
    pub reconciled: bool,
    /// Whether this recorded generation's store path matched the current store path.
    pub generation_matches_current_store_path: Option<bool>,
    /// Per-event restart classification: "system_reboot", "agent_restart", "unknown", or None.
    /// Populated for startup transitions only; None for other event kinds.
    #[serde(default)]
    pub restart_type: Option<String>,
}

/// Agent-originated event shown on system details logs tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemAgentEvent {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub event_type: String,
    pub message: String,
    pub deployment_related: bool,
}

/// Generic response for accepted system mutation actions.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardeningTopServiceResponse {
    pub service_name: String,
    pub affected_systems_count: i64,
    pub avg_score: f64,
    pub min_score: i32,
    pub max_score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveHardeningJustificationRequest {
    pub directive_name: Option<String>,
    pub category: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiThemePreference {
    Dark,
    Light,
}

impl UiThemePreference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiDensityPreference {
    Comfortable,
    Compact,
}

impl UiDensityPreference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SystemsViewPreference {
    Cards,
    Table,
}

impl SystemsViewPreference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cards => "cards",
            Self::Table => "table",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferencesDto {
    pub user_id: uuid::Uuid,
    pub theme: String,
    pub density: String,
    pub sidebar_collapsed: bool,
    pub default_systems_view: String,
    pub updated_at: DateTime<Utc>,
}

impl From<crate::models::user_preferences::UserPreferences> for UserPreferencesDto {
    fn from(value: crate::models::user_preferences::UserPreferences) -> Self {
        Self {
            user_id: value.user_id,
            theme: value.theme,
            density: value.density,
            sidebar_collapsed: value.sidebar_collapsed,
            default_systems_view: value.default_systems_view,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferencesResponse {
    pub preferences: Option<UserPreferencesDto>,
}

impl UserPreferencesResponse {
    pub fn new(preferences: Option<crate::models::user_preferences::UserPreferences>) -> Self {
        Self {
            preferences: preferences.map(UserPreferencesDto::from),
        }
    }
}

impl From<crate::models::user_preferences::UserPreferences> for UserPreferencesResponse {
    fn from(value: crate::models::user_preferences::UserPreferences) -> Self {
        Self {
            preferences: Some(UserPreferencesDto::from(value)),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateUserPreferences {
    pub theme: Option<UiThemePreference>,
    pub density: Option<UiDensityPreference>,
    pub sidebar_collapsed: Option<bool>,
    pub default_systems_view: Option<SystemsViewPreference>,
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
    CveScanRequested,
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

/// Persisted classification banner configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationBannerConfig {
    pub enabled: bool,
    pub level: String,
    pub custom_text: String,
}

/// Request payload for updating the classification banner configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateClassificationBannerRequest {
    pub enabled: bool,
    pub level: String,
    pub custom_text: String,
}

/// Persisted server-wide automatic retry policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutomaticRetryPolicyResponse {
    pub max_build_retries: i16,
    pub max_evaluation_retries: i16,
    pub backoff_seconds: i32,
    pub transient_only: bool,
}

/// Complete replacement payload for the automatic retry policy.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAutomaticRetryPolicyRequest {
    pub max_build_retries: i16,
    pub max_evaluation_retries: i16,
    pub backoff_seconds: i32,
    pub transient_only: bool,
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
    fn update_system_request_omitted_fqdn_is_unset() {
        // Older/partial clients that don't send `fqdn` must not clear it.
        let json = r#"{"hostname":"web01","deployment_policy":"manual"}"#;
        let req: UpdateSystemRequest =
            serde_json::from_str(json).expect("payload without fqdn should deserialize");
        assert_eq!(req.fqdn, FieldUpdate::Unset);
        assert!(req.fqdn.is_unset());
    }

    #[test]
    fn update_system_request_null_fqdn_is_clear() {
        let json = r#"{"hostname":"web01","fqdn":null,"deployment_policy":"manual"}"#;
        let req: UpdateSystemRequest =
            serde_json::from_str(json).expect("payload with null fqdn should deserialize");
        assert_eq!(req.fqdn, FieldUpdate::Clear);
    }

    #[test]
    fn update_system_request_value_fqdn_is_set() {
        let json =
            r#"{"hostname":"web01","fqdn":"web01.prod.cf.internal","deployment_policy":"manual"}"#;
        let req: UpdateSystemRequest =
            serde_json::from_str(json).expect("payload with fqdn value should deserialize");
        assert_eq!(
            req.fqdn,
            FieldUpdate::Set("web01.prod.cf.internal".to_string())
        );
    }

    #[test]
    fn field_update_default_is_unset() {
        let value: FieldUpdate<String> = FieldUpdate::default();
        assert_eq!(value, FieldUpdate::Unset);
    }

    #[test]
    fn update_system_request_omitted_heartbeat_interval_is_unset() {
        // Older/partial clients that don't send `heartbeat_interval_secs` must not clear it.
        let json = r#"{"hostname":"web01","deployment_policy":"manual"}"#;
        let req: UpdateSystemRequest = serde_json::from_str(json)
            .expect("payload without heartbeat_interval_secs should deserialize");
        assert_eq!(req.heartbeat_interval_secs, FieldUpdate::Unset);
    }

    #[test]
    fn update_system_request_null_heartbeat_interval_is_clear() {
        let json =
            r#"{"hostname":"web01","heartbeat_interval_secs":null,"deployment_policy":"manual"}"#;
        let req: UpdateSystemRequest = serde_json::from_str(json)
            .expect("payload with null heartbeat_interval_secs should deserialize");
        assert_eq!(req.heartbeat_interval_secs, FieldUpdate::Clear);
    }

    #[test]
    fn update_system_request_value_heartbeat_interval_is_set() {
        let json =
            r#"{"hostname":"web01","heartbeat_interval_secs":120,"deployment_policy":"manual"}"#;
        let req: UpdateSystemRequest = serde_json::from_str(json)
            .expect("payload with heartbeat_interval_secs value should deserialize");
        assert_eq!(req.heartbeat_interval_secs, FieldUpdate::Set(120));
    }

    #[test]
    fn update_system_request_omits_unset_heartbeat_interval() {
        // Unset must be omitted entirely: serializing as `null` would be
        // interpreted as Clear by the receiving server (the original P1-3 bug).
        let request = UpdateSystemRequest {
            hostname: "web01".into(),
            fqdn: FieldUpdate::Unset,
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
            fqdn: FieldUpdate::Unset,
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
            fqdn: FieldUpdate::Unset,
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

// ─────────────────────────────────────────────────────────────────────────────
// Admin Server Info DTOs — GET /api/v1/admin/server-info
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime database information displayed in Admin → Server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseRuntimeInfo {
    pub status: String,
    pub name: String,
    pub size: String,
    pub server_version: String,
}

/// Runtime server/build information displayed in Admin → Server.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
