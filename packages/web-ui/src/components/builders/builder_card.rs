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
    let card_class = if is_inactive {
        "{theme::presets::CARD} opacity-65 saturate-0"
    } else {
        "{theme::presets::CARD}"
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

    rsx! {
        div {
            class: "{card_class} transition-colors",

            // Header
            div {
                class: "flex items-start justify-between mb-3",
                div {
                    class: "flex-1",
                    h3 {
                        class: "font-semibold {theme::text::PRIMARY} mb-1",
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
                            class: "{theme::text::MUTED}",
                            "•"
                        }
                        span {
                            class: "{theme::text::SECONDARY}",
                            "Heartbeat: {heartbeat_text}"
                        }
                    }
                }
                button {
                    class: "px-2 py-1 rounded text-xs font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
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
                        {
                            if let Some(cores) = builder.max_cpu_cores {
                                cores.to_string()
                            } else {
                                "Unlimited".to_string()
                            }
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
