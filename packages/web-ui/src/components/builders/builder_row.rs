//! Builder row component for table view - pixel-perfect JSX port.

use dioxus::prelude::*;

use crate::api::models::BuilderSummary;
use crate::components::{Icon, IconName};

fn builder_status_chip(builder: &BuilderSummary) -> Element {
    // If disabled, override status display
    let (chip_class, dot_color, label) = if !builder.enabled {
        ("chip-warning", "#fbbf24", "disabled")
    } else {
        (
            builder.status.chip_class(),
            builder.status.dot_color(),
            builder.status.label(),
        )
    };

    rsx! {
        span {
            class: "chip {chip_class}",
            span {
                class: "chip-dot",
                style: "background: {dot_color};"
            }
            "{label}"
        }
    }
}

#[component]
pub fn BuilderRow(
    builder: BuilderSummary,
    can_manage: bool,
    on_open: EventHandler<()>,
    on_edit: EventHandler<()>,
) -> Element {
    let slot_pct = if builder.max_concurrent_jobs > 0 {
        ((builder.active_jobs as f64 / builder.max_concurrent_jobs as f64) * 100.0).round() as i32
    } else {
        0
    };

    let heartbeat_text = if let Some(heartbeat) = builder.last_heartbeat_at {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(heartbeat);

        if duration.num_seconds() < 60 {
            "just now".to_string()
        } else if duration.num_minutes() < 60 {
            format!("{}m ago", duration.num_minutes())
        } else if duration.num_hours() < 24 {
            format!("{}h ago", duration.num_hours())
        } else {
            format!("{}d ago", duration.num_days())
        }
    } else {
        "never".to_string()
    };

    let cores_text = builder
        .max_cpu_cores
        .map(|c| c.to_string())
        .unwrap_or_else(|| "∞".to_string());

    let mem_text = builder
        .max_memory_mb
        .map(|mb| format!("{} GiB", mb / 1024))
        .unwrap_or_else(|| "∞".to_string());

    let environments_text = if builder.assigned_environment_count > 0 {
        format!("{} assigned", builder.assigned_environment_count)
    } else {
        "All / wildcard".to_string()
    };

    rsx! {
        tr {
            style: if can_manage { "cursor: pointer;" } else { "cursor: default;" },
            onclick: move |_| {
                if can_manage {
                    on_open.call(())
                }
            },

            // Builder name + host
            td {
                div {
                    style: "font-weight: 600; font-size: 13px;",
                    "{builder.name}"
                }
                div {
                    class: "mono",
                    style: "font-size: 11px; color: var(--cf-text-muted);",
                    {builder.host.as_deref().unwrap_or("")}
                }
            }

            // Status chip
            td {
                {builder_status_chip(&builder)}
            }

            // Arch + environments
            td {
                div {
                    class: "mono",
                    style: "font-size: 12px;",
                    "{builder.arch}"
                }
                div {
                    style: "font-size: 11px; margin-top: 2px; color: var(--cf-text-muted);",
                    "{environments_text}"
                }
            }

            // Resources (cores + mem)
            td {
                class: "mono",
                style: "font-size: 12px;",
                "{cores_text}c · {mem_text}"
            }

            // Slot use (progress bar + text)
            td {
                div {
                    style: "display: flex; align-items: center; gap: 8px; min-width: 130px;",
                    div {
                        style: "height: 4px; flex: 1; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                        {
                            let slot_bg = if slot_pct > 85 { "#fbbf24" } else { "#34d399" };
                            rsx! {
                                div {
                                    style: "width: {slot_pct}%; height: 100%; background: {slot_bg};"
                                }
                            }
                        }
                    }
                    span {
                        class: "mono",
                        style: "font-size: 11px; color: var(--cf-text-muted);",
                        "{builder.active_jobs}/{builder.max_concurrent_jobs}"
                    }
                }
            }

            // Built 24h
            td {
                div {
                    style: "display: flex; flex-direction: column; gap: 1px;",
                    span {
                        class: "mono",
                        style: "font-size: 12px;",
                        "—"
                    }
                }
            }

            // Last seen
            td {
                style: "font-size: 12px; color: var(--cf-text-muted);",
                "{heartbeat_text}"
            }

            // Edit button
            td {
                div {
                    class: "row-actions",
                    if can_manage {
                        button {
                            class: "btn-icon focus-ring",
                            title: "Edit",
                            onclick: move |e| {
                                e.stop_propagation();
                                on_edit.call(())
                            },
                            Icon { name: IconName::Gear, size: 14 }
                        }
                    }
                }
            }
        }
    }
}
