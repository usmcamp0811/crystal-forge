//! Typed fixture builders for deterministic showcase demos.
//!
//! All fixtures are static, deterministic, and reusable across showcase demos
//! to ensure consistent visual testing and isolation development.

use crate::api::models::{
    BuildQueueItem, BuildQueueSummary, BuildStatus, CveSummary, DeploymentStatus, HealthStatus,
    PipelineStage, RecentDeployment, SystemSummary,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub struct StatCardFixture {
    pub label: &'static str,
    pub value: &'static str,
    pub caption: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimelineItemFixture {
    pub title: &'static str,
    pub meta: &'static str,
    pub status: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SystemFixture {
    pub hostname: &'static str,
    pub environment: &'static str,
    pub health: &'static str,
    pub deployment_status: &'static str,
    pub last_seen: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlakeFixture {
    pub name: &'static str,
    pub repo_url: &'static str,
    pub branch: &'static str,
    pub last_commit: &'static str,
    pub commit_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildFixture {
    pub id: u32,
    pub flake_name: &'static str,
    pub commit_short: &'static str,
    pub status: &'static str,
    pub started_at: &'static str,
}

pub fn stat_card_fixtures() -> Vec<StatCardFixture> {
    vec![
        StatCardFixture {
            label: "Total Systems",
            value: "24",
            caption: "fleet overview",
        },
        StatCardFixture {
            label: "Active Builds",
            value: "6",
            caption: "in progress",
        },
        StatCardFixture {
            label: "Policy Failures",
            value: "2",
            caption: "needs review",
        },
        StatCardFixture {
            label: "CVE Alerts",
            value: "12",
            caption: "high severity",
        },
    ]
}

pub fn timeline_fixtures() -> Vec<TimelineItemFixture> {
    vec![
        TimelineItemFixture {
            title: "flake/core-infra",
            meta: "commit a1b2c3d - 2 min ago",
            status: "evaluating",
        },
        TimelineItemFixture {
            title: "flake/edge-cluster",
            meta: "commit d4e5f6g - 8 min ago",
            status: "ready for build",
        },
        TimelineItemFixture {
            title: "flake/dev-desktops",
            meta: "commit h7i8j9k - 14 min ago",
            status: "policy failed",
        },
        TimelineItemFixture {
            title: "flake/monitoring",
            meta: "commit l1m2n3o - 22 min ago",
            status: "building",
        },
    ]
}

pub fn system_fixtures() -> Vec<SystemFixture> {
    vec![
        SystemFixture {
            hostname: "web-server-1",
            environment: "production",
            health: "healthy",
            deployment_status: "up-to-date",
            last_seen: "2 minutes ago",
        },
        SystemFixture {
            hostname: "db-primary",
            environment: "production",
            health: "warning",
            deployment_status: "behind",
            last_seen: "5 minutes ago",
        },
        SystemFixture {
            hostname: "staging-app",
            environment: "staging",
            health: "healthy",
            deployment_status: "up-to-date",
            last_seen: "1 minute ago",
        },
        SystemFixture {
            hostname: "dev-machine",
            environment: "development",
            health: "critical",
            deployment_status: "never-deployed",
            last_seen: "30 minutes ago",
        },
    ]
}

pub fn flake_fixtures() -> Vec<FlakeFixture> {
    vec![
        FlakeFixture {
            name: "infrastructure",
            repo_url: "git+ssh://git@gitlab.com/company/nixos-configs",
            branch: "main",
            last_commit: "feat: add monitoring stack",
            commit_count: 247,
        },
        FlakeFixture {
            name: "web-services",
            repo_url: "https://github.com/company/web-nixos",
            branch: "production",
            last_commit: "fix: update nginx config",
            commit_count: 89,
        },
    ]
}

pub fn build_fixtures() -> Vec<BuildFixture> {
    vec![
        BuildFixture {
            id: 1234,
            flake_name: "infrastructure",
            commit_short: "a1b2c3d",
            status: "success",
            started_at: "10 minutes ago",
        },
        BuildFixture {
            id: 1235,
            flake_name: "web-services",
            commit_short: "d4e5f6g",
            status: "building",
            started_at: "3 minutes ago",
        },
        BuildFixture {
            id: 1236,
            flake_name: "infrastructure",
            commit_short: "h7i8j9k",
            status: "failed",
            started_at: "45 minutes ago",
        },
    ]
}

/// Helper to create a fixed datetime for deterministic fixtures.
fn mock_datetime() -> DateTime<Utc> {
    "2026-03-16T12:00:00Z".parse().unwrap()
}

/// Helper to create a fixed UUID for deterministic fixtures.
fn mock_uuid(index: u8) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[15] = index;
    Uuid::from_bytes(bytes)
}

/// Create BuildQueueItem fixtures for showcase demos with all states.
pub fn build_queue_item_fixtures() -> Vec<BuildQueueItem> {
    let base_time = mock_datetime();

    vec![
        // Building state
        BuildQueueItem {
            job_id: None,
            system_id: None,
            hostname: "web-server-1".to_string(),
            flake_name: "infrastructure".to_string(),
            commit_hash: "a1b2c3d".to_string(),
            commit_message: Some("feat: add user authentication module".to_string()),
            status: BuildStatus::Building,
            builder_name: Some("builder-1".to_string()),
            queued_at: base_time - chrono::Duration::minutes(15),
            started_at: Some(base_time - chrono::Duration::minutes(12)),
            elapsed_secs: Some(720), // 12 minutes
            logs: None,
        },
        // Queued state (next in queue)
        BuildQueueItem {
            job_id: None,
            system_id: None,
            hostname: "db-primary".to_string(),
            flake_name: "infrastructure".to_string(),
            commit_hash: "f7e8d9c".to_string(),
            commit_message: Some("fix: database connection pooling".to_string()),
            status: BuildStatus::Queued,
            builder_name: None,
            queued_at: base_time - chrono::Duration::minutes(5),
            started_at: None,
            elapsed_secs: None,
            logs: None,
        },
        // Queued state (second in queue)
        BuildQueueItem {
            job_id: None,
            system_id: None,
            hostname: "staging-app".to_string(),
            flake_name: "web-services".to_string(),
            commit_hash: "b2c3d4e".to_string(),
            commit_message: Some("chore: update dependencies".to_string()),
            status: BuildStatus::Queued,
            builder_name: None,
            queued_at: base_time - chrono::Duration::minutes(3),
            started_at: None,
            elapsed_secs: None,
            logs: None,
        },
        // Building state with long commit message (overflow test)
        BuildQueueItem {
            job_id: None,
            system_id: None,
            hostname: "production-worker-node-with-very-long-hostname".to_string(),
            flake_name: "monitoring-stack".to_string(),
            commit_hash: "c3d4e5f".to_string(),
            commit_message: Some("feat: implement comprehensive monitoring dashboard with real-time metrics, alerting, and historical data visualization".to_string()),
            status: BuildStatus::Building,
            builder_name: Some("builder-2".to_string()),
            queued_at: base_time - chrono::Duration::minutes(8),
            started_at: Some(base_time - chrono::Duration::minutes(6)),
            elapsed_secs: Some(360), // 6 minutes
            logs: None,
        },
        // Queued state with no commit message (empty content test)
        BuildQueueItem {
            job_id: None,
            system_id: None,
            hostname: "dev-machine".to_string(),
            flake_name: "development".to_string(),
            commit_hash: "d4e5f6a".to_string(),
            commit_message: None,
            status: BuildStatus::Queued,
            builder_name: None,
            queued_at: base_time - chrono::Duration::minutes(1),
            started_at: None,
            elapsed_secs: None,
            logs: None,
        },
    ]
}

/// Create SystemSummary fixtures for showcase demos with all states.
pub fn system_summary_fixtures() -> Vec<SystemSummary> {
    let base_time = mock_datetime();

    vec![
        // Healthy production system (up-to-date)
        SystemSummary {
            id: mock_uuid(1),
            hostname: "web-server-1".to_string(),
            environment: Some("production".to_string()),
            flake_id: Some(1),
            primary_ip: Some("192.168.1.10".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::UpToDate,
            pipeline_stage: Some(PipelineStage::ReadyForDeploy),
            cve_counts: CveSummary {
                critical: 0,
                high: 2,
                medium: 5,
                low: 12,
            },
            nixos_version: Some("24.05".to_string()),
            last_seen: Some(base_time - chrono::Duration::minutes(2)),
            deployment_policy: "auto_latest".to_string(),
        },
        // Warning staging system (behind)
        SystemSummary {
            id: mock_uuid(2),
            hostname: "staging-app".to_string(),
            environment: Some("staging".to_string()),
            flake_id: Some(1),
            primary_ip: Some("192.168.2.20".to_string()),
            health_status: HealthStatus::Warning,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::ReadyForBuild),
            cve_counts: CveSummary {
                critical: 1,
                high: 8,
                medium: 15,
                low: 22,
            },
            nixos_version: Some("24.05".to_string()),
            last_seen: Some(base_time - chrono::Duration::minutes(10)),
            deployment_policy: "manual".to_string(),
        },
        // Critical dev system (never deployed)
        SystemSummary {
            id: mock_uuid(3),
            hostname: "dev-machine".to_string(),
            environment: Some("development".to_string()),
            flake_id: Some(2),
            primary_ip: Some("10.0.0.50".to_string()),
            health_status: HealthStatus::Critical,
            deployment_status: DeploymentStatus::NeverDeployed,
            pipeline_stage: Some(PipelineStage::Unknown),
            cve_counts: CveSummary {
                critical: 5,
                high: 18,
                medium: 42,
                low: 67,
            },
            nixos_version: None,
            last_seen: Some(base_time - chrono::Duration::minutes(45)),
            deployment_policy: "manual".to_string(),
        },
        // Offline system (no environment)
        SystemSummary {
            id: mock_uuid(4),
            hostname: "legacy-server".to_string(),
            environment: None,
            flake_id: None,
            primary_ip: None,
            health_status: HealthStatus::Offline,
            deployment_status: DeploymentStatus::Unknown,
            pipeline_stage: None,
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            },
            nixos_version: Some("23.11".to_string()),
            last_seen: Some(base_time - chrono::Duration::hours(24)),
            deployment_policy: "manual".to_string(),
        },
        // Healthy system with long hostname (overflow test)
        SystemSummary {
            id: mock_uuid(5),
            hostname: "production-worker-node-with-very-long-hostname-01".to_string(),
            environment: Some("production".to_string()),
            flake_id: Some(1),
            primary_ip: Some("172.16.100.200".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::UpToDate,
            pipeline_stage: Some(PipelineStage::BuildComplete),
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 3,
                low: 8,
            },
            nixos_version: Some("24.05".to_string()),
            last_seen: Some(base_time - chrono::Duration::seconds(30)),
            deployment_policy: "Immediate".to_string(),
        },
        // Building state system
        SystemSummary {
            id: mock_uuid(6),
            hostname: "db-primary".to_string(),
            environment: Some("production".to_string()),
            flake_id: Some(1),
            primary_ip: Some("192.168.1.20".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::Building),
            cve_counts: CveSummary {
                critical: 0,
                high: 1,
                medium: 4,
                low: 10,
            },
            nixos_version: Some("24.05".to_string()),
            last_seen: Some(base_time - chrono::Duration::minutes(1)),
            deployment_policy: "pinned".to_string(),
        },
    ]
}

/// Create RecentDeployment fixtures for showcase demos.
pub fn recent_deployment_fixtures() -> Vec<RecentDeployment> {
    let base_time = mock_datetime();

    vec![
        // Up-to-date deployment
        RecentDeployment {
            hostname: "web-server-1".to_string(),
            commit_hash: "a1b2c3d4e5f".to_string(),
            commit_message: Some("feat: add user authentication module".to_string()),
            deployed_at: base_time - chrono::Duration::minutes(5),
            status: DeploymentStatus::UpToDate,
        },
        // Behind deployment
        RecentDeployment {
            hostname: "staging-app".to_string(),
            commit_hash: "f7e8d9c6b5a".to_string(),
            commit_message: Some("fix: database connection pooling issues".to_string()),
            deployed_at: base_time - chrono::Duration::minutes(45),
            status: DeploymentStatus::Behind,
        },
        // Recent deployment with long commit message
        RecentDeployment {
            hostname: "production-api-gateway".to_string(),
            commit_hash: "b2c3d4e5f6a".to_string(),
            commit_message: Some(
                "refactor: comprehensive API gateway refactoring with improved rate limiting, authentication middleware, and extensive logging capabilities".to_string(),
            ),
            deployed_at: base_time - chrono::Duration::minutes(2),
            status: DeploymentStatus::UpToDate,
        },
        // Deployment with no message
        RecentDeployment {
            hostname: "dev-machine".to_string(),
            commit_hash: "c3d4e5f6a7b".to_string(),
            commit_message: None,
            deployed_at: base_time - chrono::Duration::hours(2),
            status: DeploymentStatus::Behind,
        },
        // Very recent deployment
        RecentDeployment {
            hostname: "db-primary".to_string(),
            commit_hash: "d4e5f6a7b8c".to_string(),
            commit_message: Some("chore: update dependencies".to_string()),
            deployed_at: base_time - chrono::Duration::seconds(30),
            status: DeploymentStatus::UpToDate,
        },
    ]
}

/// Create BuildQueueSummary fixture for showcase demos.
pub fn build_queue_summary_fixture() -> BuildQueueSummary {
    BuildQueueSummary {
        building_count: 2,
        queued_count: 3,
        items: build_queue_item_fixtures(),
        timestamp: mock_datetime(),
    }
}
