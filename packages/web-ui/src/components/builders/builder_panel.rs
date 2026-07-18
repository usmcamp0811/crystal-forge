//! Builder side panel — peek detail with edit/register handoff.
//! Pixel-perfect JSX port matching BuildersView.jsx BuilderPanel.

use dioxus::prelude::*;

use crate::api::models::BuilderSummary;
use crate::components::{EnvBadge, Icon, IconName};

fn builder_status_chip(builder: &BuilderSummary) -> Element {
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
pub fn BuilderPanel(
    builder: BuilderSummary,
    on_close: EventHandler<()>,
    on_edit: EventHandler<()>,
) -> Element {
    let slot_pct = if builder.max_concurrent_jobs > 0 {
        ((builder.active_jobs as f64 / builder.max_concurrent_jobs as f64) * 100.0).round() as i32
    } else {
        0
    };

    let load_pct = 0; // Load percentage not available from BuilderSummary; JSX design uses w.load

    let slot_bar_color = if slot_pct > 85 { "#fbbf24" } else { "#34d399" };
    let load_bar_color = if load_pct > 85 {
        "#f87171"
    } else if load_pct > 60 {
        "#fbbf24"
    } else {
        "#60a5fa"
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

    let has_environments = !builder.assigned_environments.is_empty();

    rsx! {
        // Backdrop
        div {
            class: "side-panel-backdrop",
            onclick: move |_| on_close.call(())
        }
        aside {
            class: "side-panel",
            role: "dialog",
            "aria-modal": "true",

            // Panel head
            div {
                class: "panel-head",
                div {
                    class: "panel-title",
                    h2 {
                        span { style: "opacity: 0.7;", Icon { name: IconName::Cpu, size: 14 } }
                        " {builder.name}"
                    }
                    span {
                        class: "fqdn mono",
                        {builder.host.as_deref().unwrap_or("")}
                    }
                }
                button {
                    class: "btn-icon focus-ring",
                    "aria-label": "Close",
                    onclick: move |_| on_close.call(()),
                    Icon { name: IconName::X, size: 16 }
                }
            }

            // Panel body
            div {
                class: "panel-body",

                // Status section
                section {
                    class: "panel-section",
                    div {
                        style: "display: flex; gap: 8px; flex-wrap: wrap;",
                        {builder_status_chip(&builder)}
                        span {
                            class: "chip chip-unknown mono",
                            "{builder.arch}"
                        }
                        if !builder.registered {
                            span {
                                class: "chip chip-warning",
                                "unregistered"
                            }
                        }
                    }
                }

                // Unregistered warning banner
                if !builder.registered {
                    section {
                        class: "panel-section",
                        div {
                            class: "builder-pending-banner",
                            style: "flex-direction: column; align-items: stretch; gap: 6px;",
                            span {
                                style: "display: flex; align-items: center; gap: 7px;",
                                Icon { name: IconName::Warn, size: 12 }
                                span {
                                    "Connected but "
                                    strong { "not registered" }
                                    " — match this key to recognize it."
                                }
                            }
                            // Fingerprint chip
                            button {
                                class: "builder-id-chip focus-ring",
                                title: "Copy key fingerprint",
                                onclick: move |_| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        use wasm_bindgen::JsCast;
                                        if let Some(window) = web_sys::window() {
                                            if let Some(clipboard) = window.navigator().clipboard() {
                                                let _ = clipboard.write_text(&builder.public_key_fingerprint);
                                            }
                                        }
                                    }
                                },
                                Icon { name: IconName::Key, size: 10 }
                                span {
                                    class: "mono",
                                    "{builder.public_key_fingerprint}"
                                }
                                Icon { name: IconName::File, size: 10, }
                            }
                        }
                    }
                }

                // Slot use section
                section {
                    class: "panel-section",
                    h3 { "Slot use" }
                    div {
                        style: "display: flex; justify-content: space-between; font-size: 11px; color: var(--cf-text-muted); margin-bottom: 4px;",
                        span { "{builder.active_jobs}/{builder.max_concurrent_jobs} slots" }
                        span { class: "mono", "{slot_pct}%" }
                    }
                    div {
                        style: "height: 6px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden; margin-bottom: 12px;",
                        div {
                            style: "width: {slot_pct}%; height: 100%; background: {slot_bar_color};"
                        }
                    }
                    div {
                        style: "display: flex; justify-content: space-between; font-size: 11px; color: var(--cf-text-muted); margin-bottom: 4px;",
                        span { "Load" }
                        span { class: "mono", "—" }
                    }
                    div {
                        style: "height: 6px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                        div {
                            style: "width: 0%; height: 100%; background: {load_bar_color};"
                        }
                    }
                }

                // Details section
                section {
                    class: "panel-section",
                    h3 { "Details" }
                    dl {
                        class: "kv-grid",
                        dt { "Cores · mem" }
                        dd {
                            class: "mono",
                            "{cores_text}c · {mem_text}"
                        }
                        dt { "Built 24h" }
                        dd {
                            class: "mono",
                            "—"
                        }
                        dt { "Last seen" }
                        dd { "{heartbeat_text}" }
                    }
                }

                // Environments section
                section {
                    class: "panel-section",
                    h3 { "Environments ({builder.assigned_environments.len()})" }
                    div {
                        style: "display: flex; gap: 6px; flex-wrap: wrap;",
                        if has_environments {
                            for env in &builder.assigned_environments {
                                EnvBadge {
                                    name: env.name.clone(),
                                    fg: Some(env.color_hex.clone()),
                                }
                            }
                        } else {
                            span {
                                style: "font-size: 12px; color: var(--cf-text-muted);",
                                "none assigned"
                            }
                        }
                    }
                }
            }

                // Panel actions
                div {
                    class: "panel-actions",
                    button {
                        class: "btn btn-primary focus-ring",
                        onclick: move |_| on_edit.call(()),
                        if builder.registered {
                            Icon { name: IconName::Gear, size: 12 }
                            " Edit builder"
                        } else {
                            Icon { name: IconName::Key, size: 12 }
                            " Register"
                        }
                    }
                }
        }
    }
}
