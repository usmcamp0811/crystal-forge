//! Redesigned system card component matching the design system.
//!
//! Implements the refined card layout from the design example with:
//! - Status rail indicator
//! - Environment badges with custom colors
//! - Two-column metadata layout
//! - Chip-based status indicators
//! - Enhanced hover states

use dioxus::prelude::*;

use crate::api::models::{HealthStatus, SystemSummary};
use crate::components::chips::{Chip, ChipVariant, EnvBadge, StatusDot};
use crate::components::heartbeat_spinner::HeartbeatSpinner;

/// Environment color configuration for badges and styling.
struct EnvColors {
    fg: &'static str,
    bg: &'static str,
    border: &'static str,
}

fn env_colors(env_name: &str) -> EnvColors {
    match env_name.to_lowercase().as_str() {
        "production" | "prod" => EnvColors {
            fg: "#f87171",
            bg: "rgba(220,38,38,0.10)",
            border: "rgba(248,113,113,0.25)",
        },
        "staging" | "stage" => EnvColors {
            fg: "#fbbf24",
            bg: "rgba(217,119,6,0.10)",
            border: "rgba(251,191,36,0.25)",
        },
        "dev" | "development" => EnvColors {
            fg: "#60a5fa",
            bg: "rgba(37,99,235,0.10)",
            border: "rgba(96,165,250,0.25)",
        },
        "edge" => EnvColors {
            fg: "#2dd4bf",
            bg: "rgba(15,118,110,0.12)",
            border: "rgba(45,212,191,0.25)",
        },
        "lab" => EnvColors {
            fg: "#a78bfa",
            bg: "rgba(124,58,237,0.10)",
            border: "rgba(167,139,250,0.25)",
        },
        _ => EnvColors {
            fg: "#6b7280",
            bg: "rgba(107,114,128,0.16)",
            border: "rgba(107,114,128,0.25)",
        },
    }
}

fn status_color(health: &HealthStatus) -> &'static str {
    match health {
        HealthStatus::Healthy => "#34d399",
        HealthStatus::Warning => "#fbbf24",
        HealthStatus::Critical => "#f87171",
        HealthStatus::Offline => "#6b7280",
    }
}

fn health_chip_variant(health: &HealthStatus) -> ChipVariant {
    match health {
        HealthStatus::Healthy => ChipVariant::Healthy,
        HealthStatus::Warning => ChipVariant::Warning,
        HealthStatus::Critical => ChipVariant::Critical,
        HealthStatus::Offline => ChipVariant::Unknown,
    }
}

fn deployment_chip_variant(status: &crate::api::models::DeploymentStatus) -> ChipVariant {
    use crate::api::models::DeploymentStatus;
    match status {
        DeploymentStatus::UpToDate => ChipVariant::Healthy,
        DeploymentStatus::Behind => ChipVariant::Warning,
        DeploymentStatus::Ahead => ChipVariant::Info,
        DeploymentStatus::NeverDeployed | DeploymentStatus::NoCommitsAvailable => {
            ChipVariant::Unknown
        }
        DeploymentStatus::Unknown => ChipVariant::Unknown,
    }
}

/// Redesigned system card with modern styling and layout.
#[component]
pub fn SystemCardV2(
    system: SystemSummary,
    #[props(default = false)] compact: bool,
    on_open: EventHandler<()>,
    on_remove: EventHandler<()>,
    on_update_key: EventHandler<()>,
    on_edit: EventHandler<()>,
    on_deploy: EventHandler<()>,
) -> Element {
    let environment = system
        .environment
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    // Debug: log environment value to console
    #[cfg(debug_assertions)]
    {
        let msg = format!(
            "SystemCardV2: hostname={}, environment='{}' (raw: {:?})",
            system.hostname, environment, system.environment
        );
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&msg));
    }

    let env = env_colors(&environment);
    let status_col = status_color(&system.health_status);

    // Get flake info
    // TODO: Backend needs to include flake name in SystemSummary/view_system_list
    let flake_name = system
        .flake_id
        .map(|id| format!("flake-{}", id))
        .unwrap_or_else(|| "unknown".to_string());

    // Format last seen as relative time
    let last_seen = system
        .last_seen
        .map(|dt| {
            let now = chrono::Utc::now();
            let diff = now.signed_duration_since(dt);
            if diff.num_seconds() < 60 {
                format!("{}s ago", diff.num_seconds().max(0))
            } else if diff.num_minutes() < 60 {
                format!("{}m ago", diff.num_minutes().max(0))
            } else if diff.num_hours() < 24 {
                format!("{}h ago", diff.num_hours().max(0))
            } else {
                format!("{}d ago", diff.num_days().max(0))
            }
        })
        .unwrap_or_else(|| "Never".to_string());

    let heartbeat_interval_sec = 60_i64;
    let heartbeat_next_in_sec = system
        .last_seen
        .map(|dt| {
            let elapsed = chrono::Utc::now().signed_duration_since(dt).num_seconds() as f64;
            heartbeat_interval_sec as f64 - elapsed
        })
        .unwrap_or(0.0);

    let compact_class = if compact { " compact" } else { "" };

    rsx! {
        div {
            class: "sys-card{compact_class}",
            onclick: move |_| {
                on_open.call(());
            },

            // Status rail (colored left edge)
            span {
                class: "status-rail",
                style: "--status-color: {status_col}"
            }

            // Card head with hostname and environment badge
            div {
                class: "flex items-start justify-between gap-3",
                div {
                    class: "min-w-0 flex-1",
                    div {
                        class: "flex items-center gap-2 mb-1",
                        StatusDot {
                            color: status_col.to_string(),
                            large: false,
                        }
                        h3 {
                            class: "text-sm font-semibold",
                            style: "color: var(--cf-text-primary)",
                            "{system.hostname}"
                        }
                    }
                    div {
                        class: "text-xs mono",
                        style: "color: var(--cf-text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                        "{system.hostname}.local"
                    }
                }
                EnvBadge {
                    name: environment.clone(),
                    fg: env.fg.to_string(),
                    bg: env.bg.to_string(),
                    border: env.border.to_string(),
                }
            }

            // Card body with two-column metadata
            if !compact {
                div {
                    class: "grid grid-cols-2 gap-x-4 gap-y-3 text-xs",
                    // Flake · branch
                    div {
                        div {
                            class: "text-[11px] uppercase tracking-wider mb-1",
                            style: "color: var(--cf-text-muted); letter-spacing: 0.06em;",
                            "Flake · branch"
                        }
                        div {
                            class: "mono text-xs",
                            style: "color: var(--cf-text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                            "{flake_name}"
                        }
                    }
                    // Commit
                    div {
                        div {
                            class: "text-[11px] uppercase tracking-wider mb-1",
                            style: "color: var(--cf-text-muted); letter-spacing: 0.06em;",
                            "Commit"
                        }
                        div {
                            class: "mono text-xs",
                            style: "color: var(--cf-text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                            "—"
                        }
                    }
                    // Heartbeat
                    div {
                        div {
                            class: "text-[11px] uppercase tracking-wider mb-1",
                            style: "color: var(--cf-text-muted); letter-spacing: 0.06em;",
                            "Heartbeat"
                        }
                        div {
                            class: "text-xs",
                            style: "color: var(--cf-text-primary); display: flex; align-items: center; gap: 8px;",
                            HeartbeatSpinner {
                                interval_sec: heartbeat_interval_sec,
                                next_in_sec: heartbeat_next_in_sec,
                                size: 22,
                                show_label: false,
                            }
                            span { "{last_seen}" }
                        }
                    }
                    // Policy
                    div {
                        div {
                            class: "text-[11px] uppercase tracking-wider mb-1",
                            style: "color: var(--cf-text-muted); letter-spacing: 0.06em;",
                            "Policy"
                        }
                        div {
                            class: "text-xs",
                            style: "color: var(--cf-text-primary)",
                            "{system.deployment_policy}"
                        }
                    }
                }
            }

            // Card footer with status chips and deploy button
            div {
                class: "flex items-center justify-between gap-2 pt-3",
                style: "border-top: 1px solid var(--cf-divider)",
                div {
                    class: "flex gap-2 flex-wrap",
                    // Health chip
                    Chip {
                        variant: health_chip_variant(&system.health_status),
                        show_dot: true,
                        "{system.health_status.label()}"
                    }
                    // Deployment chip
                    Chip {
                        variant: deployment_chip_variant(&system.deployment_status),
                        show_dot: false,
                        "{system.deployment_status.label()}"
                    }
                    // CVE chips (if any critical)
                    if system.cve_counts.critical > 0 {
                        Chip {
                            variant: ChipVariant::Critical,
                            show_dot: false,
                            "{system.cve_counts.critical} crit"
                        }
                    }
                    if system.cve_counts.high > 0 {
                        Chip {
                            variant: ChipVariant::Warning,
                            show_dot: false,
                            "{system.cve_counts.high} high"
                        }
                    }
                }
                button {
                    class: "btn btn-subtle focus-ring",
                    style: "padding: 4px 10px; font-size: 12px;",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_deploy.call(());
                    },
                    svg {
                        class: "w-3 h-3",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path { d: "M5 12h14M12 5l7 7-7 7" }
                    }
                    " Deploy"
                }
            }
        }
    }
}
