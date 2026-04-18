//! Logs tab components for system detail view.
//!
//! Components for displaying deployment logs with log levels
//! and timestamps.

use dioxus::prelude::*;

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

/// Deployment logs tab showing recent log entries.
#[component]
pub fn LogsTab(logs: Vec<DeploymentLogEntry>) -> Element {
    if logs.is_empty() {
        return rsx! {
            div {
                class: "pt-6 text-center py-12",
                p {
                    class: "{theme::text::SECONDARY}",
                    "No agent events available."
                }
            }
        };
    }

    // Get the deployment phase for grouping
    let first_phase = logs
        .first()
        .and_then(|l| l.phase.clone())
        .unwrap_or_else(|| "Deployment".to_string());

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
                    "data-testid": "system-logs-list",
                    class: "font-mono text-sm divide-y divide-gray-800/50 max-h-[400px] overflow-y-auto",
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
