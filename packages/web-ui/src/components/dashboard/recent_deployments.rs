//! Recent deployments list and row components.

use dioxus::prelude::*;

use crate::api::models::{DeploymentStatus, RecentDeployment};
use crate::theme;

use super::format_time_ago;

/// Recent deployments list panel.
#[component]
pub fn RecentDeploymentsList(
    deployments: Vec<RecentDeployment>,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    if deployments.is_empty() {
        return rsx! {
            p { class: "{theme::text::SECONDARY}", "No recent deployments." }
        };
    }

    rsx! {
        div {
            class: "flex flex-col h-full",
            "data-testid": "recent-deployments",

            // Show filter indicator if filtered
            if let Some(ref flake_name) = flake_filter {
                div {
                    class: "text-xs text-blue-400 mb-2 flex items-center gap-1 shrink-0",
                    svg {
                        class: "w-3 h-3",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z"
                        }
                    }
                    span { "{flake_name}" }
                }
            }

            // Scrollable list container - prevents overflow when widget is moved
            div {
                class: "flex-1 min-h-0 overflow-y-auto space-y-2",
                for deployment in deployments {
                    RecentDeploymentRow { deployment }
                }
            }
        }
    }
}

/// A single deployment row in the recent deployments list.
#[component]
pub fn RecentDeploymentRow(deployment: RecentDeployment) -> Element {
    let status_color = deployment.status.color_class();
    let time_ago = format_time_ago(deployment.deployed_at);
    let short_hash = deployment.commit_hash.chars().take(7).collect::<String>();

    // Truncate commit message to ~50 chars for display
    let commit_msg = deployment.commit_message.as_ref().map(|msg| {
        if msg.len() > 50 {
            format!("{}...", &msg[..47])
        } else {
            msg.clone()
        }
    });

    rsx! {
        Link {
            class: "flex items-center justify-between p-3 rounded-lg {theme::surface::SUBTLE_BG} transition {theme::interactive::HOVER_BG} hover:border {theme::surface::CARD_BORDER}",
            to: crate::routes::Route::SystemsView {},
            div {
                class: "flex items-center gap-3 min-w-0 flex-1",
                // Status indicator dot
                span {
                    class: "w-2 h-2 rounded-full shrink-0",
                    class: if deployment.status == DeploymentStatus::UpToDate { "bg-emerald-500" } else { "bg-amber-500" }
                }
                div {
                    class: "min-w-0 flex-1",
                    div {
                        class: "flex items-center gap-2",
                        span { class: "{theme::text::PRIMARY} text-sm font-medium truncate", "{deployment.hostname}" }
                        span { class: "text-[10px] font-mono {theme::text::MUTED}", "{short_hash}" }
                    }
                    if let Some(ref msg) = commit_msg {
                        p { class: "text-xs {theme::text::SECONDARY} truncate", "{msg}" }
                    }
                }
            }
            div {
                class: "text-right shrink-0 ml-3",
                p { class: "text-xs {theme::text::SECONDARY}", "{time_ago}" }
                p { class: "text-[10px] uppercase tracking-wide {status_color}", "{deployment.status.label()}" }
            }
        }
    }
}
