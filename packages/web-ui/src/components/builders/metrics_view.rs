//! Builder metrics dashboard showing resource usage.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::{self, models::BuilderMetrics};
use crate::components::loading::LoadingSpinner;
use crate::theme;

#[component]
pub fn BuilderMetricsView() -> Element {
    // Fetch all builders
    let builders = use_resource(|| async move {
        api::client::fetch_builders().await
    });

    rsx! {
        div {
            class: "space-y-6",

            // Header
            div {
                h2 {
                    class: "{theme::typography::SECTION_TITLE}",
                    "Builder Metrics"
                }
                p {
                    class: "text-sm {theme::text::SECONDARY} mt-1",
                    "Real-time resource usage across all builders"
                }
            }

            // Metrics grid
            match &*builders.read_unchecked() {
                Some(Ok(builder_list)) => rsx! {
                    if builder_list.is_empty() {
                        div {
                            class: "text-center py-12 border border-dashed border-slate-700 rounded-lg",
                            p {
                                class: "text-slate-400",
                                "No builders available for metrics"
                            }
                        }
                    } else {
                        div {
                            class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                            for builder in builder_list {
                                BuilderMetricsCard {
                                    key: "{builder.id}",
                                    builder_id: builder.id,
                                    builder_name: builder.name.clone(),
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div {
                        class: "border border-red-500/30 bg-red-500/10 rounded-lg p-4",
                        p {
                            class: "text-red-400",
                            "⚠️ Failed to load builders: {e}"
                        }
                    }
                },
                None => rsx! {
                    LoadingSpinner {}
                },
            }
        }
    }
}

#[component]
fn BuilderMetricsCard(builder_id: Uuid, builder_name: String) -> Element {
    let metrics = use_resource(move || async move {
        api::client::fetch_builder_metrics(&builder_id).await
    });

    rsx! {
        div {
            class: "border border-slate-700 bg-slate-800/50 rounded-lg p-4",

            // Card header
            div {
                class: "mb-4",
                h3 {
                    class: "font-semibold text-white",
                    "{builder_name}"
                }
                p {
                    class: "text-xs {theme::text::SECONDARY}",
                    "Builder ID: {builder_id}"
                }
            }

            // Metrics display
            match &*metrics.read_unchecked() {
                Some(Ok(metrics_list)) => rsx! {
                    if metrics_list.is_empty() {
                        p {
                            class: "text-sm {theme::text::SECONDARY}",
                            "No metrics available"
                        }
                    } else {
                        // Get the most recent metric
                        {
                            let latest = &metrics_list[0];
                            rsx! {
                                div {
                                    class: "space-y-3",

                                    // CPU Usage
                                    div {
                                        div {
                                            class: "flex justify-between text-sm mb-1",
                                            span {
                                                class: "{theme::text::SECONDARY}",
                                                "CPU Usage"
                                            }
                                            span {
                                                class: "text-white font-medium",
                                                "{latest.cpu_usage_percent:.1}%"
                                            }
                                        }
                                        div {
                                            class: "w-full bg-slate-700 rounded-full h-2",
                                            div {
                                                class: "bg-blue-500 h-2 rounded-full transition-all",
                                                style: "width: {latest.cpu_usage_percent:.1}%",
                                            }
                                        }
                                    }

                                    // Memory Usage
                                    div {
                                        div {
                                            class: "flex justify-between text-sm mb-1",
                                            span {
                                                class: "{theme::text::SECONDARY}",
                                                "Memory Usage"
                                            }
                                            span {
                                                class: "text-white font-medium",
                                                "{format_memory(latest.memory_usage_mb)}"
                                            }
                                        }
                                        {
                                            if let (Some(total), Some(used)) = (latest.system_memory_total_mb, latest.system_memory_used_mb) {
                                                let percent = (used as f64 / total as f64) * 100.0;
                                                rsx! {
                                                    div {
                                                        class: "w-full bg-slate-700 rounded-full h-2",
                                                        div {
                                                            class: "bg-emerald-500 h-2 rounded-full transition-all",
                                                            style: "width: {percent:.1}%",
                                                        }
                                                    }
                                                }
                                            } else {
                                                rsx! {
                                                    div {
                                                        class: "w-full bg-slate-700 rounded-full h-2",
                                                        div {
                                                            class: "bg-emerald-500 h-2 rounded-full",
                                                            style: "width: 0%",
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // System stats (if available)
                                    if let Some(sys_cpu) = latest.system_cpu_usage_percent {
                                        div {
                                            class: "flex justify-between text-xs {theme::text::SECONDARY}",
                                            span { "System CPU:" }
                                            span { "{sys_cpu:.1}%" }
                                        }
                                    }

                                    if let (Some(total), Some(used)) = (latest.system_memory_total_mb, latest.system_memory_used_mb) {
                                        div {
                                            class: "flex justify-between text-xs {theme::text::SECONDARY}",
                                            span { "System Memory:" }
                                            span { "{format_memory(used)} / {format_memory(total)}" }
                                        }
                                    }

                                    // Timestamp
                                    div {
                                        class: "pt-2 border-t border-slate-700",
                                        p {
                                            class: "text-xs {theme::text::SECONDARY}",
                                            "Last updated: {format_timestamp(&latest.timestamp)}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    p {
                        class: "text-sm text-red-400",
                        "Failed to load metrics: {e}"
                    }
                },
                None => rsx! {
                    div {
                        class: "py-4",
                        LoadingSpinner {}
                    }
                },
            }
        }
    }
}

fn format_memory(mb: i64) -> String {
    if mb >= 1024 {
        format!("{:.1} GB", mb as f64 / 1024.0)
    } else {
        format!("{} MB", mb)
    }
}

fn format_timestamp(timestamp: &chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(*timestamp);
    
    if duration.num_seconds() < 60 {
        format!("{}s ago", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else {
        format!("{}d ago", duration.num_days())
    }
}
