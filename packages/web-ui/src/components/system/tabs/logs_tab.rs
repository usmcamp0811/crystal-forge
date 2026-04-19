//! Logs tab components for system detail view.
//!
//! Components for displaying deployment logs with log levels,
//! timestamps, day delineation, and time filtering.

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Utc};
use dioxus::prelude::*;
use std::collections::BTreeMap;

use crate::api::models::{DeploymentLogEntry, LogLevel};
use crate::theme;

/// Deployment logs tab showing recent log entries with day grouping and time filtering.
#[component]
pub fn LogsTab(
    logs: Vec<DeploymentLogEntry>,
    on_time_range_change: EventHandler<(Option<DateTime<Utc>>, Option<DateTime<Utc>>)>,
) -> Element {
    // State for date range inputs
    let mut since_input = use_signal(|| {
        let since = Utc::now() - Duration::hours(24);
        since.format("%Y-%m-%dT%H:%M").to_string()
    });
    let mut before_input = use_signal(|| {
        Utc::now().format("%Y-%m-%dT%H:%M").to_string()
    });

    // Handler for applying time filter
    let apply_filter = move |_| {
        let since_str = since_input.read().clone();
        let before_str = before_input.read().clone();
        
        // Parse datetime-local input (no timezone, assume UTC)
        let since = chrono::NaiveDateTime::parse_from_str(&since_str, "%Y-%m-%dT%H:%M")
            .ok()
            .map(|naive| Utc.from_utc_datetime(&naive));
        let before = chrono::NaiveDateTime::parse_from_str(&before_str, "%Y-%m-%dT%H:%M")
            .ok()
            .map(|naive| Utc.from_utc_datetime(&naive));
        
        on_time_range_change.call((since, before));
    };

    // Group logs by day
    let logs_by_day = group_logs_by_day(&logs);

    rsx! {
        div {
            class: "pt-6",

            // Header
            div {
                class: "flex items-center justify-between mb-4",
                h3 {
                    class: "{theme::typography::SECTION_TITLE} text-white",
                    "Agent Events"
                }
            }

            // Time range filter controls
            div {
                class: "mb-4 p-4 rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER}",
                div {
                    class: "flex flex-wrap items-end gap-4",
                    
                    // Since input
                    div {
                        class: "flex-1 min-w-[200px]",
                        label {
                            class: "block text-sm font-medium {theme::text::SECONDARY} mb-1",
                            "From"
                        }
                        input {
                            r#type: "datetime-local",
                            class: "w-full px-3 py-2 rounded border bg-gray-800 border-gray-700 text-gray-100 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500",
                            value: "{since_input}",
                            oninput: move |evt| since_input.set(evt.value().clone()),
                        }
                    }

                    // Before input
                    div {
                        class: "flex-1 min-w-[200px]",
                        label {
                            class: "block text-sm font-medium {theme::text::SECONDARY} mb-1",
                            "To"
                        }
                        input {
                            r#type: "datetime-local",
                            class: "w-full px-3 py-2 rounded border bg-gray-800 border-gray-700 text-gray-100 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500",
                            value: "{before_input}",
                            oninput: move |evt| before_input.set(evt.value().clone()),
                        }
                    }

                    // Apply button
                    button {
                        class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded text-sm font-medium transition-colors",
                        onclick: apply_filter,
                        "Apply Filter"
                    }
                }
            }

            // Empty state or log container
            if logs.is_empty() {
                div {
                    class: "text-center py-12 rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER}",
                    p {
                        class: "{theme::text::SECONDARY}",
                        "No agent events found in the selected time range."
                    }
                }
            } else {
                // Log container with day grouping
                div {
                    class: "rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} overflow-hidden",

                    div {
                        class: "font-mono text-sm max-h-[600px] overflow-y-auto",
                        
                        // Render logs grouped by day
                        for (day_key, day_logs) in logs_by_day.iter() {
                            // Day header
                            div {
                                class: "sticky top-0 z-10 px-4 py-2 bg-gray-800/95 border-b border-gray-700 backdrop-blur-sm",
                                span {
                                    class: "text-xs font-semibold text-gray-300 uppercase tracking-wider",
                                    "{day_key}"
                                }
                            }

                            // Log entries for this day
                            div {
                                class: "divide-y divide-gray-800/50",
                                for log in day_logs.iter() {
                                    LogLine { log: log.clone() }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Group logs by day (date string like "April 19, 2026").
fn group_logs_by_day(logs: &[DeploymentLogEntry]) -> BTreeMap<String, Vec<DeploymentLogEntry>> {
    let mut grouped: BTreeMap<String, Vec<DeploymentLogEntry>> = BTreeMap::new();
    
    for log in logs {
        // Convert to local time for display
        let local_time = log.timestamp.with_timezone(&Local);
        let day_key = local_time.format("%B %d, %Y").to_string();
        
        grouped.entry(day_key).or_insert_with(Vec::new).push(log.clone());
    }
    
    grouped
}

/// Single log line with relative timestamp, level indicator, and message.
#[component]
pub fn LogLine(log: DeploymentLogEntry) -> Element {
    let now = Utc::now();
    let diff = now.signed_duration_since(log.timestamp);
    
    // Relative time display
    let relative_time = if diff < Duration::minutes(1) {
        "just now".to_string()
    } else if diff < Duration::hours(1) {
        let mins = diff.num_minutes();
        format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" })
    } else if diff < Duration::hours(24) {
        let hours = diff.num_hours();
        format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
    } else {
        let days = diff.num_days();
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    };
    
    // Full timestamp for title/tooltip
    let full_time = log.timestamp.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S %Z").to_string();
    
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

            // Relative timestamp with full datetime in title
            span {
                class: "shrink-0 text-xs {theme::text::MUTED} min-w-[90px]",
                title: "{full_time}",
                "{relative_time}"
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
