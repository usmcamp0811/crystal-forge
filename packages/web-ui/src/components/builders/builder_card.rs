//! Builder card component - pixel-perfect JSX port matching BuildersView.jsx.

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
pub fn BuilderCard(
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

    let rail_color = if !builder.enabled {
        "#fbbf24"
    } else {
        match builder.status {
            crate::api::models::BuilderStatus::Active => "#34d399",
            crate::api::models::BuilderStatus::Inactive => "#fbbf24",
            _ => "#f87171",
        }
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
        div {
            class: "sys-card",
            onclick: move |_| on_open.call(()),
            div {
                class: "status-rail",
                style: "--status-color: {rail_color};"
            }
            div {
                class: "sys-card-head",
                div {
                    class: "sys-title",
                    div {
                        class: "sys-hostname",
                        Icon { name: IconName::Cpu, size: 13 }
                        " {builder.name}"
                    }
                    div {
                        class: "sys-fqdn",
                        {builder.host.as_deref().unwrap_or("")}
                    }
                }
                {builder_status_chip(&builder)}
            }
            div {
                class: "sys-card-body",
                div {
                    div { class: "sys-kv-key", "Arch" }
                    div { class: "sys-kv-val", "{builder.arch}" }
                }
                div {
                    div { class: "sys-kv-key", "Cores · mem" }
                    div {
                        class: "sys-kv-val",
                        style: "font-family: inherit;",
                        "{cores_text}c · {mem_text}"
                    }
                }
                div {
                    div { class: "sys-kv-key", "Environments" }
                    div {
                        class: "sys-kv-val",
                        style: "font-family: inherit;",
                        "{environments_text}"
                    }
                }
                div {
                    div { class: "sys-kv-key", "Last seen" }
                    div {
                        class: "sys-kv-val",
                        style: "font-family: inherit;",
                        "{heartbeat_text}"
                    }
                }
            }

            // Slot use progress bar
            div {
                div {
                    style: "display: flex; justify-content: space-between; font-size: 11px; color: var(--cf-text-muted); margin-bottom: 4px;",
                    span { "Slot use" }
                    span {
                        class: "mono",
                        "{builder.active_jobs}/{builder.max_concurrent_jobs} · {slot_pct}%"
                    }
                }
                div {
                    style: "height: 5px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                    {
                        let slot_bg = if slot_pct > 85 { "#fbbf24" } else { "#34d399" };
                        rsx! {
                            div {
                                style: "width: {slot_pct}%; height: 100%; background: {slot_bg};"
                            }
                        }
                    }
                }
            }

            // Load progress bar
            div {
                div {
                    style: "display: flex; justify-content: space-between; font-size: 11px; color: var(--cf-text-muted); margin-bottom: 4px;",
                    span { "Load" }
                    span { class: "mono", "—" }
                }
                div {
                    style: "height: 5px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                    div { style: "width: 0%; height: 100%; background: #60a5fa;" }
                }
            }

            // Footer
            div {
                class: "sys-card-foot",
                div {
                    class: "chips-row",
                    span {
                        class: "chip chip-info",
                        "24h metrics unavailable"
                    }
                }
                if can_manage {
                    button {
                        class: "btn btn-subtle focus-ring",
                        style: "padding: 4px 10px; font-size: 12px;",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_edit.call(())
                        },
                        Icon { name: IconName::Gear, size: 12 }
                        " Edit"
                    }
                }
            }
        }
    }
}
