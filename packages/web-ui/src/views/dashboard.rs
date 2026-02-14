//! Dashboard view — fleet-wide overview with health, deployment, and CVE summaries.

use chrono::{Duration, Utc};
use dioxus::prelude::*;

use crate::api::models::{
    CveSummary, DashboardSummary, DeploymentStatus, DeploymentStatusSummary, FleetHealthSummary,
    RecentDeployment,
};
use crate::components::layout::Card;
use crate::components::stat_card::StatCard;
use crate::theme;

/// The main dashboard page.
#[component]
pub fn DashboardView() -> Element {
    // TODO: Replace with real API call using use_resource + fetch_dashboard()
    let dashboard = mock_dashboard_summary();

    rsx! {
        div {
            class: "space-y-8",
            "data-testid": "dashboard",

            // Top stats row
            div {
                class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4",
                StatCard {
                    label: "Total Systems".to_string(),
                    value: dashboard.total_systems.to_string()
                }
                StatCard {
                    label: "Healthy".to_string(),
                    value: dashboard.fleet_health.healthy.to_string(),
                    color_class: theme::health::HEALTHY_TEXT.to_string()
                }
                StatCard {
                    label: "Critical".to_string(),
                    value: dashboard.fleet_health.critical.to_string(),
                    color_class: theme::health::CRITICAL_TEXT.to_string()
                }
                StatCard {
                    label: "Active Builds".to_string(),
                    value: dashboard.active_builds.to_string(),
                    color_class: "text-blue-400".to_string()
                }
            }

            // Two-column layout for health breakdown and CVE summary
            div {
                class: "grid grid-cols-1 lg:grid-cols-2 gap-6",

                // Fleet Health Breakdown
                Card {
                    title: Some("Fleet Health".to_string()),
                    children: rsx! {
                        FleetHealthBreakdown { health: dashboard.fleet_health.clone() }
                    }
                }

                // CVE Summary
                Card {
                    title: Some("CVE Summary".to_string()),
                    children: rsx! {
                        CveSummaryPanel { cves: dashboard.cve_summary.clone() }
                    }
                }
            }

            // Deployment Status + Recent Deployments
            div {
                class: "grid grid-cols-1 lg:grid-cols-2 gap-6",

                // Deployment Status Breakdown
                Card {
                    title: Some("Deployment Status".to_string()),
                    children: rsx! {
                        DeploymentStatusBreakdown { status: dashboard.deployment_status.clone() }
                    }
                }

                // Recent Deployments
                Card {
                    title: Some("Recent Deployments".to_string()),
                    children: rsx! {
                        RecentDeploymentsList { deployments: dashboard.recent_deployments.clone() }
                    }
                }
            }
        }
    }
}

/// Fleet health breakdown with colored progress bars.
#[component]
fn FleetHealthBreakdown(health: FleetHealthSummary) -> Element {
    let total = health.total().max(1) as f64;

    rsx! {
        div {
            class: "space-y-4",
            "data-testid": "fleet-health-breakdown",

            // Stacked bar visualization
            div {
                class: "h-4 rounded-full overflow-hidden flex {theme::surface::SUBTLE_BG}",
                if health.healthy > 0 {
                    div {
                        class: "bg-emerald-500 transition-all",
                        style: "width: {(health.healthy as f64 / total * 100.0):.1}%"
                    }
                }
                if health.warning > 0 {
                    div {
                        class: "bg-amber-500 transition-all",
                        style: "width: {(health.warning as f64 / total * 100.0):.1}%"
                    }
                }
                if health.critical > 0 {
                    div {
                        class: "bg-red-500 transition-all",
                        style: "width: {(health.critical as f64 / total * 100.0):.1}%"
                    }
                }
                if health.offline > 0 {
                    div {
                        class: "bg-gray-600 transition-all",
                        style: "width: {(health.offline as f64 / total * 100.0):.1}%"
                    }
                }
            }

            // Legend
            div {
                class: "grid grid-cols-2 gap-3",
                HealthLegendItem { label: "Healthy", count: health.healthy, dot_class: theme::health::HEALTHY_DOT }
                HealthLegendItem { label: "Warning", count: health.warning, dot_class: theme::health::WARNING_DOT }
                HealthLegendItem { label: "Critical", count: health.critical, dot_class: theme::health::CRITICAL_DOT }
                HealthLegendItem { label: "Offline", count: health.offline, dot_class: theme::health::OFFLINE_DOT }
            }
        }
    }
}

/// A single legend item with dot and count.
#[component]
fn HealthLegendItem(label: &'static str, count: i64, dot_class: &'static str) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",
            span { class: "w-3 h-3 rounded-full {dot_class}" }
            span { class: "{theme::text::SECONDARY} text-sm", "{label}" }
            span { class: "ml-auto text-white font-medium", "{count}" }
        }
    }
}

/// CVE summary panel with severity badges.
#[component]
fn CveSummaryPanel(cves: CveSummary) -> Element {
    let total = cves.total();

    rsx! {
        div {
            class: "space-y-4",
            "data-testid": "cve-summary",

            // Total count header
            div {
                class: "flex items-baseline gap-2",
                span { class: "text-3xl font-bold text-white", "{total}" }
                span { class: "{theme::text::SECONDARY}", "total vulnerabilities" }
            }

            // Severity breakdown
            div {
                class: "grid grid-cols-2 gap-3",
                CveSeverityBadge { label: "Critical", count: cves.critical, text_class: theme::cve::CRITICAL_TEXT, bg_class: theme::cve::CRITICAL_BG }
                CveSeverityBadge { label: "High", count: cves.high, text_class: theme::cve::HIGH_TEXT, bg_class: theme::cve::HIGH_BG }
                CveSeverityBadge { label: "Medium", count: cves.medium, text_class: theme::cve::MEDIUM_TEXT, bg_class: theme::cve::MEDIUM_BG }
                CveSeverityBadge { label: "Low", count: cves.low, text_class: theme::cve::LOW_TEXT, bg_class: theme::cve::LOW_BG }
            }
        }
    }
}

/// A single CVE severity badge with count.
#[component]
fn CveSeverityBadge(
    label: &'static str,
    count: i64,
    text_class: &'static str,
    bg_class: &'static str,
) -> Element {
    rsx! {
        div {
            class: "flex items-center justify-between p-3 rounded-lg {bg_class}",
            span { class: "{text_class} font-medium", "{label}" }
            span { class: "{text_class} text-xl font-bold", "{count}" }
        }
    }
}

/// Deployment status breakdown.
#[component]
fn DeploymentStatusBreakdown(status: DeploymentStatusSummary) -> Element {
    let total = status.total().max(1) as f64;

    rsx! {
        div {
            class: "space-y-4",
            "data-testid": "deployment-status",

            // Stacked bar visualization
            div {
                class: "h-4 rounded-full overflow-hidden flex {theme::surface::SUBTLE_BG}",
                if status.up_to_date > 0 {
                    div {
                        class: "bg-emerald-500 transition-all",
                        style: "width: {(status.up_to_date as f64 / total * 100.0):.1}%"
                    }
                }
                if status.behind > 0 {
                    div {
                        class: "bg-amber-500 transition-all",
                        style: "width: {(status.behind as f64 / total * 100.0):.1}%"
                    }
                }
                if status.never_deployed > 0 {
                    div {
                        class: "bg-gray-600 transition-all",
                        style: "width: {(status.never_deployed as f64 / total * 100.0):.1}%"
                    }
                }
                if status.unknown > 0 {
                    div {
                        class: "bg-gray-500 transition-all",
                        style: "width: {(status.unknown as f64 / total * 100.0):.1}%"
                    }
                }
            }

            // Legend
            div {
                class: "grid grid-cols-2 gap-3",
                DeploymentLegendItem { label: "Up to Date", count: status.up_to_date, dot_class: "bg-emerald-500" }
                DeploymentLegendItem { label: "Behind", count: status.behind, dot_class: "bg-amber-500" }
                DeploymentLegendItem { label: "Never Deployed", count: status.never_deployed, dot_class: "bg-gray-600" }
                DeploymentLegendItem { label: "Unknown", count: status.unknown, dot_class: "bg-gray-500" }
            }
        }
    }
}

/// A single deployment legend item.
#[component]
fn DeploymentLegendItem(label: &'static str, count: i64, dot_class: &'static str) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-2",
            span { class: "w-3 h-3 rounded-full {dot_class}" }
            span { class: "{theme::text::SECONDARY} text-sm", "{label}" }
            span { class: "ml-auto text-white font-medium", "{count}" }
        }
    }
}

/// Recent deployments list.
#[component]
fn RecentDeploymentsList(deployments: Vec<RecentDeployment>) -> Element {
    if deployments.is_empty() {
        return rsx! {
            p { class: "{theme::text::SECONDARY}", "No recent deployments." }
        };
    }

    rsx! {
        div {
            class: "space-y-3",
            "data-testid": "recent-deployments",
            for deployment in deployments {
                RecentDeploymentRow { deployment }
            }
        }
    }
}

/// A single deployment row in the recent deployments list.
#[component]
fn RecentDeploymentRow(deployment: RecentDeployment) -> Element {
    let status_color = deployment.status.color_class();
    let time_ago = format_time_ago(deployment.deployed_at);
    let short_hash = deployment
        .commit_hash
        .chars()
        .take(7)
        .collect::<String>();

    rsx! {
        div {
            class: "flex items-center justify-between p-3 rounded-lg {theme::surface::SUBTLE_BG}",
            div {
                class: "flex items-center gap-3",
                // Status indicator dot
                span {
                    class: "w-2 h-2 rounded-full",
                    class: if deployment.status == DeploymentStatus::UpToDate { "bg-emerald-500" } else { "bg-amber-500" }
                }
                div {
                    p { class: "text-white font-medium", "{deployment.hostname}" }
                    p { class: "{theme::text::MUTED} text-xs font-mono", "{short_hash}" }
                }
            }
            div {
                class: "text-right",
                p { class: "{status_color} text-sm", "{deployment.status.label()}" }
                p { class: "{theme::text::MUTED} text-xs", "{time_ago}" }
            }
        }
    }
}

/// Format a datetime as relative time (e.g., "2 hours ago").
fn format_time_ago(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration < Duration::minutes(1) {
        "just now".to_string()
    } else if duration < Duration::hours(1) {
        let mins = duration.num_minutes();
        format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" })
    } else if duration < Duration::days(1) {
        let hours = duration.num_hours();
        format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
    } else {
        let days = duration.num_days();
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    }
}

/// Generate mock dashboard data for development.
fn mock_dashboard_summary() -> DashboardSummary {
    let now = Utc::now();

    DashboardSummary {
        fleet_health: FleetHealthSummary {
            healthy: 42,
            warning: 7,
            critical: 3,
            offline: 2,
        },
        deployment_status: DeploymentStatusSummary {
            up_to_date: 38,
            behind: 12,
            never_deployed: 3,
            unknown: 1,
        },
        cve_summary: CveSummary {
            critical: 5,
            high: 23,
            medium: 67,
            low: 142,
        },
        total_systems: 54,
        active_builds: 3,
        recent_deployments: vec![
            RecentDeployment {
                hostname: "atlas-01".to_string(),
                commit_hash: "a1b2c3d4e5f6789".to_string(),
                deployed_at: now - Duration::minutes(15),
                status: DeploymentStatus::UpToDate,
            },
            RecentDeployment {
                hostname: "nova-05".to_string(),
                commit_hash: "f9e8d7c6b5a4321".to_string(),
                deployed_at: now - Duration::hours(2),
                status: DeploymentStatus::UpToDate,
            },
            RecentDeployment {
                hostname: "luna-02".to_string(),
                commit_hash: "1234567890abcdef".to_string(),
                deployed_at: now - Duration::hours(5),
                status: DeploymentStatus::Behind,
            },
            RecentDeployment {
                hostname: "orion-03".to_string(),
                commit_hash: "deadbeefcafe1234".to_string(),
                deployed_at: now - Duration::days(1),
                status: DeploymentStatus::UpToDate,
            },
            RecentDeployment {
                hostname: "vega-04".to_string(),
                commit_hash: "cafe1234deadbeef".to_string(),
                deployed_at: now - Duration::days(2),
                status: DeploymentStatus::Behind,
            },
        ],
        timestamp: now,
    }
}
