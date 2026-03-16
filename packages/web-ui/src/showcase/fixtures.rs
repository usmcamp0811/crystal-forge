//! Typed fixture builders for deterministic showcase demos.
//!
//! All fixtures are static, deterministic, and reusable across showcase demos
//! to ensure consistent visual testing and isolation development.

use crate::api::models::{BuildQueueItem, BuildStatus};
use chrono::{DateTime, Utc};

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
