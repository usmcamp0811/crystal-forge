//! Typed fixture builders for deterministic showcase demos.
//!
//! All fixtures are static, deterministic, and reusable across showcase demos
//! to ensure consistent visual testing and isolation development.

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
