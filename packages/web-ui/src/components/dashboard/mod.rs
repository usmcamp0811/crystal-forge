//! Dashboard-specific components.
//!
//! Provides reusable dashboard panels and widgets for displaying
//! fleet health, deployment status, build queue, and CVE summaries.

mod build_queue;
mod build_summary;
mod cve_summary;
mod deployment_status;
mod fleet_health;
mod recent_deployments;

pub use build_queue::{BuildQueuePanel, BuildQueueRow};
pub use build_summary::BuildSummaryPanel;
pub use cve_summary::{CveSeverityBadge, CveSummaryPanel};
pub use deployment_status::DeploymentStatusBreakdown;
pub use fleet_health::FleetHealthBreakdown;
pub use recent_deployments::{RecentDeploymentRow, RecentDeploymentsList};

/// Format seconds into a human-readable elapsed time string.
pub fn format_elapsed(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

/// Format a datetime as relative time (e.g., "2 hours ago").
pub fn format_time_ago(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration < chrono::Duration::minutes(1) {
        "just now".to_string()
    } else if duration < chrono::Duration::hours(1) {
        let mins = duration.num_minutes();
        format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" })
    } else if duration < chrono::Duration::days(1) {
        let hours = duration.num_hours();
        format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
    } else {
        let days = duration.num_days();
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    }
}
