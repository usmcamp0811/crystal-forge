//! Builder card component displaying builder summary info.

use dioxus::prelude::*;

use crate::api::models::BuilderSummary;
use crate::theme;

#[component]
pub fn BuilderCard(builder: BuilderSummary, on_edit: EventHandler<()>) -> Element {
    let status_label = builder.status.label();
    let status_dot = builder.status.dot_class();
    let status_color = builder.status.color_class();

    let heartbeat_text = if let Some(heartbeat) = builder.last_heartbeat_at {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(heartbeat);

        if duration.num_seconds() < 60 {
            format!("{}s ago", duration.num_seconds())
        } else if duration.num_minutes() < 60 {
            format!("{}m ago", duration.num_minutes())
        } else if duration.num_hours() < 24 {
            format!("{}h ago", duration.num_hours())
        } else {
            format!("{}d ago", duration.num_days())
        }
    } else {
        "Never".to_string()
    };

    rsx! {
        div {
            class: "border border-slate-700 bg-slate-800/50 rounded-lg p-4 hover:border-slate-600 transition-colors",

            // Header
            div {
                class: "flex items-start justify-between mb-3",
                div {
                    class: "flex-1",
                    h3 {
                        class: "font-semibold text-white mb-1",
                        "{builder.name}"
                    }
                    div {
                        class: "flex items-center gap-2 text-xs",
                        span {
                            class: "flex items-center gap-1.5 {status_color}",
                            span { class: "w-2 h-2 rounded-full {status_dot}" }
                            "{status_label}"
                        }
                        span {
                            class: "text-slate-500",
                            "•"
                        }
                        span {
                            class: "text-slate-400",
                            "Heartbeat: {heartbeat_text}"
                        }
                    }
                }
                button {
                    class: "px-2 py-1 text-xs text-blue-400 hover:text-blue-300 hover:bg-blue-500/10 rounded transition-colors",
                    onclick: move |_| on_edit.call(()),
                    "Edit"
                }
            }

            // Resource Limits
            div {
                class: "space-y-2 text-sm",
                div {
                    class: "flex justify-between {theme::text::SECONDARY}",
                    span { "CPU Cores:" }
                    span {
                        class: "text-white",
                        if let Some(cores) = builder.max_cpu_cores {
                            "{cores}"
                        } else {
                            "Unlimited"
                        }
                    }
                }
                div {
                    class: "flex justify-between {theme::text::SECONDARY}",
                    span { "Memory:" }
                    span {
                        class: "text-white",
                        {
                            if let Some(mem_mb) = builder.max_memory_mb {
                                format!("{} GB", mem_mb / 1024)
                            } else {
                                "Unlimited".to_string()
                            }
                        }
                    }
                }
                div {
                    class: "flex justify-between {theme::text::SECONDARY}",
                    span { "Max Jobs:" }
                    span {
                        class: "text-white",
                        "{builder.max_concurrent_jobs}"
                    }
                }
                div {
                    class: "flex justify-between {theme::text::SECONDARY}",
                    span { "Environments:" }
                    span {
                        class: "text-white",
                        {
                            if builder.assigned_environment_count > 0 {
                                format!("{}", builder.assigned_environment_count)
                            } else {
                                "All (wildcard)".to_string()
                            }
                        }
                    }
                }
            }
        }
    }
}
