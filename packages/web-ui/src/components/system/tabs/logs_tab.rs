//! Logs tab components for system detail view.
//!
//! Components for displaying deployment logs with log levels,
//! timestamps, day delineation, and time filtering.

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Utc};
use dioxus::prelude::*;
use std::collections::BTreeMap;

use crate::api::models::{DeploymentLogEntry, LogLevel};
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LevelFilter {
    All,
    Info,
    Warn,
    Error,
    Debug,
}

impl LevelFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Info => "Info",
            Self::Warn => "Warn",
            Self::Error => "Error",
            Self::Debug => "Debug",
        }
    }

    fn matches(self, level: LogLevel) -> bool {
        match self {
            Self::All => true,
            Self::Info => matches!(level, LogLevel::Info),
            Self::Warn => matches!(level, LogLevel::Warn),
            Self::Error => matches!(level, LogLevel::Error),
            Self::Debug => matches!(level, LogLevel::Debug),
        }
    }
}

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

    // Filters / view state
    let mut level_filter = use_signal(|| LevelFilter::All);
    let mut phase_filter = use_signal(|| "all".to_string());
    let mut search_query = use_signal(String::new);
    let mut show_full_logs = use_signal(|| false);

    // Distinct event types/phases for phase filter control
    let mut phases: Vec<String> = logs.iter().filter_map(|l| l.phase.clone()).collect();
    phases.sort();
    phases.dedup();

    let query = search_query.read().trim().to_ascii_lowercase();
    let selected_phase = phase_filter.read().clone();
    let filtered_logs: Vec<DeploymentLogEntry> = logs
        .iter()
        .filter(|log| {
            let level_ok = level_filter.read().matches(log.level);
            let phase_ok = if selected_phase == "all" {
                true
            } else {
                log.phase
                    .as_deref()
                    .map(|phase| phase == selected_phase)
                    .unwrap_or(false)
            };
            let query_ok = if query.is_empty() {
                true
            } else {
                let message = log.message.to_ascii_lowercase();
                let phase = log
                    .phase
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                message.contains(&query) || phase.contains(&query)
            };

            level_ok && phase_ok && query_ok
        })
        .cloned()
        .collect();

    let has_active_filters =
        *level_filter.read() != LevelFilter::All || selected_phase != "all" || !query.is_empty();

    // Group filtered logs by day
    let logs_by_day = group_logs_by_day(&filtered_logs);

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
                button {
                    "data-testid": "system-logs-view-full",
                    class: "text-sm text-blue-400 hover:text-blue-300 transition-colors",
                    onclick: move |_| show_full_logs.set(true),
                    "View full logs →"
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

            // Filters
            div {
                class: "mb-4 rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} p-3 space-y-3",

                div {
                    class: "flex flex-wrap items-center gap-3",
                    div {
                        class: "flex items-center gap-2",
                        label { class: "text-xs uppercase tracking-wider {theme::text::MUTED}", "Severity" }
                        select {
                            "data-testid": "system-logs-filter-severity",
                            class: "text-sm rounded border border-gray-700 bg-gray-900/60 text-gray-100 px-2 py-1",
                            value: "{level_filter.read().label().to_ascii_lowercase()}",
                            onchange: move |evt| {
                                let next = match evt.value().as_str() {
                                    "info" => LevelFilter::Info,
                                    "warn" => LevelFilter::Warn,
                                    "error" => LevelFilter::Error,
                                    "debug" => LevelFilter::Debug,
                                    _ => LevelFilter::All,
                                };
                                level_filter.set(next);
                            },
                            option { value: "all", "All severities" }
                            option { value: "info", "Info" }
                            option { value: "warn", "Warn" }
                            option { value: "error", "Error" }
                            option { value: "debug", "Debug" }
                        }
                    }

                    if *level_filter.read() != LevelFilter::All {
                        span {
                            "data-testid": "system-logs-severity-active-pill",
                            class: "px-2 py-1 text-xs rounded border border-cyan-400 bg-cyan-500/15 text-cyan-300",
                            "Severity: {level_filter.read().label()}"
                        }
                    }
                }

                div {
                    class: "flex flex-wrap items-center gap-3",
                    div {
                        class: "flex items-center gap-2",
                        label { class: "text-xs uppercase tracking-wider {theme::text::MUTED}", "Event type" }
                        select {
                            "data-testid": "system-logs-filter-event-type",
                            class: "text-sm rounded border border-gray-700 bg-gray-900/60 text-gray-100 px-2 py-1",
                            value: "{selected_phase}",
                            onchange: move |evt| phase_filter.set(evt.value()),
                            option { value: "all", "All event types" }
                            for phase in phases.iter() {
                                option { value: "{phase}", "{phase}" }
                            }
                        }
                    }

                    div {
                        class: "flex items-center gap-2 flex-1 min-w-[220px]",
                        label { class: "text-xs uppercase tracking-wider {theme::text::MUTED}", "Search" }
                        input {
                            "data-testid": "system-logs-filter-search",
                            class: "w-full text-sm rounded border border-gray-700 bg-gray-900/60 text-gray-100 px-2 py-1 placeholder:text-gray-500",
                            r#type: "text",
                            value: "{search_query}",
                            placeholder: "Filter log text...",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                    }
                }

                div {
                    class: "flex items-center justify-between",
                    p {
                        "data-testid": "system-logs-filter-count",
                        class: "text-xs {theme::text::MUTED}",
                        "Showing {filtered_logs.len()} of {logs.len()} log entries"
                    }
                    if has_active_filters {
                        button {
                            "data-testid": "system-logs-filter-reset",
                            class: "text-xs text-cyan-300 hover:text-cyan-200 transition-colors",
                            onclick: move |_| {
                                level_filter.set(LevelFilter::All);
                                phase_filter.set("all".to_string());
                                search_query.set(String::new());
                            },
                            "Clear filters"
                        }
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
                        "data-testid": "system-logs-list",
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

                        if filtered_logs.is_empty() {
                            div {
                                class: "px-4 py-6 text-center text-sm {theme::text::SECONDARY}",
                                "No logs match the current filters."
                            }
                        }
                    }
                }
            }

            if *show_full_logs.read() {
                div {
                    class: "fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4",
                    "data-testid": "system-logs-full-modal",
                    div {
                        class: "w-full max-w-6xl max-h-[85vh] rounded-xl border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} overflow-hidden shadow-2xl",
                        div {
                            class: "px-4 py-3 border-b border-gray-700 flex items-center justify-between",
                            h4 { class: "text-sm font-semibold text-white", "Full Agent Event Log" }
                            button {
                                class: "text-sm text-gray-300 hover:text-white",
                                onclick: move |_| show_full_logs.set(false),
                                "Close"
                            }
                        }
                        div {
                            class: "font-mono text-sm divide-y divide-gray-800/50 overflow-y-auto max-h-[70vh]",
                            for log in filtered_logs.iter() {
                                LogLine { log: log.clone() }
                            }
                            if filtered_logs.is_empty() {
                                div {
                                    class: "px-4 py-6 text-center text-sm {theme::text::SECONDARY}",
                                    "No logs match the current filters."
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
