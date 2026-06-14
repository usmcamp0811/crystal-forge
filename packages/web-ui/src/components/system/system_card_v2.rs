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
use crate::components::environments::{normalize_color_hex, with_alpha};
use crate::components::heartbeat_spinner::HeartbeatSpinner;
use crate::components::system::helpers::deployment_state_label;

/// Environment color configuration for badges and styling.
struct EnvColors {
    fg: String,
    bg: String,
    border: String,
}

fn env_colors(env_name: &str, environment_colors: &[(String, String)]) -> EnvColors {
    if let Some((_, color_hex)) = environment_colors
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(env_name))
    {
        let fg = normalize_color_hex(color_hex);
        return EnvColors {
            bg: with_alpha(&fg, 0.10),
            border: with_alpha(&fg, 0.25),
            fg,
        };
    }

    match env_name.to_lowercase().as_str() {
        "production" | "prod" => EnvColors {
            fg: "#f87171".to_string(),
            bg: "rgba(220,38,38,0.10)".to_string(),
            border: "rgba(248,113,113,0.25)".to_string(),
        },
        "staging" | "stage" => EnvColors {
            fg: "#fbbf24".to_string(),
            bg: "rgba(217,119,6,0.10)".to_string(),
            border: "rgba(251,191,36,0.25)".to_string(),
        },
        "dev" | "development" => EnvColors {
            fg: "#60a5fa".to_string(),
            bg: "rgba(37,99,235,0.10)".to_string(),
            border: "rgba(96,165,250,0.25)".to_string(),
        },
        "edge" => EnvColors {
            fg: "#2dd4bf".to_string(),
            bg: "rgba(15,118,110,0.12)".to_string(),
            border: "rgba(45,212,191,0.25)".to_string(),
        },
        "lab" => EnvColors {
            fg: "#a78bfa".to_string(),
            bg: "rgba(124,58,237,0.10)".to_string(),
            border: "rgba(167,139,250,0.25)".to_string(),
        },
        _ => EnvColors {
            fg: "#6b7280".to_string(),
            bg: "rgba(107,114,128,0.16)".to_string(),
            border: "rgba(107,114,128,0.25)".to_string(),
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

fn derived_fqdn(hostname: &str, environment: &str) -> String {
    format!("{}.{}.cf.internal", hostname, environment.to_lowercase())
}

/// Redesigned system card with modern styling and layout.
#[component]
pub fn SystemCardV2(
    system: SystemSummary,
    #[props(default = false)] compact: bool,
    #[props(default = false)] selected: bool,
    #[props(default)] environment_colors: Vec<(String, String)>,
    #[props(default)] flake_context: Vec<(i32, String, String, Option<String>)>,
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

    let env = env_colors(&environment, &environment_colors);
    let status_col = status_color(&system.health_status);

    // Get flake info from loaded flake context.
    let (flake_name, flake_branch, flake_commit) = system
        .flake_id
        .and_then(|id| {
            flake_context
                .iter()
                .find(|(flake_id, ..)| *flake_id == id)
                .map(|(_, name, branch, latest_commit)| {
                    (name.clone(), branch.clone(), latest_commit.clone())
                })
        })
        .unwrap_or_else(|| ("—".to_string(), "—".to_string(), None));
    let flake_with_branch = format!("{flake_name} · {flake_branch}");
    let flake_commit_short = flake_commit
        .as_deref()
        .map(|hash| hash.chars().take(8).collect::<String>())
        .unwrap_or_else(|| "—".to_string());

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
    let selected_class = if selected { " selected" } else { "" };

    rsx! {
        div {
            class: "sys-card{compact_class}{selected_class}",
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
                class: "sys-card-head",
                div {
                    class: "sys-title",
                    div {
                        class: "sys-hostname",
                        StatusDot {
                            color: status_col.to_string(),
                            large: false,
                        }
                        "{system.hostname}"
                    }
                    div {
                        class: "sys-fqdn",
                        {
                            // Prefer persisted operator-managed FQDN when set.
                            let display_fqdn = system.fqdn.clone()
                                .filter(|v| !v.trim().is_empty())
                                .unwrap_or_else(|| derived_fqdn(&system.hostname, &environment));
                            rsx! { "{display_fqdn}" }
                        }
                    }
                }
                EnvBadge {
                    name: environment.clone(),
                    fg: env.fg,
                    bg: env.bg,
                    border: env.border,
                }
            }

            // Card body with two-column metadata
            if !compact {
                div {
                    class: "sys-card-body",
                    // Flake · branch
                    div {
                        div { class: "sys-kv-key", "Flake · branch" }
                        div { class: "sys-kv-val", "{flake_with_branch}" }
                    }
                    // Commit
                    div {
                        div { class: "sys-kv-key", "Commit" }
                        div { class: "sys-kv-val", "{flake_commit_short}" }
                    }
                    // Heartbeat
                    div {
                        div { class: "sys-kv-key", "Heartbeat" }
                        div {
                            class: "sys-kv-val",
                            style: "font-family: var(--font-sans); display: flex; align-items: center; gap: 8px;",
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
                        div { class: "sys-kv-key", "Policy" }
                        div {
                            class: "sys-kv-val",
                            style: "font-family: var(--font-sans);",
                            "{system.deployment_policy}"
                        }
                    }
                }
            }

            // Compact body: condensed inline metadata
            if compact {
                div {
                    style: "display: flex; gap: 12px; font-size: 12px; color: var(--cf-text-secondary); flex-wrap: wrap;",
                    span {
                        class: "mono",
                        style: "color: var(--cf-text-primary);",
                        "{flake_name}"
                    }
                    span { class: "mono", "{flake_commit_short} · {flake_branch}" }
                    span { "{last_seen}" }
                }
            }

            // Card footer with status chips and deploy button
            div {
                class: "sys-card-foot",
                div {
                    class: "chips-row",
                    // Health chip
                    Chip {
                        variant: health_chip_variant(&system.health_status),
                        show_dot: true,
                        "{system.health_status.label()}"
                    }
                    // Deployment chip (design lowercase state labels)
                    Chip {
                        variant: deployment_chip_variant(&system.deployment_status),
                        show_dot: false,
                        "{deployment_state_label(&system.deployment_status)}"
                    }
                    // CVE chips: crit/high when present, otherwise a clean chip
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
                    if system.cve_counts.critical == 0 && system.cve_counts.high == 0 {
                        span { class: "chip chip-healthy", "✓ clean" }
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
                        path { d: "M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" }
                    }
                    " Deploy"
                }
            }
        }
    }
}
