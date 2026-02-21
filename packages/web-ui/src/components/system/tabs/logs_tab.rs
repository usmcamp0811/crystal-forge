//! Logs tab components for system detail view.
//!
//! Components for displaying deployment logs with log levels
//! and timestamps.

use dioxus::prelude::*;

use crate::api::models::{DeploymentLogEntry, LogLevel};
use crate::theme;

/// Deployment logs tab showing recent log entries.
#[component]
pub fn LogsTab(logs: Vec<DeploymentLogEntry>) -> Element {
    if logs.is_empty() {
        return rsx! {
            div {
                class: "pt-6 text-center py-12",
                p {
                    class: "{theme::text::SECONDARY}",
                    "No deployment logs available."
                }
            }
        };
    }

    // Get the deployment phase for grouping
    let first_phase = logs
        .first()
        .and_then(|l| l.phase.clone())
        .unwrap_or_else(|| "Deployment".to_string());

    rsx! {
        div {
            class: "pt-6",

            // Header
            div {
                class: "flex items-center justify-between mb-4",
                h3 {
                    class: "{theme::typography::SECTION_TITLE} text-white",
                    "Recent Deployment"
                }
                // TODO: Add link to full logs view
                button {
                    class: "text-sm text-blue-400 hover:text-blue-300 transition-colors",
                    "View full logs →"
                }
            }

            // Log container
            div {
                class: "rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} overflow-hidden",

                // Phase header
                div {
                    class: "px-4 py-2 bg-gray-800/50 border-b border-gray-700",
                    span {
                        class: "text-xs font-medium text-gray-400 uppercase tracking-wider",
                        "{first_phase}"
                    }
                }

                // Log entries
                div {
                    class: "font-mono text-sm divide-y divide-gray-800/50 max-h-[400px] overflow-y-auto",
                    for log in logs.iter() {
                        LogLine { log: log.clone() }
                    }
                }
            }
        }
    }
}

/// Single log line with timestamp, level indicator, and message.
#[component]
pub fn LogLine(log: DeploymentLogEntry) -> Element {
    let time = log.timestamp.format("%H:%M:%S").to_string();
    let level_bg = match log.level {
        LogLevel::Error => "bg-red-500",
        LogLevel::Warn => "bg-yellow-500",
        LogLevel::Info => "bg-gray-500",
        LogLevel::Debug => "bg-gray-700",
    };
    let level_text = log.level.color_class();

    rsx! {
        div {
            class: "flex gap-3 px-4 py-2 hover:bg-gray-800/30",

            // Timestamp
            span {
                class: "shrink-0 text-xs {theme::text::MUTED}",
                "{time}"
            }

            // Level indicator
            span {
                class: "shrink-0 w-1 rounded-full {level_bg}",
            }

            // Message
            span {
                class: "flex-1 {level_text}",
                "{log.message}"
            }
        }
    }
}
