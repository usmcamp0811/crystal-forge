//! Builder row component for table view - pixel-perfect JSX port.

use dioxus::prelude::*;

use crate::api::models::BuilderSummary;
use crate::components::{EnvBadge, Icon, IconName};

fn builder_status_chip(builder: &BuilderSummary) -> Element {
    let chip_class = builder.status.chip_class();
    let dot_color = builder.status.dot_color();
    let label = builder.status.label();

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
pub fn BuilderRow(builder: BuilderSummary, on_edit: EventHandler<()>) -> Element {
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

    // TODO: Load actual environments when backend provides them
    let environments: Vec<String> = vec![];

    // TODO: Load actual completed/failed 24h when backend provides them
    let completed24h = 0;
    let failed24h = 0;

    rsx! {
        tr {
            style: "cursor: pointer;",
            onclick: move |_| on_edit.call(()),

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
                    style: "font-size: 11px; display: flex; gap: 4px; flex-wrap: wrap; margin-top: 2px;",
                    for env in environments {
                        EnvBadge { name: env.clone() }
                    }
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
                        "{completed24h}"
                    }
                    if failed24h > 0 {
                        span {
                            style: "font-size: 11px; color: #f87171;",
                            "{failed24h} failed"
                        }
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
