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
    ]
}
