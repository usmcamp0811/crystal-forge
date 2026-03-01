//! Builder card component displaying builder summary info.

use dioxus::prelude::*;

use crate::api::models::BuilderSummary;
use crate::theme;

#[component]
pub fn BuilderCard(builder: BuilderSummary, on_edit: EventHandler<()>) -> Element {
    let status_label = builder.status.label();
    let status_dot = builder.status.dot_class();
    let status_color = builder.status.color_class();
    let is_inactive = matches!(builder.status, crate::api::models::BuilderStatus::Inactive);

    let inactive_classes = if is_inactive {
        "opacity-60 saturate-0"
    } else {
        ""
    };

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

    let cpu_cores_text = if let Some(cores) = builder.max_cpu_cores {
        cores.to_string()
    } else {
        "Unlimited".to_string()
    };

    let memory_text = if let Some(mem_mb) = builder.max_memory_mb {
        format!("{} GB", mem_mb / 1024)
    } else {
        "Unlimited".to_string()
    };

    let environments_text = if builder.assigned_environment_count > 0 {
        builder.assigned_environment_count.to_string()
    } else {
        "All (wildcard)".to_string()
    };

    rsx! {
        div {
            class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm {inactive_classes}",

            // Header section with purple gradient background
            div {
                class: "flex items-center justify-between px-6 py-4 border-b border-gray-800",
                style: "background: linear-gradient(135deg, rgba(147, 51, 234, 0.42) 0%, rgba(17, 24, 39, 0.92) 100%);",
                div {
                    class: "flex-1",
                    h3 {
                        class: "text-lg font-semibold text-white mb-1",
                        "{builder.name}"
                    }
                    div {
                        class: "flex items-center gap-2 text-xs",
                        span {
                            class: "flex items-center gap-1.5 {status_color}",
                            span { class: "w-2 h-2 rounded-full {status_dot}" }
                            "{status_label}"
                        }
                    }
                }
                button {
                    class: "px-4 py-2 rounded text-xs font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                    onclick: move |_| on_edit.call(()),
                    "Edit"
                }
            }

            // Status section
            div {
                class: "px-6 py-3 bg-gray-800/50",
                p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500 mb-2", "Status" }
                div {
                    class: "text-sm {theme::text::SECONDARY}",
                    "Last heartbeat: "
                    span { class: "text-white", "{heartbeat_text}" }
                }
            }

            // Resource Limits section
            div {
                class: "px-6 py-3 bg-gray-900",
                p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500 mb-3", "Resource Limits" }
                div {
                    class: "grid grid-cols-2 gap-3 text-sm",
                    div {
                        span { class: "text-gray-500 text-xs block mb-0.5", "CPU Cores" }
                        span {
                            class: "text-gray-200",
                            "{cpu_cores_text}"
                        }
                    }
                    div {
                        span { class: "text-gray-500 text-xs block mb-0.5", "Memory" }
                        span {
                            class: "text-gray-200",
                            "{memory_text}"
                        }
                    }
                    div {
                        span { class: "text-gray-500 text-xs block mb-0.5", "Max Concurrent Jobs" }
                        span {
                            class: "text-gray-200",
                            "{builder.max_concurrent_jobs}"
                        }
                    }
                    div {
                        span { class: "text-gray-500 text-xs block mb-0.5", "Environments" }
                        span {
                            class: "text-gray-200",
                            "{environments_text}"
                        }
                    }
                }
            }
        }
    }
}
